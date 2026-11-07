//! Dictation on this computer's own speech model, on a real host with a stand-in `whisper-server`:
//! the recording goes to it over loopback even with a proxy configured, and the server stops with
//! the host.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use common::host::Host;
use serde_json::json;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Answers `/inference` with a fixed transcript and writes its pid to `$SPEECH_PID`.
const FAKE_SERVER: &str = r#"#!/usr/bin/env python3
import http.server, json, os, sys
open(os.environ["SPEECH_PID"], "w").write(str(os.getpid()))
port = int(sys.argv[sys.argv.index("--port") + 1])
class Inference(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        body = json.dumps({"text": " Ask Builder to run cargo test."}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer(("127.0.0.1", port), Inference).serve_forever()
"#;

/// One second of a loud tone, as the clients send it: a 16 kHz mono WAV.
fn speech() -> String {
    let pcm: Vec<u8> =
        (0..16_000).flat_map(|i: i32| (if i % 20 < 10 { 8_000i16 } else { -8_000 }).to_le_bytes()).collect();
    let len = u32::try_from(pcm.len()).unwrap();
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&[16, 0, 0, 0, 1, 0, 1, 0]);
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&32_000u32.to_le_bytes());
    wav.extend_from_slice(&[2, 0, 16, 0]);
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    STANDARD.encode(wav)
}

/// A proxy that notes whether a recording came through it; the host's other requests (update
/// checks) may use it.
async fn proxy() -> (u16, Arc<AtomicBool>) {
    use tokio::io::AsyncReadExt as _;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let used = Arc::new(AtomicBool::new(false));
    let seen = used.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut head = [0u8; 1024];
            let n = stream.read(&mut head).await.unwrap_or(0);
            if String::from_utf8_lossy(&head[..n]).contains("/inference") {
                seen.store(true, Ordering::SeqCst);
            }
        }
    });
    (port, used)
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid]).status().unwrap().success()
}

#[tokio::test]
async fn local_dictation_stays_on_this_computer_and_stops_with_the_host() {
    let bin = std::env::temp_dir().join(format!("codync-speech-bin-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&bin).unwrap();
    let server = bin.join("whisper-server");
    std::fs::write(&server, FAKE_SERVER).unwrap();
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).unwrap();
    let pid_file = bin.join("pid");
    let (proxy_port, proxied) = proxy().await;
    let proxy_url = format!("http://127.0.0.1:{proxy_port}");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    // SAFETY: this test binary runs this one test, and sets them before starting anything; the host inherits them.
    unsafe {
        std::env::set_var("PATH", path);
        std::env::set_var("SPEECH_PID", &pid_file);
        for name in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
            std::env::set_var(name, &proxy_url);
        }
    }

    let mut host = Host::start().await;
    let before = host.call("voiceStatus", json!({})).await;
    assert_eq!(before["local"]["ready"], false, "{before}");
    std::fs::create_dir_all(host.home.join("speech")).unwrap();
    std::fs::write(host.home.join("speech/ggml-large-v3-turbo.bin"), b"").unwrap();
    assert_eq!(host.call("voiceStatus", json!({})).await["local"]["ready"], true);

    let said = host.call("voiceTranscribe", json!({"provider": "local", "model": "", "audio": speech()})).await;
    assert_eq!(said["text"], "Ask Builder to run cargo test.");
    assert!(!proxied.load(Ordering::SeqCst), "the recording went through the proxy");

    let pid = std::fs::read_to_string(&pid_file).unwrap();
    assert!(alive(&pid));
    host.restart(|_: &Path| {}).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) {
        assert!(Instant::now() < deadline, "the speech server outlived the host");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    std::fs::remove_dir_all(&bin).unwrap();
}
