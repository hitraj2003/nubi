//! Signed updates with silent downloads and explicit installation.
use crate::host::NativeHost as AppHandle;
use crate::update_transport::Update;
use nubi_core::host::Host;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

const STARTUP_DELAY_SECS: u64 = 3;
const CHECK_INTERVAL_SECS: u64 = 6 * 3600;
const OFFER_MS: u64 = 10_000;

#[derive(Clone, Serialize)]
pub struct Notice {
    pub id: u64,
    pub version: String,
    pub notes: String,
    pub phase: String,
    pub automatic: bool,
    pub expires_ms: u64,
    pub held: bool,
    pub message: String,
}

#[derive(Default)]
pub(crate) struct Inner {
    candidate: Option<Update>,
    downloaded: Option<Arc<[u8]>>,
    notice: Option<Notice>,
    next_id: u64,
    checking: bool,
    installing: bool,
}

#[derive(Debug, PartialEq)]
enum Tick {
    Stop,
    Wait,
    Dismiss,
}

impl Inner {
    fn tick(&mut self, id: u64, now: u64, elapsed: u64) -> Tick {
        let Some(n) = self.notice.as_mut().filter(|n| n.id == id) else {
            return Tick::Stop;
        };
        if n.phase != "available" || n.automatic {
            return Tick::Stop;
        }
        if n.held {
            if n.expires_ms > 0 {
                n.expires_ms = n.expires_ms.saturating_add(elapsed);
            }
            return Tick::Wait;
        }
        // Zero means the notch card has not acknowledged being displayed yet.
        if n.expires_ms == 0 || now < n.expires_ms {
            Tick::Wait
        } else {
            Tick::Dismiss
        }
    }

    fn offer(&mut self, version: String, notes: String, automatic: bool) -> u64 {
        self.next_id += 1;
        self.notice = Some(Notice {
            id: self.next_id,
            version,
            notes,
            phase: if automatic { "ready" } else { "available" }.into(),
            automatic,
            expires_ms: 0,
            held: false,
            message: String::new(),
        });
        self.next_id
    }

    fn dismiss(&mut self, id: u64) {
        if self
            .notice
            .as_ref()
            .is_some_and(|n| n.id == id && n.phase != "installing")
        {
            // Keep the candidate and verified bytes for the next periodic check.
            self.notice = None;
        }
    }

    fn download_current(&self, id: u64) -> bool {
        self.notice
            .as_ref()
            .is_some_and(|n| n.id == id && n.phase == "downloading")
    }
}

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut Inner) -> T) -> T {
    let mut inner = app.updates.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut inner)
}

fn emit(app: &AppHandle) {
    app.emit_state();
    let _ = app.post(crate::host::UiEvent::Update);
}

fn show(app: &AppHandle) -> Result<(), String> {
    app.post(crate::host::UiEvent::Update)
        .map_err(|e| e.to_string())
}

pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) || nubi_core::isolation::development() {
        if let Some(automatic) = std::env::var(DEV_SAMPLE_ENV)
            .ok()
            .as_deref()
            .and_then(dev_sample_mode)
        {
            show_dev_sample(app, automatic);
        }
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;
        loop {
            if let Err(error) = check(&app).await {
                eprintln!("[nubi] update check: {error}");
            }
            tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
        }
    });
}

/// Seconds between this build and a manifest published for it (build, sign, upload). A manifest
/// for the same version published later than this is a rebuild of that version.
const REBUILD_GRACE_SECS: i64 = 2 * 60 * 60;
/// A higher version, or the same version rebuilt and published after this build.
fn newer(current: semver::Version, remote: &Update) -> bool {
    let Ok(version) = semver::Version::parse(&remote.version) else {
        return false;
    };
    if version != current {
        return version > current;
    }
    let built: i64 = env!("NUBI_BUILD_TIME").parse().unwrap_or(i64::MAX);
    remote
        .pub_date
        .is_some_and(|date| date > built.saturating_add(REBUILD_GRACE_SECS))
}

