//! Tauri command wrappers for the core report service.
use crate::app::App;
use crate::core_host::TauriHost;
use serde_json::{Map, Value};

#[tauri::command]
pub async fn report_context(app: App) -> Value {
    nubi_core::report::report_context(TauriHost::new(app)).await
}
#[tauri::command]
pub async fn draft_report(agent: String, text: String, context: Value) -> Result<Value, String> {
    nubi_core::report::draft_report(agent, text, context).await
}
#[tauri::command]
pub fn open_report(fields: Map<String, Value>) { nubi_core::report::open_report(fields); }
#[tauri::command]
pub async fn list_releases(app: App) -> Result<Value, String> {
    nubi_core::report::list_releases(TauriHost::new(app)).await
}
#[tauri::command]
pub fn open_release_link(url: String) -> Result<(), String> { nubi_core::report::open_release_link(url) }
