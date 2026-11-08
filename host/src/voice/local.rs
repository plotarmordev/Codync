//! Dictation on the user's own speech model: a server they run that answers the `OpenAI`
//! `/v1/audio/transcriptions` API (whisper.cpp's `whisper-server`, Speaches, …), on this computer or
//! another of theirs on the same network. Audio goes only to loopback, private-network or
//! Tailscale addresses, pinned at each request, so it never leaves the user's computers.

use super::{TRANSCRIBE_HINT, multipart, names_hint, spoken_names};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use url::Url;

const SERVER_KEY: &str = "speech-server";
/// The model to ask the server for: servers serving several (Speaches) need one; whisper-server
/// serves one and ignores it.
const MODEL_KEY: &str = "speech-server-model";
const TIMEOUT: Duration = Duration::from_mins(2);
/// Whisper writes "Thank you." or "you" for silence or faint room noise; a recording whose loudest
/// 100 ms stays under this RMS (-45 dBFS; room noise is about -60, speech -30 to -10) has no speech.
const SPEECH_RMS: f32 = 0.0056;
/// 100 ms at 16 kHz.
const WINDOW: u16 = 1_600;

/// Where to link a speech server: in Settings for Codync on this computer; a phone or another
/// computer is told which computer, since the model runs there and not on the device reading it.
fn not_linked(here: bool) -> String {
    if here {
        return "No speech model is linked on this computer. Link one in Settings, Voice chat.".into();
    }
    let computer = crate::service::host_name();
    format!("No speech model is linked on {computer}. Link one in Codync on that computer: Settings, Voice chat.")
}

fn server(store: &Store) -> Option<String> {
    store.kv_get(SERVER_KEY).filter(|s| !s.is_empty())
}

fn model(store: &Store) -> String {
    store.kv_get(MODEL_KEY).unwrap_or_default()
}

/// `{ready, message, server}` for `localDictation`: the linked server, or what to do without one,
/// worded for a caller on this computer (`here`) or elsewhere.
pub fn status(store: &Store, here: bool) -> Value {
    let server = server(store);
    json!({"ready": server.is_some(), "message": server.is_none().then(|| not_linked(here)), "server": server})
}

/// `setSpeechServer` (only on this computer): links the server at `url`, asking it for `model` if
/// one is given, once it transcribes a second of silence; or unlinks it when `url` is empty.
pub async fn link(store: &Store, url: &str, model: &str) -> Result<Value> {
    let (url, model) = (url.trim(), model.trim());
    if url.is_empty() {
        store.kv_delete(SERVER_KEY)?;
        store.kv_delete(MODEL_KEY)?;
    } else {
        post(store, url, model, &super::wav(&vec![0; 32_000], 16_000)).await?;
        store.kv_set(SERVER_KEY, url)?;
        store.kv_set(MODEL_KEY, model)?;
    }
    Ok(status(store, true))
}

/// Speech to text for one utterance (a 16 kHz mono 16-bit WAV) on the linked server.
pub async fn transcribe(store: &Store, wav: &[u8]) -> Result<Value> {
    let url = server(store).ok_or_else(|| anyhow!(not_linked(false)))?;
    if !voiced(wav) {
        return Ok(json!({"text": ""}));
    }
    let text = post(store, &url, &model(store), wav).await?;
    Ok(json!({"text": spoken(&text)}))
}

/// One transcription request to the server at `url`; its transcript, or why it gave none.
async fn post(store: &Store, url: &str, model: &str, wav: &[u8]) -> Result<String> {
    let parsed = Url::parse(url).map_err(|_| anyhow!("`{url}` isn't a web address"))?;
    let client = pinned(&parsed).await?;
    let prompt = format!("{TRANSCRIBE_HINT} {}", names_hint(&spoken_names(store)));
    let mut fields = vec![("prompt", prompt.as_str()), ("temperature", "0"), ("response_format", "json")];
    if !model.is_empty() {
        fields.push(("model", model));
    }
    let (boundary, body) = multipart(&fields, wav);
    let res = client
        .post(parsed)
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .send()
        .await
        .map_err(|_| anyhow!("The speech server at {url} isn't answering. Check that it's running."))?;
    if res.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY && model.is_empty() {
        bail!("The speech server at {url} needs a model: enter the one it serves.");
    }
    if !res.status().is_success() {
        bail!("The speech server at {url} answered {}. Check its address and model.", res.status());
    }
    let v: Value = res.json().await.unwrap_or_default();
    v["text"]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("The speech server at {url} didn't answer with a transcript."))
}

