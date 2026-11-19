//! `bots` through a real host: the roster the event stream carries, behind the bearer token.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};

/// A bot's id, name and status.
fn summary(bot: &Value) -> [&Value; 3] {
    [&bot["id"], &bot["name"], &bot["status"]]
}

#[tokio::test]
async fn bots_lists_the_roster_the_event_stream_carries() {
    let host = Host::start().await;
    let (a, b) = (host.bot("A", "auto").await, host.bot("B", "auto").await);
    let group = host.call("createBot", json!({"name": "G", "kind": "group", "members": [a, b]})).await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let gone = host.bot("Gone", "auto").await;
    host.call("deleteBot", json!({"botId": gone})).await;

    let snapshot = host.snapshot().await;
    let listed = host.call("bots", json!({})).await;
    let listed = listed["bots"].as_array().unwrap();
    // The lead bot is made in the background at start, so it may be listed without being in the snapshot yet.
    for bot in listed.iter().filter(|bot| !bot["lead"].as_bool().unwrap_or(false)) {
        assert_eq!(summary(bot), summary(common::bot(&snapshot, bot["id"].as_str().unwrap())), "as the event stream");
    }
    let ids: Vec<&Value> = listed.iter().map(|bot| &bot["id"]).collect();
    assert!([&a, &b, &group].iter().all(|id| ids.contains(&&json!(id))), "bots and group chats: {ids:?}");
    assert!(!ids.contains(&&json!(gone)), "a deleted bot isn't listed");
}

#[tokio::test]
async fn bots_needs_the_token() {
    let host = Host::start().await;
    let url = format!("{}/api/bots", host.base);
    let client = reqwest::Client::new();
    let without = client.post(&url).json(&json!({})).send().await.unwrap();
    assert_eq!(without.status(), reqwest::StatusCode::UNAUTHORIZED);
    let wrong = client.post(&url).bearer_auth("wrong").json(&json!({})).send().await.unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
}