async fn check(app: &AppHandle) -> Result<String, String> {
    if !with(app, |i| {
        if i.checking || i.installing {
            return false;
        }
        // Also covers the silent download, so checks cannot duplicate it.
        i.checking = true;
        true
    }) {
        return Ok("An update check or download is already in progress".into());
    }
    let result = async {
        let update = tokio::task::spawn_blocking(crate::update_transport::check)
            .await
            .map_err(|e| e.to_string())??;
        let Some(update) = update.filter(|remote| {
            newer(
                semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("package version"),
                remote,
            )
        }) else {
            return Ok("You have the latest version".into());
        };
        let automatic = nubi_core::host::with(app, |i| i.settings.auto_update);
        check_candidate(app, update, automatic, |update| update.download()).await?;
        Ok("Update available. See the notification".into())
    }
    .await;
    with(app, |i| i.checking = false);
    result
}

/// Shares the real check flow with network-free lifecycle tests.
async fn check_candidate(
    app: &AppHandle,
    update: Update,
    automatic: bool,
    download: impl FnOnce(Update) -> Result<Vec<u8>, String> + Send + 'static,
) -> Result<(), String> {
    let prepare = with(app, |i| {
        if i.installing {
            return None;
        }
        if i.candidate.as_ref() == Some(&update)
            && i.notice.as_ref().is_some_and(|n| n.automatic == automatic)
        {
            return None;
        }
        if i.candidate.as_ref() != Some(&update) {
            i.candidate = Some(update.clone());
            i.downloaded = None;
        }
        let removed_notice = i.notice.take().is_some();
        Some((automatic && i.downloaded.is_none(), removed_notice))
    });
    let Some((needs_download, removed_notice)) = prepare else {
        return Ok(());
    };
    if removed_notice {
        emit(app);
    }
    if needs_download {
        let downloader = update.clone();
        let result = tokio::task::spawn_blocking(move || download(downloader))
            .await
            .map_err(|e| e.to_string())
            .and_then(|value| value);
        match result {
            Ok(bytes) => with(app, |i| i.downloaded = Some(bytes.into())),
            Err(error) => {
                eprintln!("[nubi] background update download: {error}");
                return Err(error);
            }
        }
    }
    let id = with(app, |i| i.offer(update.version, update.notes, automatic));
    emit(app);
    if let Err(error) = show(app) {
        dismiss(app, id);
        return Err(error);
    }
    if !automatic {
        watch_notice(app.clone(), id);
    }
    Ok(())
}

/// Development samples never check the network or store installer bytes.
const DEV_SAMPLE_ENV: &str = "NUBI_DEV_UPDATE_NOTICE";

fn dev_sample_mode(value: &str) -> Option<bool> {
    match value {
        "on" => Some(true),
        "off" | "1" => Some(false),
        _ => None,
    }
}

fn show_dev_sample(app: AppHandle, automatic: bool) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let id = with(&app, |i| {
            i.offer(
                "9.9.9-dev".into(),
                "Development sample notice. Nothing is downloaded or installed.".into(),
                automatic,
            )
        });
        emit(&app);
        if show(&app).is_ok() && !automatic {
            watch_notice(app, id);
        }
    });
}

fn watch_notice(app: AppHandle, id: u64) {
    tokio::spawn(async move {
        let mut last = nubi_core::state::now_ms();
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let now = nubi_core::state::now_ms();
            let action = with(&app, |i| i.tick(id, now, now.saturating_sub(last)));
            last = now;
            match action {
                Tick::Stop => return,
                Tick::Dismiss => {
                    dismiss(&app, id);
                    return;
                }
                Tick::Wait => {}
            }
        }
    });
}

