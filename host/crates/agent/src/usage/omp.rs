//! `omp usage --json`: one window per upstream limit oh-my-pi reports.
//! Claude and Codex stay on their own cards; these windows belong to oh-my-pi,
//! labeled with the upstream, so a second login isn't written over the CLI's.
//!
//! A host whose own `omp` has no logins (they belong to another account on the computer) reads the
//! file `CODYNC_OMP_USAGE_FILE` names instead, which that account keeps current: either `omp usage --json`
//! as is, or a weekly summary of it, `{"providers": {"<upstream>": {"week_pct": [<% used per account>]}}}`.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::Window;

const SAVED_REPORT: &str = "CODYNC_OMP_USAGE_FILE";

pub async fn collect() -> Option<Vec<Window>> {
    match saved_report() {
        Some(path) => read_saved(&path).await,
        None => super::command::windows("omp", &["usage", "--json"], None, Duration::from_secs(30), parse).await,
    }
}

/// A saved report to read on a host without `omp` installed.
pub fn report_saved() -> bool {
    saved_report().is_some_and(|path| path.is_file())
}

fn saved_report() -> Option<PathBuf> {
    std::env::var_os(SAVED_REPORT).filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// The saved report's limits; a missing file or one without limits is no reading, and the last one stays.
async fn read_saved(path: &Path) -> Option<Vec<Window>> {
    let text = match tokio::fs::read_to_string(path).await {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "couldn't read the saved oh-my-pi usage report");
            return None;
        }
    };
    let mut windows = parse(&text);
    if windows.is_empty() {
        windows = serde_json::from_str(&text).map(|summary| weekly_summary(&summary)).unwrap_or_default();
    }
    (!windows.is_empty()).then_some(windows)
}

/// One weekly window per account of each upstream in a summary of `omp usage`.
fn weekly_summary(summary: &Value) -> Vec<Window> {
    let Some(providers) = summary["providers"].as_object() else { return Vec::new() };
    let mut windows = Vec::new();
    for (provider, reading) in providers {
        let Some(accounts) = reading["week_pct"].as_array() else { continue };
        let name = provider_name(provider);
        for (n, percent) in accounts.iter().enumerate() {
            let Some(percent) = percent.as_f64() else { continue };
            let label =
                if accounts.len() > 1 { format!("{name} Weekly (#{})", n + 1) } else { format!("{name} Weekly") };
            windows.push(Window::new(
                format!("{provider}:account-{}:7d:0", n + 1),
                label,
                percent.clamp(0.0, 100.0),
                None,
            ));
        }
    }
    windows
}

fn parse(text: &str) -> Vec<Window> {
    let Some(start) = text.find('{') else { return Vec::new() };
    let Some(end) = text.rfind('}') else { return Vec::new() };
    if end <= start {
        return Vec::new();
    }
    let Ok(payload) = serde_json::from_str::<Value>(&text[start..=end]) else { return Vec::new() };
    let Some(reports) = payload["reports"].as_array() else { return Vec::new() };
    let mut per_provider: HashMap<&str, usize> = HashMap::new();
    for report in reports {
        if let Some(id) = report["provider"].as_str() {
            *per_provider.entry(id).or_insert(0) += 1;
        }
    }
    let mut windows = Vec::new();
    for (index, report) in reports.iter().enumerate() {
        let Some(provider) = report["provider"].as_str() else { continue };
        let name = provider_name(provider);
        let suffix = (per_provider.get(provider).copied().unwrap_or(0) > 1).then(|| discriminator(report, index));
        let account = account_key(report, index);
        let mut pools = HashSet::new();
        let Some(limits) = report["limits"].as_array() else { continue };
        for (n, limit) in limits.iter().enumerate() {
            if let Some(pool) = limit["scope"]["sharedGroup"].as_str().filter(|s| !s.is_empty())
                && !pools.insert(pool.to_owned())
            {
                continue;
            }
            let Some(window) = one_limit(&name, suffix.as_deref(), provider, &account, n, limit) else { continue };
            windows.push(window);
        }
    }
    windows
}

