//! Auto-update (release builds only). Checks the GitHub latest.json at start and every 6 hours.
//! When a newer signed version is there, it downloads the NSIS installer, runs it in passive mode
//! (small progress window, no questions) and restarts the app. The signature is checked against
//! the public key in tauri.conf.json before anything runs.
use std::time::Duration;
use tauri::AppHandle;

pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        // Let the app settle first, then check every 6 hours.
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if let Err(e) = check(&app).await {
                eprintln!("[nubi] update check: {e}");
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

async fn check(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_updater::UpdaterExt;
    let Some(update) = app.updater()?.check().await? else { return Ok(()) };
    eprintln!("[nubi] updating to {}", update.version);
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart();
}
