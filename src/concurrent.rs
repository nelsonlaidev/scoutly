use std::future::Future;
use std::sync::Arc;

use tokio::task::{JoinError, JoinSet};

/// Runs at most `concurrency` jobs at once and restores input order after
/// completion. Dropping this future drops the JoinSet and aborts in-flight work.
///
/// # Panics
///
/// Panics when `concurrency` is zero. Callers derive this value from validated
/// [`crate::Options`].
pub(crate) async fn run_ordered<T, R, F, Fut>(
    inputs: Vec<T>,
    concurrency: usize,
    work: F,
) -> Result<Vec<R>, JoinError>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    assert!(concurrency > 0, "ordered concurrency must be positive");

    let total = inputs.len();
    let mut results = (0..total).map(|_| None).collect::<Vec<_>>();
    let mut inputs = inputs.into_iter().enumerate();
    let work = Arc::new(work);
    let mut tasks = JoinSet::new();

    for _ in 0..concurrency.min(total) {
        spawn_next(&mut tasks, &mut inputs, &work);
    }

    while let Some(completed) = tasks.join_next().await {
        let (index, output) = completed?;
        results[index] = Some(output);
        spawn_next(&mut tasks, &mut inputs, &work);
    }

    Ok(results
        .into_iter()
        .map(|result| result.expect("every ordered task produced a result"))
        .collect())
}

fn spawn_next<T, R, F, Fut>(
    tasks: &mut JoinSet<(usize, R)>,
    inputs: &mut impl Iterator<Item = (usize, T)>,
    work: &Arc<F>,
) where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    let Some((index, input)) = inputs.next() else {
        return;
    };
    let work = Arc::clone(work);
    tasks.spawn(async move { (index, work(input).await) });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::run_ordered;

    #[tokio::test]
    async fn bounds_concurrency_and_restores_input_order() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let results = run_ordered(vec![30_u64, 5, 20, 1], 2, {
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);

            move |delay| {
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                async move {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    delay
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(results, [30, 5, 20, 1]);
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
}
