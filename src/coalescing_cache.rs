use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use tokio::sync::OnceCell;

/// Stores one asynchronously initialized value per key. Initialization runs in
/// the caller's task, so dropping the last caller also drops in-flight work.
/// Entries live for the lifetime of the cache and are never evicted.
#[derive(Debug)]
pub(crate) struct CoalescingCache<K, V> {
    entries: Mutex<HashMap<K, Arc<OnceCell<V>>>>,
}

impl<K, V> Default for CoalescingCache<K, V> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<K, V> CoalescingCache<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    /// Returns the initialized value without waiting for in-flight work.
    ///
    /// `None` means that the key is absent or its initializer has not completed.
    #[must_use]
    pub(crate) fn get_if_ready(&self, key: &K) -> Option<V> {
        let cell = Arc::clone(
            self.entries
                .lock()
                .expect("cache mutex poisoned")
                .get(key)?,
        );

        cell.get().cloned()
    }

    pub(crate) async fn get_or_init<F, Fut>(&self, key: K, initialize: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = V>,
    {
        let cell = self.cell(key);
        cell.get_or_init(initialize).await.clone()
    }

    pub(crate) async fn get_or_try_init<F, Fut, E>(&self, key: K, initialize: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        let cell = self.cell(key);
        cell.get_or_try_init(initialize).await.cloned()
    }

    fn cell(&self, key: K) -> Arc<OnceCell<V>> {
        Arc::clone(
            self.entries
                .lock()
                .expect("cache mutex poisoned")
                .entry(key)
                .or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::sync::Notify;

    use super::CoalescingCache;

    #[tokio::test]
    async fn concurrent_and_later_calls_share_one_value() {
        let cache = Arc::new(CoalescingCache::default());
        let initializations = Arc::new(AtomicUsize::new(0));

        let first_cache = Arc::clone(&cache);
        let first_count = Arc::clone(&initializations);
        let first = tokio::spawn(async move {
            first_cache
                .get_or_init("resource", || async move {
                    first_count.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    7
                })
                .await
        });

        tokio::task::yield_now().await;

        let second_cache = Arc::clone(&cache);
        let second_count = Arc::clone(&initializations);
        let second = tokio::spawn(async move {
            second_cache
                .get_or_init("resource", || async move {
                    second_count.fetch_add(1, Ordering::SeqCst);
                    9
                })
                .await
        });

        assert_eq!(first.await.unwrap(), 7);
        assert_eq!(second.await.unwrap(), 7);
        assert_eq!(cache.get_or_init("resource", || async { 11 }).await, 7);
        assert_eq!(initializations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn canceled_initializer_allows_a_waiter_to_retry_the_key() {
        let cache = Arc::new(CoalescingCache::default());
        let started = Arc::new(Notify::new());
        let task_cache = Arc::clone(&cache);
        let task_started = Arc::clone(&started);
        let task = tokio::spawn(async move {
            task_cache
                .get_or_init("resource", || async move {
                    task_started.notify_one();
                    std::future::pending::<usize>().await
                })
                .await
        });

        started.notified().await;

        let waiting_cache = Arc::clone(&cache);
        let waiting =
            tokio::spawn(
                async move { waiting_cache.get_or_init("resource", || async { 13 }).await },
            );

        tokio::task::yield_now().await;

        task.abort();

        assert!(task.await.unwrap_err().is_cancelled());

        assert_eq!(waiting.await.unwrap(), 13);
        assert_eq!(cache.get_or_init("resource", || async { 17 }).await, 13);
    }

    #[tokio::test]
    async fn failed_initialization_is_not_cached() {
        let cache = CoalescingCache::default();

        assert_eq!(
            cache
                .get_or_try_init("origin", || async { Err::<usize, _>("failed") })
                .await,
            Err("failed")
        );
        assert_eq!(cache.get_if_ready(&"origin"), None);
        assert_eq!(
            cache
                .get_or_try_init("origin", || async { Ok::<_, &str>(23) })
                .await,
            Ok(23)
        );
        assert_eq!(cache.get_if_ready(&"origin"), Some(23));
    }
}