async fn install(
    app: &AppHandle,
    id: u64,
    download: impl FnOnce(Update) -> Result<Vec<u8>, String> + Send + 'static,
    run_installer: impl FnOnce(Update, Arc<[u8]>) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    let (update, cached) = with(app, |i| {
        if i.installing {
            return Err("An update is already in progress");
        }
        let n = i
            .notice
            .as_mut()
            .filter(|n| n.id == id)
            .ok_or("Update offer has expired")?;
        if !matches!(n.phase.as_str(), "available" | "ready" | "error") {
            return Err("Update is already in progress");
        }
        let update = i.candidate.clone().ok_or("Check for updates again")?;
        let cached = i.downloaded.clone();
        i.installing = true;
        n.phase = if cached.is_some() {
            "installing"
        } else {
            "downloading"
        }
        .into();
        n.expires_ms = 0;
        n.message.clear();
        Ok((update, cached))
    })
    .map_err(str::to_string)?;
    emit(app);
    let result: Result<(), String> = async {
        let bytes = if let Some(bytes) = cached {
            bytes
        } else {
            let downloader = update.clone();
            let downloaded = tokio::task::spawn_blocking(move || download(downloader))
                .await
                .map_err(|e| e.to_string())
                .and_then(|value| value);
            // Check cancellation and enter the non-cancellable phase under the same lock.
            let bytes = with(app, |i| {
                if !i.download_current(id) {
                    i.installing = false;
                    return Ok(None);
                }
                let bytes: Arc<[u8]> = downloaded?.into();
                i.downloaded = Some(bytes.clone());
                if let Some(n) = i.notice.as_mut().filter(|n| n.id == id) {
                    n.phase = "installing".into();
                }
                Ok::<_, String>(Some(bytes))
            })?;
            let Some(bytes) = bytes else { return Ok(()) };
            emit(app);
            bytes
        };
        tokio::task::spawn_blocking(move || run_installer(update, bytes))
            .await
            .map_err(|e| e.to_string())??;
        Ok(())
    }
    .await;
    if let Err(error) = &result {
        with(app, |i| i.installing = false);
        install_error(app, id, error);
    }
    result
}

fn install_error(app: &AppHandle, id: u64, error: &str) {
    let changed = with(app, |i| {
        // A duplicate or stale callback cannot replace an active operation's card.
        if i.installing {
            return false;
        }
        let Some(n) = i.notice.as_mut().filter(|n| n.id == id) else {
            return false;
        };
        n.phase = "error".into();
        n.expires_ms = 0;
        n.message = format!("Update failed: {error}");
        true
    });
    if changed {
        emit(app);
    }
}

fn dismiss(app: &AppHandle, id: u64) {
    with(app, |i| i.dismiss(id));
    emit(app);
}

pub fn get_update_notice(app: AppHandle) -> Option<Notice> {
    with(&app, |i| i.notice.clone())
}

pub fn update_notice_ready(app: AppHandle, id: u64) {
    with(&app, |i| {
        if let Some(n) = i
            .notice
            .as_mut()
            .filter(|n| n.id == id && !n.automatic && n.phase == "available" && n.expires_ms == 0)
        {
            n.expires_ms = nubi_core::state::now_ms() + OFFER_MS;
        }
    });
    emit(&app);
}

pub fn dismiss_update(app: AppHandle, id: u64) {
    dismiss(&app, id);
}

pub fn hold_update(app: AppHandle, id: u64, on: bool) {
    with(&app, |i| {
        if let Some(n) = i.notice.as_mut().filter(|n| n.id == id) {
            n.held = on;
        }
    });
    emit(&app);
}

pub async fn install_update(app: AppHandle, id: u64) -> Result<(), String> {
    let result = if cfg!(debug_assertions) || nubi_core::isolation::development() {
        Err("Updates only work in release builds".into())
    } else {
        install(
            &app,
            id,
            |update| update.download(),
            |update, bytes| {
                update.install(&bytes)?;
                // Installer shutdown must not be dropped when the bounded UI queue is full.
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
                Ok(())
            },
        )
        .await
    };
    if let Err(error) = &result {
        install_error(&app, id, error);
    }
    result
}

