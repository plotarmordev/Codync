//! A public JSON document kept on disk for every host on this computer, so many hosts (a service, dev builds, test
//! runs that each start their own) fetch it once per window between them instead of once each.

use anyhow::Result;
use serde_json::Value;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// One document under the computer's cache folder.
pub struct Document {
    /// The file name under the cache folder.
    pub name: &'static str,
    /// A copy younger than this is used without asking upstream.
    pub fresh_for: Duration,
    /// After any attempt (by any host), the next waits this long: a failing upstream is asked once per window.
    pub retry_after: Duration,
}

impl Document {
    /// The shared copy, however old.
    pub fn read(&self) -> Option<Value> {
        self.read_in(&dir())
    }

    /// Fetches a new copy when the shared one is stale and no host tried within `retry_after`. Answers whether it
    /// fetched one.
    pub async fn refresh(&self, fetch: impl AsyncFnOnce() -> Result<Value>) -> bool {
        self.refresh_in(&dir(), SystemTime::now(), fetch).await
    }

    fn read_in(&self, dir: &Path) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(dir.join(self.name)).ok()?).ok()
    }

    async fn refresh_in(&self, dir: &Path, now: SystemTime, fetch: impl AsyncFnOnce() -> Result<Value>) -> bool {
        let file = dir.join(self.name);
        let attempt = dir.join(format!("{}.attempt", self.name));
        if age(&file, now).is_some_and(|a| a < self.fresh_for)
            || age(&attempt, now).is_some_and(|a| a < self.retry_after)
        {
            return false;
        }
        if let Err(error) = write_at(&attempt, b"", now) {
            tracing::warn!(%error, name = self.name, "couldn't mark a shared cache fetch");
        }
        match fetch().await {
            Ok(v) => {
                if let Err(error) = write_at(&file, v.to_string().as_bytes(), now) {
                    tracing::warn!(%error, name = self.name, "couldn't save a shared cache copy");
                }
                true
            }
            Err(error) => {
                tracing::info!(error = format!("{error:#}"), name = self.name, "fetch failed; keeping the shared copy");
                false
            }
        }
    }
}

/// The computer's cache folder for Codync: shared by every host, whatever its data folder.
fn dir() -> PathBuf {
    if cfg!(test) {
        return super::setup::get().data_dir.join("cache");
    }
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("codync")
}

fn age(path: &Path, now: SystemTime) -> Option<Duration> {
    now.duration_since(std::fs::metadata(path).and_then(|m| m.modified()).ok()?).ok()
}

/// Written beside `path` and renamed over it, so another host reads the old copy or the new one, never half of one.
fn write_at(path: &Path, bytes: &[u8], now: SystemTime) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(path))?;
    let partial = path.with_extension(format!("{}.partial", std::process::id()));
    std::fs::write(&partial, bytes)?;
    File::options().write(true).open(&partial)?.set_modified(now)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    const DOC: Document =
        Document { name: "doc.json", fresh_for: Duration::from_hours(1), retry_after: Duration::from_mins(10) };

    fn empty_dir(name: &str) -> PathBuf {
        let dir = super::super::setup::get().data_dir.join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn hosts_sharing_the_folder_fetch_once_per_window() {
        let dir = empty_dir("shared-cache-ok");
        let fetches = Cell::new(0);
        let ok = async || {
            fetches.set(fetches.get() + 1);
            Ok(json!({"v": 1}))
        };
        let now = SystemTime::now();
        for _ in 0..50 {
            DOC.refresh_in(&dir, now, ok).await;
        }
        assert_eq!(fetches.get(), 1, "50 hosts starting together share one copy");
        assert_eq!(DOC.read_in(&dir), Some(json!({"v": 1})));
        assert!(DOC.refresh_in(&dir, now + Duration::from_mins(90), ok).await, "a stale copy is fetched again");
        assert_eq!(fetches.get(), 2);
    }

    #[tokio::test]
    async fn a_failing_upstream_is_asked_once_per_retry_window_by_all_hosts() {
        let dir = empty_dir("shared-cache-down");
        let fails = Cell::new(0);
        let down = async || {
            fails.set(fails.get() + 1);
            anyhow::bail!("503")
        };
        let start = SystemTime::now();
        for minute in 0..60 {
            for _ in 0..100 {
                DOC.refresh_in(&dir, start + Duration::from_mins(minute), down).await;
            }
        }
        assert_eq!(fails.get(), 6, "6,000 host starts in an hour, one attempt per 10 minutes");
        assert!(DOC.read_in(&dir).is_none());
    }
}