fn one_limit(
    name: &str,
    suffix: Option<&str>,
    provider: &str,
    account: &str,
    n: usize,
    limit: &Value,
) -> Option<Window> {
    let amount = &limit["amount"];
    let window_id = limit["scope"]["windowId"].as_str().or_else(|| limit["window"]["id"].as_str()).unwrap_or("limit");
    let raw_label = limit["label"].as_str().map(str::trim).filter(|s| !s.is_empty());
    // The account goes in parentheses: the apps read the part after " · " as Claude's model name
    // and would show "5h · ada" and "Weekly · ada" both as "ada".
    let label = match (raw_label, suffix) {
        (Some(label), Some(suffix)) => format!("{label} ({suffix})"),
        (Some(label), None) => label.to_owned(),
        (None, Some(suffix)) => format!("{name} ({suffix})"),
        (None, None) => name.to_owned(),
    };
    let id = format!("{provider}:{account}:{window_id}:{n}");
    let resets = epoch_ms(&limit["window"]["resetsAt"]);
    if amount["unit"].as_str().is_some_and(|u| u.eq_ignore_ascii_case("usd")) && amount["limit"].is_null() {
        let spent = amount["used"].as_f64()?;
        return Some(Window::note(id, label, format!("${spent:.2} spent")));
    }
    let percent = used_percent(amount)?;
    Some(Window::new(id, label, percent, resets))
}

/// Used share, 0–100. Fractions are 0–1; `used`/`limit` is a count or dollars.
fn used_percent(amount: &Value) -> Option<f64> {
    if let Some(used) = amount["usedFraction"].as_f64() {
        return Some((used * 100.0).clamp(0.0, 100.0));
    }
    if let Some(left) = amount["remainingFraction"].as_f64() {
        return Some(((1.0 - left) * 100.0).clamp(0.0, 100.0));
    }
    let used = amount["used"].as_f64()?;
    let limit = amount["limit"].as_f64().filter(|n| *n > 0.0)?;
    Some((used / limit * 100.0).clamp(0.0, 100.0))
}

fn epoch_ms(v: &Value) -> Option<i64> {
    let n = v.as_i64().or_else(|| v.as_f64().and_then(super::finite_i64))?;
    Some(if n > 100_000_000_000 { n } else { n.saturating_mul(1000) })
}

