//! A test host never asks the public agent and MCP registries for anything: every request it would send goes to a proxy
//! that only writes down where it was headed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::net::TcpListener;

const REGISTRIES: [&str; 2] = ["registry.modelcontextprotocol.io", "cdn.agentclientprotocol.com"];

/// A proxy on loopback that keeps the first line of every request (`CONNECT host:443 HTTP/1.1`) and answers none.
async fn recording_proxy() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let log = log.clone();
            tokio::spawn(async move {
                let mut line = String::new();
                let _ = BufReader::new(stream).read_line(&mut line).await;
                log.lock().unwrap().push(line);
            });
        }
    });
    (url, seen)
}

#[tokio::test]
async fn a_test_host_never_reaches_the_registries() {
    let (proxy, seen) = recording_proxy().await;
    let env = [
        ("HTTPS_PROXY", OsStr::new(&proxy)),
        ("HTTP_PROXY", OsStr::new(&proxy)),
        ("NO_PROXY", OsStr::new("127.0.0.1,localhost")),
    ];
    let host = Host::start_with(&env).await;
    for search in ["", "github"] {
        host.refused("marketConnectors", json!({"search": search})).await;
    }
    host.refused("connectorInfo", json!({"registryName": "io.github.github/github-mcp-server"})).await;
    // What the host fetches at launch has had its time to go out.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let seen = seen.lock().unwrap();
    let reached: Vec<_> = seen.iter().filter(|line| REGISTRIES.iter().any(|r| line.contains(r))).collect();
    assert!(reached.is_empty(), "the host asked the registries: {reached:?}");
}
