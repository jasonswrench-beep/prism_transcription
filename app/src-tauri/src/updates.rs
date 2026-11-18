//! Program updates: a button, never a timer (as PrismQDA's decision 189; chosen for Prism Transcribe 2026-11-18).
//!
//! Prism Transcribe makes no network connection of its own except the ones a person starts: the speech-model download, and pressing
//! **Check for updates** in Settings. That asks one address (the update feed, a small JSON file on github.com) for the newest version, and
//! sends nothing but what any web request sends (the program's version and the computer's kind, in the request's headers). Nothing is checked
//! in the background, at start, or on a schedule. Downloading and installing is a second step that needs the person's go-ahead, and the
//! download is checked against a signature made with the maintainer's update-signing key (its public half is in this program) before anything
//! runs; a file that does not match is refused.
//!
//! While `PUBKEY` is empty the updater is "not set up" and cannot connect at all (tools/make-update-key.ps1 puts the key in). The check is also
//! refused while the Offline switch is on, and installing is refused while a transcription or model download runs (the installer closes the
//! program). Ported from PrismQDA's app/src-tauri/src/updates.rs, same author.

use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Where the newest version is announced: `latest.json` on the `update-feed` release of the public repository (docs/UPDATES.md). Not
/// `releases/latest`, because GitHub does not count a pre-release as the latest release.
pub const ENDPOINT: &str = "https://github.com/jasonswrench-beep/prism_transcription/releases/download/update-feed/latest.json";

/// The public half of the update-signing key (the text of the `.pub` file `tools/make-update-key.ps1` makes). **Empty until that is done**,
/// and then the updater is "not set up": it cannot connect to anything.
pub const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDREQjJEREZDNkY1QTYyNzkKUldSNVlscHYvTjJ5VFN5OGJOVTJBZndNYXptTmpRbVZ2QnFHNnkyM3VURExzSXgxems2WWdNcWkK";

#[derive(Clone, Debug)]
pub struct Feed {
    pub endpoint: String,
    pub pubkey: String,
}

impl Feed {
    pub fn built_in() -> Feed {
        Feed { endpoint: ENDPOINT.to_string(), pubkey: PUBKEY.to_string() }
    }

    /// True when both the address and the key are present. Without them nothing is ever requested.
    pub fn is_set_up(&self) -> bool {
        !self.endpoint.trim().is_empty() && !self.pubkey.trim().is_empty()
    }

    /// The name of the computer the address is on, for saying who is asked.
    pub fn host(&self) -> String {
        tauri::Url::parse(&self.endpoint).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
    }
}

/// Why an update cannot be checked or installed now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    NotSetUp,
    Offline,
    /// A transcription or a model download is running; the installer would stop it.
    Busy,
}

impl Blocked {
    fn code(self) -> &'static str {
        match self {
            Blocked::NotSetUp => "not_set_up",
            Blocked::Offline => "offline",
            Blocked::Busy => "busy",
        }
    }

    fn words(self) -> &'static str {
        match self {
            Blocked::NotSetUp => "this version of Prism Transcribe is not set up to check for updates",
            Blocked::Offline => "Offline is on in Settings, so Prism Transcribe does not connect to anything",
            Blocked::Busy => "a transcription or a model download is running; pause the queue or wait for it to end, then install",
        }
    }
}

pub fn blocked(feed: &Feed, offline: bool) -> Option<Blocked> {
    if !feed.is_set_up() {
        Some(Blocked::NotSetUp)
    } else if offline {
        Some(Blocked::Offline)
    } else {
        None
    }
}

/// What the Updates part of Settings shows before anything is asked.
pub fn state<R: Runtime>(app: &AppHandle<R>, feed: &Feed, offline: bool) -> Value {
    json!({
        "version": app.package_info().version.to_string(),
        "set_up": feed.is_set_up(),
        "host": feed.host(),
        "blocked": blocked(feed, offline).map(Blocked::code),
    })
}