fn account_key(report: &Value, index: usize) -> String {
    let raw = report["metadata"]["accountId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| report["metadata"]["email"].as_str().filter(|s| !s.is_empty()));
    let Some(raw) = raw else { return format!("account-{}", index + 1) };
    let key: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@') { c } else { '_' })
        .take(80)
        .collect();
    if key.is_empty() { format!("account-{}", index + 1) } else { key }
}

fn discriminator(report: &Value, index: usize) -> String {
    let email = report["metadata"]["email"].as_str().filter(|s| s.contains('@'));
    if let Some(email) = email {
        return email.split('@').next().unwrap_or(email).chars().take(16).collect();
    }
    report["metadata"]["accountId"].as_str().map_or_else(|| format!("#{}", index + 1), |s| s.chars().take(8).collect())
}

fn provider_name(id: &str) -> String {
    match id {
        "anthropic" => "Claude",
        "openai-codex" => "Codex",
        "zai" => "Z.ai",
        "google-gemini-cli" => "Gemini",
        "google-antigravity" => "Antigravity",
        "github-copilot" => "Copilot",
        "kimi-code" => "Kimi",
        "minimax-code" => "MiniMax",
        "minimax-code-cn" => "MiniMax CN",
        "opencode-go" => "OpenCode Go",
        "alibaba-token-plan" => "Qwen Cloud",
        "xai-oauth" => "Grok",
        other => return title_case(other),
    }
    .to_owned()
}

fn title_case(id: &str) -> String {
    id.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `model`'s provider's windows. A window's id starts `<provider>:`, the upstream's whole id (`kimi-code`,
/// `google-antigravity`) in a live report, a short name (`claude`, `kimi`) in a saved summary. It is the model's
/// when a part of the model id (`anthropic/claude-opus-5-5`, split at `/`) is that provider, or starts with it
/// as a whole word (`claude` of `claude-opus-5-5`, never `google` of `google-gemini-cli`).
/// `model` unset: every window counts. A provider the report doesn't name has no reading, so none counts.
pub fn of_model<'a>(windows: &'a [Window], model: Option<&str>) -> Vec<&'a Window> {
    let Some(model) = model.filter(|model| !model.is_empty()) else { return windows.iter().collect() };
    let parts: Vec<String> = model.split('/').map(str::to_lowercase).collect();
    let is_models = |provider: &str| {
        let provider = provider.to_lowercase();
        parts.iter().any(|part| {
            part.strip_prefix(provider.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
        })
    };
    windows.iter().filter(|window| is_models(window.id.split(':').next().unwrap_or_default())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LockExt;
    use crate::usage::{Provider, near_limit, services};

    #[test]
    fn parses_a_used_fraction_and_a_dollar_cap() {
        let text = r#"status
{"reports":[
  {"provider":"anthropic","limits":[{"label":"Claude 5 Hour","scope":{"windowId":"5h"},
     "window":{"id":"5h","resetsAt":1783885200000},
     "amount":{"usedFraction":0.08,"unit":"percent"}}]},
  {"provider":"xai-oauth","limits":[{"label":"Spend","scope":{"windowId":"monthly"},
     "amount":{"used":4.5,"limit":20,"unit":"usd"}}]}
]}"#;
        let w = parse(text);
        assert_eq!(w[0].label, "Claude 5 Hour");
        assert!((w[0].percent - 8.0).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some(1_783_885_200_000));
        assert!((w[1].percent - 22.5).abs() < 1e-9);
        assert_eq!(w[1].label, "Spend");
    }

    #[test]
    fn two_accounts_keep_separate_window_ids() {
        let text = r#"{"reports":[
          {"provider":"anthropic","metadata":{"email":"ada@x.com","accountId":"acct_ada"},
           "limits":[{"label":"5h","scope":{"windowId":"5h"},"amount":{"usedFraction":0.1}}]},
          {"provider":"anthropic","metadata":{"email":"grace@x.com","accountId":"acct_grace"},
           "limits":[{"label":"5h","scope":{"windowId":"5h"},"amount":{"usedFraction":0.2}}]}
        ]}"#;
        let w = parse(text);
        assert_eq!(w[0].id, "anthropic:acct_ada:5h:0");
        assert_eq!(w[1].id, "anthropic:acct_grace:5h:0");
        assert_eq!(w[0].label, "5h (ada)");
        assert_eq!(w[1].label, "5h (grace)");
    }

    #[test]
    fn an_uncapped_dollar_spend_is_a_note() {
        let text = r#"{"reports":[{"provider":"openai","limits":[
            {"label":"API","amount":{"used":3.2,"unit":"usd"}}]}]}"#;
        let w = parse(text);
        assert_eq!(w[0].text.as_deref(), Some("$3.20 spent"));
        assert!(w[0].percent.abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn a_saved_report_is_read_like_the_command() {
        let dir = std::env::temp_dir().join(format!("codync-omp-usage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("usage.json");
        std::fs::write(
            &file,
            r#"{"generatedAt":1,"reports":[{"provider":"kimi-code","limits":[{"label":"Weekly",
               "scope":{"windowId":"7d"},"amount":{"usedFraction":0.36}}]}]}"#,
        )
        .unwrap();
        let windows = read_saved(&file).await.expect("a reading");
        assert_eq!(windows[0].label, "Weekly");
        assert!((windows[0].percent - 36.0).abs() < 1e-9);

        std::fs::write(&file, r#"{"generatedAt":2,"reports":[]}"#).unwrap();
        assert!(read_saved(&file).await.is_none(), "a report without limits is no reading");
        assert!(read_saved(&dir.join("missing.json")).await.is_none(), "nor is a missing file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_saved_weekly_summary_gives_each_account_its_week() {
        let dir = std::env::temp_dir().join(format!("codync-omp-summary-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("summary.json");
        std::fs::write(
            &file,
            r#"{"week_pct": 100, "as_of": "2026-11-16T00:50:21+00:00", "providers": {
               "claude": {"week_pct": [88, 100], "fable_week_pct": [82, 100]},
               "kimi": {"week_pct": [37.5]}, "zai": {"week_pct": [99.8]}, "codex": {"week_pct": []}}}"#,
        )
        .unwrap();
        let windows = read_saved(&file).await.expect("a reading");
        let read: Vec<_> = windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.percent)).collect();
        assert_eq!(
            read,
            [
                ("claude:account-1:7d:0", "Claude Weekly (#1)", 88.0),
                ("claude:account-2:7d:0", "Claude Weekly (#2)", 100.0),
                ("kimi:account-1:7d:0", "Kimi Weekly", 37.5),
                ("zai:account-1:7d:0", "Z.ai Weekly", 99.8),
            ],
            "an upstream with no accounts gives no window"
        );

        std::fs::write(&file, r#"{"providers": {}}"#).unwrap();
        assert!(read_saved(&file).await.is_none(), "a summary without accounts is no reading");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_models_own_provider_holds_an_omp_bot_back() {
        // A weekly summary with Kimi used up and Claude at 28 and 50.
        let today = r#"{"providers": {"claude": {"week_pct": [28, 50]}, "kimi": {"week_pct": [100]},
            "zai": {"week_pct": [30.4]}, "devin": {"week_pct": [100, 19.0]}, "codex": {"week_pct": [3.0]}}}"#;
        let windows = weekly_summary(&serde_json::from_str(today).unwrap());
        let hub = services::tests::hub();
        hub.update_usage(|usage| usage.providers = vec![Provider::new("omp", "oh-my-pi", "test", windows)]);
        let bot = |model: Option<&str>| {
            serde_json::from_value(serde_json::json!({"id": "b", "name": "b", "backend": "omp", "model": model}))
                .unwrap()
        };

        assert!(!near_limit(&hub, &bot(Some("router/anthropic/claude-opus-5-5"))), "Kimi's full week isn't Claude's");
        assert!(near_limit(&hub, &bot(Some("router/kimi-code/k3.1-pro"))));
        assert!(near_limit(&hub, &bot(Some("router/devin/swe-2"))), "one account used up");
        assert!(!near_limit(&hub, &bot(Some("local/qwen-flash-next"))), "no plan window");
        assert!(near_limit(&hub, &bot(None)), "no model: every window counts");
        assert_eq!(hub.usage.locked().visible().providers[0].windows.len(), 7, "the widget shows every provider");
    }

    #[test]
    fn a_live_reports_provider_ids_match_the_model() {
        let live = [
            Window::new("anthropic:acct:5h:0", "5h", 80.0, None),
            Window::new("google-antigravity:acct:d:0", "Day", 100.0, None),
            Window::new("google-gemini-cli:acct:d:0", "Day", 10.0, None),
        ];
        let ids = |model| of_model(&live, Some(model)).iter().map(|w| w.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids("anthropic/claude-opus-5-5"), ["anthropic:acct:5h:0"]);
        assert_eq!(ids("google-antigravity/gemini-3-pro"), ["google-antigravity:acct:d:0"]);
        assert_eq!(ids("google-gemini-cli/gemini-3-pro"), ["google-gemini-cli:acct:d:0"]);
        assert!(ids("openai-codex/gpt-6-astra").is_empty(), "the report names no codex");
    }
}
