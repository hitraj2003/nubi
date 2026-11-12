//! Tauri-compatible manifest and minisign artifact transport; no Tauri runtime.
use base64::Engine;
use serde_json::Value;
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::time::Duration;

// Read the canonical production configuration at compile time to prevent key drift.
const CONFIG: &str = include_str!("../../../src-tauri/tauri.conf.json");
const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_ARTIFACT: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct Update {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<i64>,
    url: String,
    signature: String,
}

fn config() -> Value {
    serde_json::from_str(CONFIG).expect("production updater configuration")
}

fn get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("update URL must use HTTPS".into());
    }
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .redirects(5)
        .build()
        .get(url)
        .set("User-Agent", concat!("Nubi/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    if !response.get_url().starts_with("https://") {
        return Err("insecure update redirect".into());
    }
    let expected = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    if expected.is_some_and(|size| size > limit) {
        return Err("update response too large".into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit || expected.is_some_and(|size| size != bytes.len() as u64) {
        return Err("oversized or incomplete update response".into());
    }
    Ok(bytes)
}

pub fn check() -> Result<Option<Update>, String> {
    let configuration = config();
    let endpoint = configuration["plugins"]["updater"]["endpoints"][0]
        .as_str()
        .ok_or("missing update endpoint")?;
    let bytes = get(endpoint, MAX_MANIFEST)?;
    parse(&bytes).map(Some)
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Update, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let version = value["version"]
        .as_str()
        .ok_or("update version missing")?
        .trim_start_matches('v');
    semver::Version::parse(version).map_err(|e| e.to_string())?;
    let platform = if value.get("platforms").is_some() {
        let key = format!(
            "windows-{}",
            if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            }
        );
        value["platforms"]
            .get(format!("{key}-nsis"))
            .or_else(|| value["platforms"].get(&key))
            .ok_or("Windows update artifact missing")?
    } else {
        &value
    };
    let pub_date = value["pub_date"]
        .as_str()
        .map(|date| chrono::DateTime::parse_from_rfc3339(date).map(|date| date.timestamp()))
        .transpose()
        .map_err(|e| e.to_string())?;
    Ok(Update {
        version: version.into(),
        notes: value["notes"].as_str().unwrap_or_default().into(),
        pub_date,
        url: platform["url"].as_str().ok_or("update URL missing")?.into(),
        signature: platform["signature"]
            .as_str()
            .filter(|signature| !signature.is_empty())
            .ok_or("update signature missing")?
            .into(),
    })
}

impl Update {
    pub fn download(&self) -> Result<Vec<u8>, String> {
        let bytes = get(&self.url, MAX_ARTIFACT)?;
        self.verify(&bytes)?;
        Ok(bytes)
    }

    fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        let configuration = config();
        let key = configuration["plugins"]["updater"]["pubkey"]
            .as_str()
            .ok_or("update public key missing")?;
        verify(bytes, &self.signature, key, &self.version)
    }

    pub fn install(&self, bytes: &[u8]) -> Result<(), String> {
        if nubi_core::isolation::development() {
            return Err("Updates are disabled in development mode".into());
        }
        self.verify(bytes)?;
        if !bytes.starts_with(b"MZ") {
            return Err("update is not an NSIS executable".into());
        }
        let directory = nubi_core::settings::data_dir().join("updates");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let path = directory.join(format!(
            "nubi-{}-{}-installer.exe",
            self.version,
            nubi_core::state::now_ms()
        ));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            // Hold a read-only sharing lock from final verification through process creation.
            let mut guard = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let mut actual = Vec::new();
            guard.read_to_end(&mut actual).map_err(|e| e.to_string())?;
            self.verify(&actual)?;
            let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let executable = wide(&path.to_string_lossy());
            let parameters = wide("/P /UPDATE /R /ARGS");
            let action = wide("open");
            let status = unsafe {
                windows_sys::Win32::UI::Shell::ShellExecuteW(
                    std::ptr::null_mut(),
                    action.as_ptr(),
                    executable.as_ptr(),
                    parameters.as_ptr(),
                    std::ptr::null(),
                    5,
                )
            } as isize;
            if status <= 32 {
                return Err(format!("installer launch failed: {status}"));
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        result
    }
}

fn verify(bytes: &[u8], signature: &str, key: &str, version: &str) -> Result<(), String> {
    let decode = |text: &str| -> Result<String, String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string())
    };
    let public_key =
        minisign_verify::PublicKey::decode(&decode(key)?).map_err(|e| e.to_string())?;
    let signature =
        minisign_verify::Signature::decode(&decode(signature)?).map_err(|e| e.to_string())?;
    public_key
        .verify(bytes, &signature, true)
        .map_err(|e| e.to_string())?;
    // Same compatibility policy as Tauri: old signatures can omit the version;
    // when a signed version exists it must match the unsigned manifest version.
    check_signed_version(signature.trusted_comment(), version)
}

fn check_signed_version(comment: &str, version: &str) -> Result<(), String> {
    if let Some(signed) = comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
    {
        let signed =
            semver::Version::parse(signed.trim_start_matches('v')).map_err(|e| e.to_string())?;
        let announced = semver::Version::parse(version).map_err(|e| e.to_string())?;
        if signed != announced {
            return Err("signed update version does not match manifest".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_compatibility_and_missing_signature() {
        let release = parse(br#"{"version":"v0.2.0","notes":"text","platforms":{"windows-x86_64":{"url":"https://example.test/app.exe","signature":"signed"}}}"#).unwrap();
        assert_eq!(release.version, "0.2.0");
        assert!(parse(br#"{"version":"0.2.0","url":"https://example.test/app.exe"}"#).is_err());
        assert!(release.verify(b"tampered or incomplete").is_err());
    }

    #[test]
    fn artifacts_require_valid_signature_bytes_key_and_trusted_comment() {
        // Public verification vector from minisign-verify, also exercising Tauri's
        // outer base64 encoding. This key is only used by this test.
        let key = base64::engine::general_purpose::STANDARD.encode("untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3");
        let signature = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature);
        assert!(verify(b"test", &encoded, &key, "0.2.0").is_ok());
        assert!(verify(b"Test", &encoded, &key, "0.2.0").is_err());
        assert!(verify(b"tes", &encoded, &key, "0.2.0").is_err());
        let changed = base64::engine::general_purpose::STANDARD
            .encode(signature.replace("file:test", "file:evil"));
        assert!(verify(b"test", &changed, &key, "0.2.0").is_err());
        let production = config();
        assert!(verify(
            b"test",
            &encoded,
            production["plugins"]["updater"]["pubkey"].as_str().unwrap(),
            "0.2.0"
        )
        .is_err());
        assert!(check_signed_version("file:installer\tversion:v0.2.0", "0.2.0").is_ok());
        assert!(check_signed_version("file:installer\tversion:0.1.3", "0.2.0").is_err());
    }
}