fn updater<R: Runtime>(app: &AppHandle<R>, feed: &Feed) -> Result<tauri_plugin_updater::Updater, String> {
    let url = tauri::Url::parse(&feed.endpoint).map_err(|e| format!("the update address is not valid ({e})"))?;
    app.updater_builder()
        .pubkey(feed.pubkey.clone())
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

/// Asks the feed whether a version newer than this one exists. This is the one request, made only when this is called.
pub async fn check_update<R: Runtime>(app: &AppHandle<R>, feed: &Feed) -> Result<Option<Update>, String> {
    updater(app, feed)?.check().await.map_err(|e| format!("the check for updates did not work: {e}"))
}

/// The Check button: refused while blocked; otherwise one request to the feed.
pub async fn check<R: Runtime>(app: &AppHandle<R>, feed: &Feed, offline: bool) -> Result<Value, String> {
    if let Some(why) = blocked(feed, offline) {
        return Err(why.words().to_string());
    }
    Ok(match check_update(app, feed).await? {
        Some(u) => json!({ "available": true, "version": u.version, "notes": u.body.clone().unwrap_or_default(), "date": u.date.map(|d| d.to_string()).unwrap_or_default() }),
        None => json!({ "available": false }),
    })
}

/// The Install button: checks again (the feed may have moved on), downloads, verifies the signature, and runs the installer for this Windows
/// account, which replaces the program; the program ends when the installer starts.
pub async fn install<R: Runtime>(app: &AppHandle<R>, feed: &Feed, offline: bool, busy: bool) -> Result<Value, String> {
    if let Some(why) = blocked(feed, offline).or(busy.then_some(Blocked::Busy)) {
        return Err(why.words().to_string());
    }
    let Some(update) = check_update(app, feed).await? else {
        return Ok(json!({ "installed": false }));
    };
    let handle = app.clone();
    let mut received: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                received += chunk as u64;
                let _ = handle.emit("update-progress", json!({ "received": received, "total": total }));
            },
            || {},
        )
        .await
        .map_err(|e| format!("the update was not installed: {e}"))?;
    Ok(json!({ "installed": true, "version": update.version }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};

    // A test key pair's public half, a file signed with its private half, and another key's public half: PrismQDA's update test fixtures.
    const PUB: &str = include_str!("../tests/fixtures/update-test.pub");
    const OTHER_PUB: &str = include_str!("../tests/fixtures/update-other.pub");
    const SIGNATURE: &str = include_str!("../tests/fixtures/update-setup.bin.sig");
    const SETUP: &[u8] = include_bytes!("../tests/fixtures/update-setup.bin");

    fn app() -> tauri::App<MockRuntime> {
        let mut context = mock_context(noop_assets());
        context.config_mut().plugins.0.insert("updater".into(), json!({ "pubkey": "", "endpoints": [], "windows": { "installMode": "passive" } }));
        mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).expect("the mock app builds")
    }

    struct Server {
        base: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    /// A feed on this computer: `/latest.json` announces `version` with the fixture's signature, and `/setup.exe` gives `bytes`.
    fn serve(version: &'static str, bytes: &'static [u8], status_for_feed: u16) -> Server {
        serve_signed(version, bytes.to_vec(), SIGNATURE.trim().to_string(), status_for_feed)
    }

    /// The same, for a file and a signature given at run time (a setup that was really built).
    fn serve_signed(version: &str, bytes: Vec<u8>, signature: String, status_for_feed: u16) -> Server {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        let manifest = json!({
            "version": version,
            "notes": "Fixes for the test.",
            "pub_date": "2026-11-18T12:00:00Z",
            "platforms": { "windows-x86_64": { "signature": signature, "url": format!("{base}/setup.exe") } },
        })
        .to_string();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let url = request.url().to_string();
                log.lock().unwrap().push(url.clone());
                let response = if url == "/latest.json" && status_for_feed == 200 {
                    tiny_http::Response::from_string(manifest.clone()).with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()).boxed()
                } else if url == "/setup.exe" {
                    tiny_http::Response::from_data(bytes.clone()).boxed()
                } else {
                    tiny_http::Response::from_string("not here").with_status_code(status_for_feed.max(404)).boxed()
                };
                let _ = request.respond(response);
            }
        });
        Server { base, requests }
    }

    fn feed(server: &Server, pubkey: &str) -> Feed {
        Feed { endpoint: format!("{}/latest.json", server.base), pubkey: pubkey.trim().to_string() }
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn a_build_with_no_key_is_not_set_up_and_never_connects() {
        let app = app();
        let server = serve("9.9.9", SETUP, 200);
        let no_key = Feed { endpoint: format!("{}/latest.json", server.base), pubkey: String::new() };
        assert!(!no_key.is_set_up());
        assert_eq!(state(app.handle(), &no_key, false)["blocked"], "not_set_up");
        assert!(block_on(check(app.handle(), &no_key, false)).unwrap_err().contains("not set up"));
        assert!(server.requests.lock().unwrap().is_empty(), "nothing was asked of the server");
        assert_eq!(Feed::built_in().is_set_up(), !PUBKEY.is_empty());
        assert_eq!(Feed::built_in().host(), "github.com");
        assert!(ENDPOINT.starts_with("https://"));
    }

    #[test]
    fn offline_and_a_running_job_stop_it_before_any_connection() {
        let app = app();
        let server = serve("9.9.9", SETUP, 200);
        let f = feed(&server, PUB);
        assert_eq!(state(app.handle(), &f, true)["blocked"], "offline");
        assert!(block_on(check(app.handle(), &f, true)).unwrap_err().contains("Offline"));
        assert!(block_on(install(app.handle(), &f, true, false)).is_err());
        assert!(block_on(install(app.handle(), &f, false, true)).unwrap_err().contains("running"));
        assert!(server.requests.lock().unwrap().is_empty(), "refused first");
        assert_eq!(state(app.handle(), &f, false)["blocked"], Value::Null);
    }

    #[test]
    fn a_newer_version_is_announced_with_its_notes_and_an_older_one_is_not() {
        let app = app();
        let newer = serve("9.9.9", SETUP, 200);
        let found = block_on(check(app.handle(), &feed(&newer, PUB), false)).unwrap();
        assert_eq!((found["available"].as_bool(), found["version"].as_str(), found["notes"].as_str()), (Some(true), Some("9.9.9"), Some("Fixes for the test.")));
        assert_eq!(newer.requests.lock().unwrap().as_slice(), ["/latest.json"], "one request, and not for the installer");
        let older = serve("0.0.1", SETUP, 200);
        assert_eq!(block_on(check(app.handle(), &feed(&older, PUB), false)).unwrap()["available"], false);
        let missing = serve("9.9.9", SETUP, 404);
        assert!(block_on(check(app.handle(), &feed(&missing, PUB), false)).is_err(), "a missing feed fails in words");
    }

    #[test]
    fn only_the_signed_download_passes() {
        let app = app();
        let good = serve("9.9.9", SETUP, 200);
        let update = block_on(check_update(app.handle(), &feed(&good, PUB))).unwrap().expect("a newer version");
        assert_eq!(block_on(update.download(|_, _| {}, || {})).expect("the signed file passes"), SETUP);
        let tampered: &'static [u8] = Box::leak({
            let mut b = SETUP.to_vec();
            b[3] ^= 1;
            b.into_boxed_slice()
        });
        let bad = serve("9.9.9", tampered, 200);
        let update = block_on(check_update(app.handle(), &feed(&bad, PUB))).unwrap().expect("announced");
        assert!(block_on(update.download(|_, _| {}, || {})).is_err(), "a changed byte is refused");
        let update = block_on(check_update(app.handle(), &feed(&good, OTHER_PUB))).unwrap().expect("announced");
        assert!(block_on(update.download(|_, _| {}, || {})).is_err(), "another key's signature is refused");
    }

    /// Run by hand on a release built with `app\BUILD-INSTALLER.ps1 -Sign -Updates`, in a build whose `PUBKEY` is the key that signed it:
    /// `PRISM_REAL_SETUP_DIR=<target\release-assets> cargo test -p prism-transcribe-app real_built -- --ignored`. The real, Azure-signed
    /// setup must pass its update signature with the key built into this program, and the same file with one byte changed must not.
    #[test]
    #[ignore = "needs a setup built with -Updates; set PRISM_REAL_SETUP_DIR to its folder"]
    fn a_real_built_setup_passes_its_update_signature_and_a_changed_byte_does_not() {
        let Some(dir) = std::env::var_os("PRISM_REAL_SETUP_DIR") else { return };
        assert!(!PUBKEY.is_empty(), "this build has no public key to check with");
        let dir = std::path::PathBuf::from(dir);
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("latest.json")).unwrap()).unwrap();
        let platform = &manifest["platforms"]["windows-x86_64"];
        let signature = platform["signature"].as_str().unwrap().to_string();
        let name = platform["url"].as_str().unwrap().rsplit('/').next().unwrap().to_string();
        let bytes = std::fs::read(dir.join(&name)).unwrap();
        let app = app();
        let built_in = |s: &Server| Feed { endpoint: format!("{}/latest.json", s.base), pubkey: PUBKEY.to_string() };

        let good = serve_signed("99.0.0", bytes.clone(), signature.clone(), 200);
        let update = block_on(check_update(app.handle(), &built_in(&good))).unwrap().expect("a newer version");
        assert_eq!(block_on(update.download(|_, _| {}, || {})).expect("the built setup passes its signature"), bytes);

        let mut changed = bytes;
        let middle = changed.len() / 2;
        changed[middle] ^= 1;
        let bad = serve_signed("99.0.0", changed, signature, 200);
        let update = block_on(check_update(app.handle(), &built_in(&bad))).unwrap().expect("announced");
        assert!(block_on(update.download(|_, _| {}, || {})).is_err(), "one changed byte is refused");
    }
}
