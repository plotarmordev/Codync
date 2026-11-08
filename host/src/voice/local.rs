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

/// `{ready, message, server}` for `localDictation`: the linked server, or what to do without one,
/// worded for a caller on this computer (`here`) or elsewhere.
pub fn status(store: &Store, here: bool) -> Value {
    let server = server(store);
    json!({"ready": server.is_some(), "message": server.is_none().then(|| not_linked(here)), "server": server})
}

/// `setSpeechServer` (only on this computer): links the server at `url` once it transcribes a
/// short silence, or unlinks it when `url` is empty.
pub async fn link(store: &Store, url: &str) -> Result<Value> {
    let url = url.trim();
    if url.is_empty() {
        store.kv_delete(SERVER_KEY)?;
    } else {
        post(store, url, &super::wav(&[0; 16_000], 16_000)).await?;
        store.kv_set(SERVER_KEY, url)?;
    }
    Ok(status(store, true))
}

/// Speech to text for one utterance (a 16 kHz mono 16-bit WAV) on the linked server.
pub async fn transcribe(store: &Store, wav: &[u8]) -> Result<Value> {
    let url = server(store).ok_or_else(|| anyhow!(not_linked(false)))?;
    if !voiced(wav) {
        return Ok(json!({"text": ""}));
    }
    let text = post(store, &url, wav).await?;
    Ok(json!({"text": spoken(&text)}))
}

/// One transcription request to the server at `url`; its transcript, or why it gave none.
async fn post(store: &Store, url: &str, wav: &[u8]) -> Result<String> {
    let parsed = Url::parse(url).map_err(|_| anyhow!("`{url}` isn't a web address"))?;
    let client = pinned(&parsed).await?;
    let prompt = format!("{TRANSCRIBE_HINT} {}", names_hint(&spoken_names(store)));
    let (boundary, body) = multipart(&[("prompt", &prompt), ("temperature", "0"), ("response_format", "json")], wav);
    let res = client
        .post(parsed)
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .send()
        .await
        .map_err(|_| anyhow!("The speech server at {url} isn't answering. Check that it's running."))?;
    if !res.status().is_success() {
        bail!("The speech server at {url} answered {}. Check its address.", res.status());
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

/// The transcript without whisper.cpp's markers for silence and sounds: every `[BLANK_AUDIO]`-style
/// tag, and a transcript that is only a caption like `(music)`. Words beside a tag, and a spoken
/// parenthetical (it has sentence punctuation, or sits among other words), stay.
fn spoken(text: &str) -> String {
    let mut words = String::new();
    let mut rest = text;
    while let Some((before, tagged)) = rest.split_once('[')
        && let Some((_, after)) = tagged.split_once(']')
    {
        words.push_str(before);
        words.push(' ');
        rest = after;
    }
    words.push_str(rest);
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let caption = words
        .strip_prefix('(')
        .and_then(|w| w.strip_suffix(')'))
        .is_some_and(|inner| !inner.contains(['(', ')', '.', ',', '!', '?']));
    if caption { String::new() } else { words }
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
            let e = link(&store, url).await.expect_err(url);
            assert!(server(&store).is_none(), "{url} linked: {e:#}");
        }
    }

    #[tokio::test]
    async fn a_linked_server_transcribes_and_unlinking_says_how_to_link_again() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/audio/transcriptions", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut seen = Vec::new();
                // The multipart body ends with its closing boundary.
                while !seen.ends_with(b"--\r\n") {
                    socket.read_buf(&mut seen).await.unwrap();
                }
                let body = r#"{"text":" Hello from my own model. "}"#;
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let store = memory_store();
        let computer = crate::service::host_name();
        let elsewhere = status(&store, false)["message"].as_str().unwrap_or_default().to_owned();
        assert!(elsewhere.contains(&format!("on {computer}.")), "a phone is told which computer: {elsewhere}");
        assert_eq!(
            status(&store, true)["message"],
            "No speech model is linked on this computer. Link one in Settings, Voice chat."
        );
        assert!(transcribe(&store, &recording(&[0; 160])).await.is_err());

        let linked = link(&store, &url).await.unwrap();
        assert_eq!((linked["ready"].as_bool(), linked["server"].as_str()), (Some(true), Some(url.as_str())));
        let speech: Vec<i16> = (0..8_000).map(|i| if i / 40 % 2 == 0 { 3_000 } else { -3_000 }).collect();
        assert_eq!(transcribe(&store, &recording(&speech)).await.unwrap()["text"], "Hello from my own model.");
        assert_eq!(transcribe(&store, &recording(&[0; 8_000])).await.unwrap()["text"], "");

        let unlinked = link(&store, "").await.unwrap();
        assert_eq!(
            (unlinked["ready"].as_bool(), unlinked["message"].as_str()),
            (Some(false), Some(not_linked(true).as_str()))
        );
    }

    #[test]
    fn whisper_markers_are_not_speech() {
        assert_eq!(spoken(" [BLANK_AUDIO] "), "");
        assert_eq!(spoken("(music)"), "");
        assert_eq!(spoken("(upbeat music)"), "");
        assert_eq!(spoken(" Ship it. "), "Ship it.");
    }

    #[test]
    fn speech_beside_a_marker_and_spoken_parentheticals_stay() {
        assert_eq!(spoken("[BLANK_AUDIO] Hello, ship the fix. [BLANK_AUDIO]"), "Hello, ship the fix.");
        assert_eq!(spoken("(Please keep this parenthetical.)"), "(Please keep this parenthetical.)");
        assert_eq!(spoken("Run the tests (all of them) first."), "Run the tests (all of them) first.");
    }
}