/// A client that reaches `url`'s host only at the addresses checked here: each on this computer or
/// its local network, with no proxy and no redirects, so a DNS change or a redirect can't send the
/// audio elsewhere.
async fn pinned(url: &Url) -> Result<reqwest::Client> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("the address must start with http:// or https://");
    }
    let host = url.host_str().context("the address has no host")?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.trim_matches(['[', ']']), port)).await?.collect();
    if addrs.is_empty() || !addrs.iter().all(|a| on_users_network(a.ip())) {
        bail!("{host} isn't on this computer or its local network, so audio would leave your computers");
    }
    Ok(reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .resolve_to_addrs(host, &addrs)
        .build()?)
}

/// Loopback, private (RFC 1918, IPv6 unique local), link-local, or Tailscale's 100.64.0.0/10.
fn on_users_network(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || (v4.octets()[0] == 100 && v4.octets()[1] & 0xc0 == 64)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
    }
}

/// Whether any 100 ms of the WAV's samples is loud enough to be speech (a shorter last window counts
/// its missing samples as silence).
fn voiced(wav: &[u8]) -> bool {
    let start = wav.windows(4).position(|w| w == b"data").map_or(wav.len(), |i| i + 8).min(wav.len());
    let floor = SPEECH_RMS * SPEECH_RMS * f32::from(WINDOW);
    wav[start..]
        .as_chunks::<2>()
        .0
        .chunks(usize::from(WINDOW))
        .any(|w| w.iter().map(|b| f32::from(i16::from_le_bytes(*b)) / 32_768.0).map(|s| s * s).sum::<f32>() >= floor)
}

/// Words Whisper writes, alone in brackets or parentheses, for what isn't speech (`[BLANK_AUDIO]`,
/// `(upbeat music)`, `[ Silence ]`); a span of anything else is the speaker's own.
const CAPTION_WORDS: [&str; 17] = [
    "blank",
    "audio",
    "music",
    "silence",
    "inaudible",
    "applause",
    "laughter",
    "laughs",
    "laughing",
    "noise",
    "static",
    "upbeat",
    "soft",
    "background",
    "sound",
    "sighs",
    "coughs",
];

fn is_caption(inner: &str) -> bool {
    let mut words = inner.split(|c: char| c == '_' || c.is_whitespace()).filter(|w| !w.is_empty()).peekable();
    words.peek().is_some() && words.all(|w| CAPTION_WORDS.contains(&w.to_lowercase().as_str()))
}