pub async fn check_update(app: AppHandle) -> Result<String, String> {
    if cfg!(debug_assertions) || nubi_core::isolation::development() {
        return Ok("Updates only work in release builds".into());
    }
    check(&app).await
}

#[cfg(test)]
pub(crate) fn seed_notice(app: &AppHandle, automatic: bool) {
    with(app, |i| {
        i.offer("9.9.9-dev".into(), String::new(), automatic);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_policy_rejects_rollback_and_requires_later_rebuild_date() {
        let current = semver::Version::parse("0.1.3").unwrap();
        let release = |version: &str, date: &str| {
            crate::update_transport::parse(format!(r#"{{"version":"{version}","pub_date":"{date}","url":"https://example.test/app.exe","signature":"test"}}"#).as_bytes()).unwrap()
        };
        assert!(newer(
            current.clone(),
            &release("0.1.4", "2020-01-01T00:00:00Z")
        ));
        assert!(!newer(
            current.clone(),
            &release("0.1.2", "2099-01-01T00:00:00Z")
        ));
        assert!(!newer(
            current.clone(),
            &release("0.1.3", "2020-01-01T00:00:00Z")
        ));
        assert!(newer(current, &release("0.1.3", "2099-01-01T00:00:00Z")));
    }

    fn release(version: &str) -> Update {
        crate::update_transport::parse(format!(r#"{{"version":"{version}","url":"https://example.test/app.exe","signature":"test"}}"#).as_bytes()).unwrap()
    }

    fn host() -> (AppHandle, std::sync::mpsc::Receiver<crate::host::UiEvent>) {
        AppHandle::from_settings(nubi_core::settings::Settings::default())
    }

    #[tokio::test]
    async fn automatic_download_precedes_persistent_card_and_later_reuses_bytes() {
        let (app, _events) = host();
        let observed = app.clone();
        check_candidate(&app, release("0.2.0"), true, move |_| {
            assert!(get_update_notice(observed.clone()).is_none());
            assert!(with(&observed, |i| i.downloaded.is_none()));
            Ok(b"verified installer".to_vec())
        })
        .await
        .unwrap();
        let notice = get_update_notice(app.clone()).unwrap();
        assert_eq!(notice.phase, "ready");
        assert!(notice.automatic);
        update_notice_ready(app.clone(), notice.id);
        assert_eq!(get_update_notice(app.clone()).unwrap().expires_ms, 0);
        assert_eq!(with(&app, |i| i.tick(notice.id, u64::MAX, 250)), Tick::Stop);
        assert!(!with(&app, |i| i.installing));
        let bytes = with(&app, |i| i.downloaded.clone().unwrap());
        dismiss_update(app.clone(), notice.id);
        assert!(get_update_notice(app.clone()).is_none());
        check_candidate(&app, release("0.2.0"), true, |_| {
            panic!("must not download again")
        })
        .await
        .unwrap();
        let next = get_update_notice(app.clone()).unwrap();
        assert!(next.id > notice.id);
        assert_eq!(next.phase, "ready");
        assert_eq!(next.expires_ms, 0);
        assert!(Arc::ptr_eq(
            &bytes,
            &with(&app, |i| i.downloaded.clone().unwrap())
        ));
        let observed = app.clone();
        install(
            &app,
            next.id,
            |_| panic!("ready install must use cached bytes"),
            move |_, installed| {
                assert!(Arc::ptr_eq(&installed, &bytes));
                assert_eq!(get_update_notice(observed).unwrap().phase, "installing");
                Ok(())
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn automatic_download_failure_is_silent_and_next_check_retries() {
        let (app, _events) = host();
        assert!(
            check_candidate(&app, release("0.2.0"), true, |_| Err("offline".into()))
                .await
                .is_err()
        );
        assert!(get_update_notice(app.clone()).is_none());
        assert!(with(&app, |i| i.downloaded.is_none()));
        check_candidate(&app, release("0.2.0"), true, |_| Ok(vec![1, 2, 3]))
            .await
            .unwrap();
        assert_eq!(get_update_notice(app).unwrap().phase, "ready");
    }

    #[tokio::test]
    async fn manual_offer_waits_for_display_pauses_and_dismisses_after_ten_seconds() {
        let (app, _events) = host();
        check_candidate(&app, release("0.2.0"), false, |_| {
            panic!("manual offer cannot download")
        })
        .await
        .unwrap();
        let notice = get_update_notice(app.clone()).unwrap();
        assert_eq!(notice.phase, "available");
        assert!(!notice.automatic);
        assert!(with(&app, |i| i.downloaded.is_none()));
        assert_eq!(with(&app, |i| i.tick(notice.id, u64::MAX, 250)), Tick::Wait);
        update_notice_ready(app.clone(), notice.id);
        let expires = get_update_notice(app.clone()).unwrap().expires_ms;
        assert!((9_900..=10_000).contains(&expires.saturating_sub(nubi_core::state::now_ms())));
        hold_update(app.clone(), notice.id, true);
        assert_eq!(with(&app, |i| i.tick(notice.id, expires, 250)), Tick::Wait);
        assert_eq!(
            get_update_notice(app.clone()).unwrap().expires_ms,
            expires + 250
        );
        hold_update(app.clone(), notice.id, false);
        assert_eq!(
            with(&app, |i| i.tick(notice.id, expires + 249, 1)),
            Tick::Wait
        );
        assert_eq!(
            with(&app, |i| i.tick(notice.id, expires + 250, 1)),
            Tick::Dismiss
        );
        dismiss_update(app.clone(), notice.id);
        assert!(get_update_notice(app).is_none());
    }

    #[tokio::test]
    async fn manual_update_downloads_then_installs_without_another_prompt() {
        let (app, _events) = host();
        check_candidate(&app, release("0.2.0"), false, |_| {
            panic!("no early download")
        })
        .await
        .unwrap();
        let notice = get_update_notice(app.clone()).unwrap();
        let downloading = app.clone();
        let installing = app.clone();
        let installed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = installed.clone();
        install(
            &app,
            notice.id,
            move |_| {
                let notice = get_update_notice(downloading).unwrap();
                assert_eq!(notice.phase, "downloading");
                assert_eq!(notice.expires_ms, 0);
                Ok(b"verified bytes".to_vec())
            },
            move |_, bytes| {
                let current = get_update_notice(installing).unwrap();
                assert_eq!(current.id, notice.id);
                assert_eq!(current.phase, "installing");
                assert_eq!(&*bytes, b"verified bytes");
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(installed.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cancelled_manual_download_cannot_install_and_stale_callbacks_do_not_change_new_card() {
        let (app, _events) = host();
        check_candidate(&app, release("0.2.0"), false, |_| unreachable!())
            .await
            .unwrap();
        let id = get_update_notice(app.clone()).unwrap().id;
        let cancelled = app.clone();
        install(
            &app,
            id,
            move |_| {
                dismiss_update(cancelled.clone(), id);
                seed_notice(&cancelled, false);
                Ok(vec![1])
            },
            |_, _| panic!("cancelled download must not install"),
        )
        .await
        .unwrap();
        assert!(!with(&app, |i| i.installing));
        assert!(with(&app, |i| i.downloaded.is_none()));
        let next = get_update_notice(app.clone()).unwrap();
        assert!(next.id > id);
        dismiss_update(app.clone(), id);
        install_error(&app, id, "stale");
        assert_eq!(get_update_notice(app).unwrap().phase, "available");
    }

    #[tokio::test]
    async fn manual_download_and_install_errors_allow_retry_and_later() {
        let (app, _events) = host();
        check_candidate(&app, release("0.2.0"), false, |_| unreachable!())
            .await
            .unwrap();
        let id = get_update_notice(app.clone()).unwrap().id;
        assert!(
            install(&app, id, |_| Err("offline".into()), |_, _| unreachable!())
                .await
                .is_err()
        );
        let error = get_update_notice(app.clone()).unwrap();
        assert_eq!(error.phase, "error");
        assert!(error.message.contains("offline"));
        assert_eq!(error.expires_ms, 0);
        assert_eq!(with(&app, |i| i.tick(id, u64::MAX, 250)), Tick::Stop);
        assert!(install(
            &app,
            id,
            |_| Ok(vec![1]),
            |_, _| Err("installer blocked".into())
        )
        .await
        .is_err());
        assert!(get_update_notice(app.clone())
            .unwrap()
            .message
            .contains("installer blocked"));
        assert!(!with(&app, |i| i.installing));
        install(
            &app,
            id,
            |_| panic!("retry should retain verified bytes"),
            |_, bytes| {
                assert_eq!(&*bytes, &[1]);
                Ok(())
            },
        )
        .await
        .unwrap();
        dismiss_update(app.clone(), id);
        assert_eq!(
            get_update_notice(app.clone()).unwrap().phase,
            "installing",
            "cannot cancel installer"
        );
        install_error(&app, id, "duplicate click");
        assert_eq!(get_update_notice(app).unwrap().phase, "installing");
    }

    #[tokio::test]
    async fn changed_release_invalidates_cached_bytes_and_existing_offer_does_not_redownload() {
        let (app, _events) = host();
        check_candidate(&app, release("0.2.0"), true, |_| Ok(vec![1]))
            .await
            .unwrap();
        let id = get_update_notice(app.clone()).unwrap().id;
        check_candidate(&app, release("0.2.0"), true, |_| panic!("already shown"))
            .await
            .unwrap();
        assert_eq!(get_update_notice(app.clone()).unwrap().id, id);
        let observed = app.clone();
        check_candidate(&app, release("0.3.0"), true, move |_| {
            assert!(get_update_notice(observed.clone()).is_none());
            assert!(with(&observed, |i| i.downloaded.is_none()));
            Ok(vec![2])
        })
        .await
        .unwrap();
        assert_eq!(get_update_notice(app.clone()).unwrap().version, "0.3.0");
        assert_eq!(&*with(&app, |i| i.downloaded.clone().unwrap()), &[2]);
    }

    #[test]
    fn dev_switch_supports_both_card_modes() {
        assert_eq!(dev_sample_mode("on"), Some(true));
        assert_eq!(dev_sample_mode("off"), Some(false));
        assert_eq!(dev_sample_mode("1"), Some(false));
        assert_eq!(dev_sample_mode(""), None);
    }

    #[tokio::test]
    async fn both_dev_samples_report_release_only_error_and_remain_retryable() {
        for automatic in [false, true] {
            let (app, _events) = host();
            seed_notice(&app, automatic);
            let notice = get_update_notice(app.clone()).unwrap();
            assert_eq!(notice.phase, if automatic { "ready" } else { "available" });
            for _ in 0..2 {
                assert_eq!(
                    install_update(app.clone(), notice.id).await.unwrap_err(),
                    "Updates only work in release builds"
                );
                let error = get_update_notice(app.clone()).unwrap();
                assert_eq!(error.phase, "error");
                assert_eq!(error.expires_ms, 0);
                assert!(error
                    .message
                    .contains("Updates only work in release builds"));
                assert!(!with(&app, |i| i.installing));
                assert!(with(&app, |i| i.candidate.is_none() && i.downloaded.is_none()));
            }
            dismiss_update(app.clone(), notice.id);
            assert!(get_update_notice(app).is_none());
        }
    }
}
