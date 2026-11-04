//! Chrome native messaging host for the Nubi DeskTime browser extension.
//! Chrome, Brave and Edge start `nubi.exe chrome-extension://<id>/ [--parent-window=N]` and talk
//! over stdio: a 4-byte little-endian length, then JSON. One message in, one reply out.
//! The message is validated here and again in the GUI, then forwarded over the named pipe.
//! Cookie values are never logged or echoed.
use crate::protocol::*;
use crate::settings;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub const HOST_NAME: &str = "dev.nubi.desktime";
/// Fixed by the `key` in extension/desktime-connect/manifest.json.
pub const EXTENSION_ID: &str = "gpoebchpnogppdkgjphjelcipnfiecif";
/// Browsers that read native host registrations from these HKCU keys.
const BROWSER_KEYS: [&str; 3] = [r"Software\Google\Chrome", r"Software\BraveSoftware\Brave-Browser", r"Software\Microsoft\Edge"];
/// The unpacked extension, written next to the settings so installed builds have a folder to load.
const EXTENSION_FILES: [(&str, &str); 4] = [
    ("manifest.json", include_str!("../../extension/desktime-connect/manifest.json")),
    ("background.js", include_str!("../../extension/desktime-connect/background.js")),
    ("popup.html", include_str!("../../extension/desktime-connect/popup.html")),
    ("popup.js", include_str!("../../extension/desktime-connect/popup.js")),
];

pub fn origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}/")
}
/// Chrome passes the caller's origin as the first argument.
pub fn is_native_call(arg: &str) -> bool {
    arg.starts_with("chrome-extension://")
}

pub fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, String> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).map_err(|_| "No message")?;
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 {
        return Err("Empty message".into());
    }
    if len > MAX_BROWSER_MSG {
        return Err("Message is too large".into());
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).map_err(|_| "Message was cut off")?;
    Ok(body)
}
pub fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> std::io::Result<()> {
    w.write_all(&(body.len() as u32).to_le_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// Send a validated message to the running GUI and wait for its answer.
fn forward(msg: BrowserMsg) -> BrowserReply {
    let Ok(mut line) = serde_json::to_string(&BrowserFrame { browser: msg }) else {
        return BrowserReply::result(Err("Unexpected message".into()));
    };
    line.push('\n');
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let exchange = || -> Result<BrowserReply, &'static str> {
            let mut f = crate::hook_client::open_pipe().ok_or("Nubi is not running")?;
            f.write_all(line.as_bytes()).and_then(|_| f.flush()).map_err(|_| "Could not reach Nubi")?;
            let mut resp = String::new();
            BufReader::new(f).take(4096).read_line(&mut resp).map_err(|_| "Nubi did not answer")?;
            serde_json::from_str(resp.trim()).map_err(|_| "Nubi did not answer")
        };
        let _ = tx.send(exchange());
    });
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(reply)) => reply,
        Ok(Err(e)) => BrowserReply::result(Err(e.into())),
        Err(_) => BrowserReply::result(Err("Nubi did not answer".into())),
    }
}

pub fn handle(origin_arg: &str, input: &mut impl Read) -> BrowserReply {
    if origin_arg != origin() {
        return BrowserReply::result(Err("Unknown extension".into()));
    }
    match read_frame(input).and_then(|body| parse_browser_msg(&body)) {
        Ok(msg) => forward(msg),
        Err(e) => BrowserReply::result(Err(e)),
    }
}

pub fn run(origin_arg: &str) -> i32 {
    let reply = handle(origin_arg, &mut std::io::stdin().lock());
    let body = serde_json::to_vec(&reply).unwrap_or_else(|_| br#"{"ok":false}"#.to_vec());
    let _ = write_frame(&mut std::io::stdout().lock(), &body);
    0
}

// ---------- registration (GUI start-up) ----------
pub fn extension_dir() -> PathBuf {
    settings::data_dir().join("desktime-extension")
}
fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
fn set_default_value(subkey: &str, value: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (key, data) = (wide(subkey), wide(value));
    // Creates the key when missing; a null value name sets the key's default value.
    let rc = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), std::ptr::null(), REG_SZ, data.as_ptr() as *const _, (data.len() * 2) as u32)
    };
    if rc != 0 {
        return Err(format!("could not write HKCU\\{subkey} (error {rc})"));
    }
    Ok(())
}
fn try_register() -> Result<(), String> {
    let dir = settings::data_dir().join("native-host");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": "Nubi DeskTime connector",
        "path": exe,
        "type": "stdio",
        "allowed_origins": [origin()],
    });
    let path = dir.join(format!("{HOST_NAME}.json"));
    write_if_changed(&path, &serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)?;
    for base in BROWSER_KEYS {
        set_default_value(&format!(r"{base}\NativeMessagingHosts\{HOST_NAME}"), &path.display().to_string())?;
    }
    let ext = extension_dir();
    std::fs::create_dir_all(&ext).map_err(|e| e.to_string())?;
    for (name, text) in EXTENSION_FILES {
        write_if_changed(&ext.join(name), text)?;
    }
    Ok(())
}
/// Writes the host manifest and the extension folder and registers the host for each browser.
/// Idempotent; runs at every start so the manifest follows the current exe path.
pub fn register() {
    if let Err(e) = try_register() {
        eprintln!("[desktime] native host not registered: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_frame(&mut out, body).unwrap();
        out
    }

    #[test]
    fn frames_round_trip() {
        let out = framed(br#"{"ok":true}"#);
        assert_eq!(&out[..4], &11u32.to_le_bytes());
        assert_eq!(read_frame(&mut out.as_slice()).unwrap(), br#"{"ok":true}"#);
    }

    #[test]
    fn bad_frames_are_rejected() {
        assert!(read_frame(&mut [1u8, 0].as_slice()).is_err());
        assert!(read_frame(&mut 0u32.to_le_bytes().as_slice()).is_err());
        let mut big = ((MAX_BROWSER_MSG + 1) as u32).to_le_bytes().to_vec();
        big.extend(vec![b' '; 16]);
        assert!(read_frame(&mut big.as_slice()).is_err());
        let mut cut = 10u32.to_le_bytes().to_vec();
        cut.extend(b"{}");
        assert!(read_frame(&mut cut.as_slice()).is_err());
    }

    #[test]
    fn only_our_extension_and_valid_messages_pass() {
        let body = framed(br#"{"type":"desktime_revoke"}"#);
        let r = handle("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/", &mut body.as_slice());
        assert_eq!(r.error.as_deref(), Some("Unknown extension"));
        let body = framed(br#"{"type":"desktime_session","cookies":[{"name":"a","value":"secret","domain":"evil.com","path":"/","secure":true,"httpOnly":true,"expires":null}]}"#);
        let r = handle(&origin(), &mut body.as_slice());
        assert!(!r.ok);
        assert!(!r.error.unwrap().contains("secret"));
        assert!(is_native_call(&origin()));
        assert!(!is_native_call("--hook"));
    }

    #[test]
    fn extension_manifest_matches_the_registered_id() {
        let m: serde_json::Value = serde_json::from_str(EXTENSION_FILES[0].1).unwrap();
        assert_eq!(m["manifest_version"], 3);
        // Chrome's ID: first 32 hex digits of SHA-256 of the public key, mapped 0-f to a-p.
        // Checked by length and alphabet here; the key itself is fixed in the manifest.
        assert!(m["key"].as_str().is_some_and(|k| k.len() > 300));
        assert!(EXTENSION_ID.len() == 32 && EXTENSION_ID.bytes().all(|b| (b'a'..=b'p').contains(&b)));
    }
}