/// The transcript without Whisper's markers for silence and sounds: each bracketed or parenthesized
/// span made only of caption words. Every other bracket and parenthetical stays as spoken.
fn spoken(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(['[', '(']) {
        let close = if rest[open..].starts_with('[') { ']' } else { ')' };
        let Some(len) = rest[open + 1..].find(close) else { break };
        let end = open + 1 + len;
        kept.push_str(&rest[..open]);
        if !is_caption(&rest[open + 1..end]) {
            kept.push_str(&rest[open..=end]);
        }
        rest = &rest[end + 1..];
    }
    kept.push_str(rest);
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_store() -> Store {
        Store::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn recording(pcm: &[i16]) -> Vec<u8> {
        super::super::wav(&pcm.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>(), 16_000)
    }

    #[test]
    fn short_silence_and_room_noise_are_not_speech_but_quiet_words_are() {
        // Room noise at -60 dBFS, and a word spoken quietly over it: 300 ms of a 200 Hz wave at -40 dBFS.
        let room: Vec<i16> = (0..16_000).map(|i| if i % 2 == 0 { 33 } else { -33 }).collect();
        let mut quiet_word = room.clone();
        for (i, s) in quiet_word[4_000..8_800].iter_mut().enumerate() {
            *s += if i / 40 % 2 == 0 { 320 } else { -320 };
        }
        assert!(!voiced(&recording(&[0; 8_000])));
        assert!(!voiced(&recording(&room)));
        assert!(voiced(&recording(&quiet_word)));
    }

    #[test]
    fn only_the_users_own_computers_can_hear_the_audio() {
        let ip = |s: &str| s.parse::<IpAddr>().expect("test addresses are valid");
        for own in [
            "127.0.0.1",
            "::1",
            "192.168.1.20",
            "10.0.0.5",
            "172.16.3.4",
            "169.254.1.1",
            "100.101.102.103",
            "fd7a:115c:a1e0::1",
            "fe80::1",
            "::ffff:192.168.1.20",
        ] {
            assert!(on_users_network(ip(own)), "{own}");
        }
        for outside in ["8.8.8.8", "100.128.0.1", "172.32.0.1", "2606:4700::1111", "::ffff:8.8.8.8"] {
            assert!(!on_users_network(ip(outside)), "{outside}");
        }
    }

    #[tokio::test]
    async fn a_server_out_on_the_internet_is_never_sent_audio() {
        for url in [
            "http://8.8.8.8/v1/audio/transcriptions",
            "https://[2606:4700::1111]/inference",
            "ftp://127.0.0.1/",
            "not a url",
        ] {
            let store = memory_store();
            let e = link(&store, url, "").await.expect_err(url);
            assert!(server(&store).is_none(), "{url} linked: {e:#}");
        }
    }

    /// A server like Speaches: it serves several models, so a request without `model` gets 422.
    async fn model_required_server(model: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/audio/transcriptions", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let field = format!("name=\"model\"\r\n\r\n{model}\r\n");
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut seen = Vec::new();
                // The multipart body ends with its closing boundary.
                while !seen.ends_with(b"--\r\n") {
                    socket.read_buf(&mut seen).await.unwrap();
                }
                let (status, body) = if String::from_utf8_lossy(&seen).contains(&field) {
                    ("200 OK", r#"{"text":" Hello from my own model. "}"#)
                } else {
                    ("422 Unprocessable Entity", r#"{"detail":"model: Field required"}"#)
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        url
    }

    #[tokio::test]
    async fn a_linked_server_transcribes_with_its_model_and_unlinking_says_how_to_link_again() {
        let model = "Systran/faster-whisper-large-v3";
        let url = model_required_server(model).await;
        let store = memory_store();
        let computer = crate::service::host_name();
        let elsewhere = status(&store, false)["message"].as_str().unwrap_or_default().to_owned();
        assert!(elsewhere.contains(&format!("on {computer}.")), "a phone is told which computer: {elsewhere}");
        assert_eq!(
            status(&store, true)["message"],
            "No speech model is linked on this computer. Link one in Settings, Voice chat."
        );
        assert!(transcribe(&store, &recording(&[0; 160])).await.is_err());

        let e = link(&store, &url, "").await.expect_err("a server that needs a model");
        assert!(e.to_string().contains("needs a model"), "{e:#}");
        assert!(server(&store).is_none());

        let linked = link(&store, &url, model).await.unwrap();
        assert_eq!((linked["ready"].as_bool(), linked["server"].as_str()), (Some(true), Some(url.as_str())));
        let speech: Vec<i16> = (0..8_000).map(|i| if i / 40 % 2 == 0 { 3_000 } else { -3_000 }).collect();
        assert_eq!(transcribe(&store, &recording(&speech)).await.unwrap()["text"], "Hello from my own model.");
        assert_eq!(transcribe(&store, &recording(&[0; 8_000])).await.unwrap()["text"], "");

        let unlinked = link(&store, "", "").await.unwrap();
        assert_eq!(
            (unlinked["ready"].as_bool(), unlinked["message"].as_str()),
            (Some(false), Some(not_linked(true).as_str()))
        );
        assert!(store.kv_get(MODEL_KEY).is_none(), "unlinking forgets the model");
    }

    #[test]
    fn whisper_markers_are_not_speech() {
        assert_eq!(spoken(" [BLANK_AUDIO] "), "");
        assert_eq!(spoken("(music)"), "");
        assert_eq!(spoken("(upbeat music)"), "");
        assert_eq!(spoken("[ Silence ]"), "");
        assert_eq!(spoken("Ship it [MUSIC] now."), "Ship it now.");
        assert_eq!(spoken(" Ship it. "), "Ship it.");
    }

    #[test]
    fn speech_beside_a_marker_brackets_and_parentheticals_stay() {
        assert_eq!(spoken("[BLANK_AUDIO] Hello, ship the fix. [BLANK_AUDIO]"), "Hello, ship the fix.");
        assert_eq!(spoken("(Please keep this parenthetical.)"), "(Please keep this parenthetical.)");
        assert_eq!(spoken("(Please keep this parenthetical)"), "(Please keep this parenthetical)");
        assert_eq!(spoken("Run the tests (all of them) first."), "Run the tests (all of them) first.");
        assert_eq!(spoken("Set the value to [1, 2, 3]."), "Set the value to [1, 2, 3].");
        assert_eq!(spoken("Use arr[0] instead."), "Use arr[0] instead.");
        assert_eq!(spoken("(play some music)"), "(play some music)");
    }
}
