//! Every MCP Registry request goes through here: a repeated one is answered from memory, and a budget shared by every
//! host of the user (a token bucket, and a growing pause while the registry is down) caps what still goes out (runaway
//! hosts once sent ~6 a second for two days).

use crate::LockExt;
use anyhow::{Context, Result, bail};
use codync_agent::shared_cache::{Shared, registries_off};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long an answer is reused for the same URL.
const ANSWER_FRESH: Duration = Duration::from_mins(10);
/// Requests that may go out at once: opening the Marketplace cold costs 13 (12 featured servers and the first page).
const BURST: u32 = 30;
/// After the burst, one more request per this: at most `BURST` + 60 an hour, whatever and however many hosts call.
const REFILL_EVERY: Duration = Duration::from_mins(1);
const FIRST_BACKOFF: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_mins(15);
const BUDGET: &str = "mcp-registry-budget.json";

/// The shared budget; times are seconds since the Unix epoch.
#[derive(Serialize, Deserialize)]
struct Budget {
    tokens: u32,
    refilled: u64,
    backoff: u64,
    /// No request goes out before this, after an outage.
    paused_until: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self { tokens: BURST, refilled: 0, backoff: FIRST_BACKOFF.as_secs(), paused_until: 0 }
    }
}

impl Budget {
    fn spend(&mut self, now: u64) -> Result<()> {
        if self.paused_until > now {
            bail!("the MCP Registry isn't answering; trying again in {} s", self.paused_until - now);
        }
        let refills = now.saturating_sub(self.refilled) / REFILL_EVERY.as_secs();
        if refills > 0 {
            self.tokens = u32::try_from(refills).unwrap_or(BURST).saturating_add(self.tokens).min(BURST);
            self.refilled = now;
        }
        if self.tokens == 0 {
            bail!("too many MCP Registry requests; try again in a minute");
        }
        self.tokens -= 1;
        Ok(())
    }

    fn answered(&mut self, outage: bool, now: u64) {
        if outage {
            self.paused_until = now + self.backoff;
            self.backoff = (self.backoff * 2).min(MAX_BACKOFF.as_secs());
        } else {
            self.backoff = FIRST_BACKOFF.as_secs();
            self.paused_until = 0;
        }
    }
}

pub(super) struct Gate {
    shared: Shared,
    answers: Mutex<HashMap<String, (SystemTime, Value)>>,
}

impl Gate {
    pub(super) fn new(shared: Shared) -> Self {
        Self { shared, answers: Mutex::default() }
    }

    /// `fetch(url)` unless the gate answers or refuses it first.
    pub(super) async fn through(&self, url: &str, fetch: impl AsyncFnOnce(&str) -> Result<Value>) -> Result<Value> {
        if registries_off() {
            bail!("the MCP Registry is off on this host (CODYNC_REGISTRIES=off)");
        }
        self.through_at(url, SystemTime::now, fetch).await
    }

    async fn through_at(
        &self,
        url: &str,
        clock: impl Fn() -> SystemTime,
        fetch: impl AsyncFnOnce(&str) -> Result<Value>,
    ) -> Result<Value> {
        if let Some(v) = self.answer(url, clock()) {
            return Ok(v);
        }
        self.shared
            .update(BUDGET, |b: &mut Budget| b.spend(secs(clock())))
            .context("couldn't read the MCP Registry budget")??;
        let result = fetch(url).await;
        let outage = result.as_ref().err().is_some_and(is_outage);
        if let Err(error) = self.shared.update(BUDGET, |b: &mut Budget| b.answered(outage, secs(clock()))) {
            tracing::warn!(%error, "couldn't save the MCP Registry budget");
        }
        if let Ok(v) = &result {
            self.answers.locked().insert(url.to_owned(), (clock(), v.clone()));
        }
        result
    }

    fn answer(&self, url: &str, now: SystemTime) -> Option<Value> {
        let mut answers = self.answers.locked();
        answers.retain(|_, (at, _)| now.duration_since(*at).is_ok_and(|age| age < ANSWER_FRESH));
        answers.get(url).map(|(_, v)| v.clone())
    }
}

