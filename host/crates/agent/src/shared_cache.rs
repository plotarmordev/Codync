//! Public JSON documents and small shared state kept in the user's cache folder for every host they run (a service, dev
//! builds, test runs that each start their own), so those hosts fetch a document once per window between them, and
//! limit what they send together, instead of each on its own.

use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::fs::{File, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

/// The folder the hosts share.
#[derive(Clone)]
pub struct Shared {
    dir: PathBuf,
}

impl Shared {
    /// The user's cache folder for Codync; without one (and in this crate's own tests), the host's private data folder,
    /// never a temporary folder other users can write to.
    pub fn computer() -> &'static Self {
        static SHARED: LazyLock<Shared> = LazyLock::new(|| {
            let cache = dirs::cache_dir().filter(|_| !cfg!(test));
            Shared::at(cache.map_or_else(|| super::setup::get().data_dir.join("cache"), |dir| dir.join("codync")))
        });
        &SHARED
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Reads `name`, lets `change` change it, and saves it, while no other host can: one host's change is never lost to
    /// another's. Waits for a host that holds it (they hold it only to read and save a few bytes).
    pub fn update<T: Default + Serialize + DeserializeOwned, R>(
        &self,
        name: &str,
        change: impl FnOnce(&mut T) -> R,
    ) -> std::io::Result<R> {
        let lock = self.lock(name)?;
        lock.lock()?;
        let file = self.dir.join(name);
        let mut state = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let answer = change(&mut state);
        write_at(&file, &serde_json::to_vec(&state)?, SystemTime::now())?;
        Ok(answer)
    }

    fn lock(&self, name: &str) -> std::io::Result<File> {
        std::fs::create_dir_all(&self.dir)?;
        File::options().create(true).truncate(false).write(true).open(self.dir.join(format!("{name}.lock")))
    }
}

/// One document in the shared folder.
pub struct Document {
    pub name: &'static str,
    /// A copy younger than this is used without asking upstream.
    pub fresh_for: Duration,
    /// After any attempt (by any host), the next waits this long: a failing upstream is asked once per window.
    pub retry_after: Duration,
}

impl Document {
    /// The shared copy, however old.
    pub fn read(&self, shared: &Shared) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(shared.dir.join(self.name)).ok()?).ok()
    }

    /// Fetches a new copy when the shared one is stale, no host tried within `retry_after` and no host is fetching it
    /// now. Answers whether it fetched one.
    pub async fn refresh(&self, shared: &Shared, fetch: impl AsyncFnOnce() -> Result<Value>) -> bool {
        self.refresh_at(shared, SystemTime::now(), fetch).await
    }

    async fn refresh_at(&self, shared: &Shared, now: SystemTime, fetch: impl AsyncFnOnce() -> Result<Value>) -> bool {
        let file = shared.dir.join(self.name);
        let attempt = shared.dir.join(format!("{}.attempt", self.name));
        let due = || {
            age(&file, now).is_none_or(|a| a >= self.fresh_for)
                && age(&attempt, now).is_none_or(|a| a >= self.retry_after)
        };
        if !due() {
            return false;
        }
        // Held through the fetch: a host that finds it held leaves the fetch to that one.
        let lock = match shared.lock(self.name) {
            Ok(lock) => lock,
            Err(error) => {
                tracing::warn!(%error, name = self.name, "couldn't open the shared cache");
                return false;
            }
        };
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Error(error)) => {
                tracing::warn!(%error, name = self.name, "couldn't lock the shared cache");
                return false;
            }
        }
        if !due() {
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

fn age(path: &Path, now: SystemTime) -> Option<Duration> {
    now.duration_since(std::fs::metadata(path).and_then(|m| m.modified()).ok()?).ok()
}

/// Written beside `path` and renamed over it, so a reader without the lock sees the old copy or the new one, never
/// half of one. Callers hold the lock, so the one partial file per name is theirs: one left by a crash is removed, and
/// the new one is created afresh, so a link put in its place is never followed.
fn write_at(path: &Path, bytes: &[u8], now: SystemTime) -> std::io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    match std::fs::remove_file(&partial) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut file = File::options().write(true).create_new(true).open(&partial)?;
    file.write_all(bytes)?;
    file.set_modified(now)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Barrier};

    const DOC: Document =
        Document { name: "doc.json", fresh_for: Duration::from_hours(1), retry_after: Duration::from_mins(10) };

    fn empty(name: &str) -> Shared {
        let dir = std::env::temp_dir().join(format!("codync-shared-cache-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Shared::at(dir)
    }

    /// `hosts` threads, each with its own runtime and file handles, start refreshing at the same instant.
    fn hosts_at_once(shared: &Arc<Shared>, hosts: usize, ok: bool) -> u32 {
        let fetches = Arc::new(AtomicU32::new(0));
        let start = Arc::new(Barrier::new(hosts));
        let threads: Vec<_> = (0..hosts)
            .map(|_| {
                let (shared, fetches, start) = (shared.clone(), fetches.clone(), start.clone());
                std::thread::spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
                    start.wait();
                    runtime.block_on(DOC.refresh(&shared, async || {
                        fetches.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        if ok { Ok(json!({"v": 1})) } else { anyhow::bail!("503") }
                    }))
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        fetches.load(Ordering::Relaxed)
    }

    #[test]
    fn hosts_starting_together_fetch_once() {
        let shared = Arc::new(empty("together"));
        assert_eq!(hosts_at_once(&shared, 16, true), 1);
        assert_eq!(DOC.read(&shared), Some(json!({"v": 1})));
        assert_eq!(hosts_at_once(&shared, 16, true), 0, "a fresh copy is used as it is");
    }

    #[tokio::test]
    async fn a_stale_copy_is_fetched_again() {
        let shared = empty("stale");
        let now = SystemTime::now();
        assert!(DOC.refresh_at(&shared, now, async || Ok(json!(1))).await);
        assert!(!DOC.refresh_at(&shared, now + Duration::from_mins(59), async || Ok(json!(2))).await);
        assert!(DOC.refresh_at(&shared, now + Duration::from_mins(61), async || Ok(json!(3))).await);
        assert_eq!(DOC.read(&shared), Some(json!(3)));
    }

    #[tokio::test]
    async fn a_failing_upstream_is_asked_once_per_retry_window_by_all_hosts() {
        let shared = Arc::new(empty("down"));
        assert_eq!(hosts_at_once(&shared, 16, false), 1);
        let fails = AtomicU32::new(0);
        let start = SystemTime::now();
        for minute in 0..60 {
            for _ in 0..100 {
                DOC.refresh_at(&shared, start + Duration::from_mins(minute), async || {
                    fails.fetch_add(1, Ordering::Relaxed);
                    anyhow::bail!("503")
                })
                .await;
            }
        }
        assert_eq!(fails.load(Ordering::Relaxed), 5, "after the first attempt, one per 10 minutes");
        assert!(DOC.read(&shared).is_none());
    }

    #[test]
    fn concurrent_updates_are_never_lost() {
        let shared = Arc::new(empty("update"));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let shared = shared.clone();
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        shared.update("count.json", |n: &mut u32| *n += 1).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(shared.update("count.json", |n: &mut u32| *n).unwrap(), 400);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_place_of_the_partial_file_is_never_written_through() {
        let shared = empty("link");
        std::fs::create_dir_all(&shared.dir).unwrap();
        let target = shared.dir.join("elsewhere");
        std::fs::write(&target, "kept").unwrap();
        std::os::unix::fs::symlink(&target, shared.dir.join("count.json.partial")).unwrap();
        shared.update("count.json", |n: &mut u32| *n += 1).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept");
        assert_eq!(shared.update("count.json", |n: &mut u32| *n).unwrap(), 1);
    }
}
