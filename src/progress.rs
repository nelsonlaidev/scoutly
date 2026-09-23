use std::future::Future;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use url::Url;

use crate::AuditError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Robots,
    Crawl,
    Sitemaps,
    Links,
    Images,
    Report,
}

impl Phase {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Robots => "robots",
            Self::Crawl => "crawl",
            Self::Sitemaps => "sitemaps",
            Self::Links => "links",
            Self::Images => "images",
            Self::Report => "report",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PageProgress {
    pub discovered: usize,
    pub crawled: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SitemapProgress {
    pub fetched: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceProgress {
    pub checked: usize,
    pub total: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Progress {
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current_url: String,
    pub pages: PageProgress,
    pub sitemaps: SitemapProgress,
    pub links: ResourceProgress,
    pub images: ResourceProgress,
}

pub(crate) struct ProgressReporter {
    sender: Option<mpsc::Sender<Progress>>,
    state: Progress,
}

pub(crate) enum PhaseStart<'a> {
    Robots(&'a Url),
    Crawl(&'a Url),
    Sitemaps,
    Links { total: usize },
    Images { total: usize },
    Report,
}

impl ProgressReporter {
    pub(crate) fn new(sender: Option<mpsc::Sender<Progress>>) -> Self {
        Self {
            sender,
            state: Progress {
                phase: Phase::Robots,
                current_url: String::new(),
                pages: PageProgress::default(),
                sitemaps: SitemapProgress::default(),
                links: ResourceProgress::default(),
                images: ResourceProgress::default(),
            },
        }
    }

    pub(crate) async fn start_phase(&mut self, start: PhaseStart<'_>) -> Result<(), AuditError> {
        match start {
            PhaseStart::Robots(url) => {
                self.state.phase = Phase::Robots;
                self.state.current_url = url.to_string();
            }
            PhaseStart::Crawl(url) => {
                self.state.phase = Phase::Crawl;
                self.state.current_url = url.to_string();
            }
            PhaseStart::Sitemaps => {
                self.state.phase = Phase::Sitemaps;
                self.state.current_url.clear();
            }
            PhaseStart::Links { total } => {
                self.state.phase = Phase::Links;
                self.state.current_url.clear();
                self.state.links.total = Some(total);
            }
            PhaseStart::Images { total } => {
                self.state.phase = Phase::Images;
                self.state.current_url.clear();
                self.state.images.total = Some(total);
            }
            PhaseStart::Report => {
                self.state.phase = Phase::Report;
                self.state.current_url.clear();
            }
        }

        self.emit().await
    }

    pub(crate) async fn page_discovered(&mut self, url: &Url) -> Result<(), AuditError> {
        self.state.pages.discovered += 1;
        self.state.current_url = url.to_string();

        self.emit().await
    }

    pub(crate) async fn page_crawled(&mut self, url: &Url) -> Result<(), AuditError> {
        self.state.pages.crawled += 1;
        self.state.current_url = url.to_string();

        self.emit().await
    }

    pub(crate) async fn sitemap_fetched(&mut self, url: &Url) -> Result<(), AuditError> {
        self.state.sitemaps.fetched += 1;
        self.state.current_url = url.to_string();

        self.emit().await
    }

    pub(crate) async fn link_checked(&mut self, url: &str) -> Result<(), AuditError> {
        self.state.links.checked += 1;
        self.state.current_url = url.to_owned();

        self.emit().await
    }

    pub(crate) async fn image_checked(&mut self, url: &str) -> Result<(), AuditError> {
        self.state.images.checked += 1;
        self.state.current_url = url.to_owned();

        self.emit().await
    }

    pub(crate) async fn with_receiver_open<F, T>(&self, future: F) -> Result<T, AuditError>
    where
        F: Future<Output = T>,
    {
        let Some(sender) = &self.sender else {
            return Ok(future.await);
        };

        tokio::select! {
            biased;
            () = sender.closed() => Err(AuditError::ProgressClosed),
            output = future => Ok(output),
        }
    }

    pub(crate) fn ensure_receiver_open(&self) -> Result<(), AuditError> {
        if self.is_closed() {
            Err(AuditError::ProgressClosed)
        } else {
            Ok(())
        }
    }

    async fn emit(&self) -> Result<(), AuditError> {
        if self.is_closed() {
            return Err(AuditError::ProgressClosed);
        }

        let Some(sender) = &self.sender else {
            return Ok(());
        };

        sender
            .send(self.state.clone())
            .await
            .map_err(|_| AuditError::ProgressClosed)
    }

    fn is_closed(&self) -> bool {
        self.sender.as_ref().is_some_and(mpsc::Sender::is_closed)
    }
}

#[cfg(test)]
mod tests {
    use super::Phase;

    #[test]
    fn display_names_match_serialized_enum_values() {
        for phase in [
            Phase::Robots,
            Phase::Crawl,
            Phase::Sitemaps,
            Phase::Links,
            Phase::Images,
            Phase::Report,
        ] {
            assert_eq!(
                serde_json::to_string(&phase).unwrap(),
                format!("\"{}\"", phase.as_str())
            );
        }
    }
}