/// No answer, a server error or "slow down" pause every request; a refusal or bad body for one URL (a name the registry
/// doesn't list) fails only that one.
fn is_outage(error: &anyhow::Error) -> bool {
    error.downcast_ref::<reqwest::Error>().is_some_and(|e| match e.status() {
        Some(s) => s.is_server_error() || s == StatusCode::TOO_MANY_REQUESTS,
        None => !e.is_decode(),
    })
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn gate(name: &str) -> Gate {
        let dir = std::env::temp_dir().join(format!("codync-registry-gate-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Gate::new(Shared::at(dir))
    }

    fn answering(sent: &AtomicU32, status: u16, body: &'static str) -> impl AsyncFn(&str) -> Result<Value> {
        async move |_: &str| {
            sent.fetch_add(1, Ordering::Relaxed);
            let res = reqwest::Response::from(axum::http::Response::builder().status(status).body(body).unwrap());
            Ok(res.error_for_status()?.json().await?)
        }
    }

    struct Clock(Cell<SystemTime>);

    impl Clock {
        fn new() -> Self {
            Self(Cell::new(SystemTime::now()))
        }
        fn now(&self) -> SystemTime {
            self.0.get()
        }
        fn advance(&self, by: Duration) {
            self.0.set(self.0.get() + by);
        }
    }

    #[tokio::test]
    async fn a_repeated_search_is_sent_once_per_ten_minutes() {
        let (gate, clock, sent) = (gate("repeat"), Clock::new(), AtomicU32::new(0));
        let fetch = answering(&sent, 200, "null");
        for _ in 0..1000 {
            gate.through_at("search=github", || clock.now(), &fetch).await.unwrap();
        }
        assert_eq!(sent.load(Ordering::Relaxed), 1);
        clock.advance(ANSWER_FRESH);
        gate.through_at("search=github", || clock.now(), &fetch).await.unwrap();
        assert_eq!(sent.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn two_hosts_in_a_loop_stay_under_a_hundred_an_hour_together() {
        let (a, clock, sent) = (gate("loop"), Clock::new(), AtomicU32::new(0));
        let b = Gate::new(a.shared.clone());
        let fetch = answering(&sent, 200, "null");
        for second in 0..3600 {
            for n in 0..3 {
                for (host, gate) in [("a", &a), ("b", &b)] {
                    let _ = gate.through_at(&format!("cursor={host}{second}-{n}"), || clock.now(), &fetch).await;
                }
            }
            clock.advance(Duration::from_secs(1));
        }
        assert_eq!(sent.load(Ordering::Relaxed), BURST + 59, "21,600 calls: the burst, then one a minute");
    }

    #[tokio::test]
    async fn a_failing_registry_is_asked_less_and_less_often() {
        let (gate, clock, sent) = (gate("down"), Clock::new(), AtomicU32::new(0));
        let down = answering(&sent, 503, "null");
        for second in 0..3600 {
            assert!(gate.through_at(&format!("page={second}"), || clock.now(), &down).await.is_err());
            clock.advance(Duration::from_secs(1));
        }
        // Pauses of 30 s, 1, 2, 4, 8 min, then 15 min: 8 tries in the hour.
        assert_eq!(sent.load(Ordering::Relaxed), 8);
        let up = answering(&sent, 200, "null");
        clock.advance(MAX_BACKOFF);
        gate.through_at("page=back", || clock.now(), &up).await.unwrap();
        assert!(gate.through_at("page=next", || clock.now(), &up).await.is_ok(), "an answer ends the pause");
    }

    #[tokio::test]
    async fn a_name_the_registry_doesnt_list_delays_no_other_request() {
        let (gate, clock, sent) = (gate("missing"), Clock::new(), AtomicU32::new(0));
        let (missing, garbled) = (answering(&sent, 404, "null"), answering(&sent, 200, "<html>"));
        let listed = answering(&sent, 200, "null");
        for _ in 0..5 {
            assert!(gate.through_at("servers/typo", || clock.now(), &missing).await.is_err());
            assert!(gate.through_at("servers/odd", || clock.now(), &garbled).await.is_err());
            assert!(gate.through_at("servers/github", || clock.now(), &listed).await.is_ok());
        }
        assert_eq!(sent.load(Ordering::Relaxed), 11, "each failure is asked again; the listed name is reused");
    }
}
