//! Every MCP Registry request goes through here: a repeated one is answered from memory, a failing registry is asked
//! again only after a growing pause, and a token bucket caps what still goes out (a runaway caller once sent ~6 a
//! second for two days).

use crate::LockExt;
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

/// How long an answer is reused for the same URL.
const ANSWER_FRESH: Duration = Duration::from_mins(10);
/// Requests that may go out at once: opening the Marketplace cold costs 13 (12 featured servers and the first page).
const BURST: u32 = 30;
/// After the burst, one more request per this: at most `BURST` + 60 an hour, whatever calls.
const REFILL_EVERY: Duration = Duration::from_mins(1);
const FIRST_BACKOFF: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_mins(15);

pub(super) struct Gate {
    answers: HashMap<String, (Instant, Value)>,
    tokens: u32,
    refilled: Instant,
    backoff: Duration,
    /// No request goes out before this, after a failure.
    paused_until: Option<Instant>,
}

enum Pass {
    Cached(Value),
    Fetch,
}

impl Gate {
    fn new(now: Instant) -> Self {
        Self { answers: HashMap::new(), tokens: BURST, refilled: now, backoff: FIRST_BACKOFF, paused_until: None }
    }

    fn admit(&mut self, url: &str, now: Instant) -> Result<Pass> {
        self.answers.retain(|_, (at, _)| now.duration_since(*at) < ANSWER_FRESH);
        if let Some((_, v)) = self.answers.get(url) {
            return Ok(Pass::Cached(v.clone()));
        }
        if let Some(until) = self.paused_until.filter(|until| *until > now) {
            return Err(anyhow!("the MCP Registry isn't answering; trying again in {} s", (until - now).as_secs() + 1));
        }
        let refills =
            u32::try_from(now.duration_since(self.refilled).as_secs() / REFILL_EVERY.as_secs()).unwrap_or(BURST);
        if refills > 0 {
            self.tokens = self.tokens.saturating_add(refills).min(BURST);
            self.refilled = now;
        }
        if self.tokens == 0 {
            return Err(anyhow!("too many MCP Registry requests; try again in a minute"));
        }
        self.tokens -= 1;
        Ok(Pass::Fetch)
    }

    fn answered(&mut self, url: &str, result: &Result<Value>, now: Instant) {
        if let Ok(v) = result {
            self.answers.insert(url.to_owned(), (now, v.clone()));
            self.backoff = FIRST_BACKOFF;
            self.paused_until = None;
        } else {
            self.paused_until = Some(now + self.backoff);
            self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
        }
    }
}

/// `fetch(url)` unless the gate answers or refuses it first.
pub(super) async fn through(
    gate: &Mutex<Option<Gate>>,
    url: &str,
    fetch: impl AsyncFnOnce(&str) -> Result<Value>,
) -> Result<Value> {
    let pass = gate.locked().get_or_insert_with(|| Gate::new(Instant::now())).admit(url, Instant::now())?;
    if let Pass::Cached(v) = pass {
        return Ok(v);
    }
    let result = fetch(url).await;
    if let Some(g) = gate.locked().as_mut() {
        g.answered(url, &result, Instant::now());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn counting(sent: &AtomicU32, ok: bool) -> impl AsyncFn(&str) -> Result<Value> {
        async move |_: &str| {
            sent.fetch_add(1, Ordering::Relaxed);
            if ok { Ok(Value::Null) } else { Err(anyhow!("503")) }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_repeated_search_is_sent_once_per_ten_minutes() {
        let gate = Mutex::new(None);
        let sent = AtomicU32::new(0);
        let fetch = counting(&sent, true);
        for _ in 0..1000 {
            through(&gate, "search=github", &fetch).await.unwrap();
        }
        assert_eq!(sent.load(Ordering::Relaxed), 1);
        tokio::time::advance(ANSWER_FRESH).await;
        through(&gate, "search=github", &fetch).await.unwrap();
        assert_eq!(sent.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn distinct_requests_in_a_loop_stay_under_a_hundred_an_hour() {
        let gate = Mutex::new(None);
        let sent = AtomicU32::new(0);
        let fetch = counting(&sent, true);
        for second in 0..3600 {
            for n in 0..6 {
                let _ = through(&gate, &format!("cursor={second}-{n}"), &fetch).await;
            }
            tokio::time::advance(Duration::from_secs(1)).await;
        }
        assert_eq!(sent.load(Ordering::Relaxed), BURST + 59, "21,600 calls: the burst, then one a minute");
    }

    #[tokio::test(start_paused = true)]
    async fn a_failing_registry_is_asked_less_and_less_often() {
        let gate = Mutex::new(None);
        let sent = AtomicU32::new(0);
        let down = counting(&sent, false);
        for second in 0..3600 {
            assert!(through(&gate, &format!("page={second}"), &down).await.is_err());
            tokio::time::advance(Duration::from_secs(1)).await;
        }
        // Pauses of 30 s, 1, 2, 4, 8 min, then 15 min: 8 tries in the hour.
        assert_eq!(sent.load(Ordering::Relaxed), 8);
        let up = counting(&sent, true);
        tokio::time::advance(MAX_BACKOFF).await;
        through(&gate, "page=back", &up).await.unwrap();
        assert!(through(&gate, "page=next", &up).await.is_ok(), "an answer ends the pause");
    }
}
