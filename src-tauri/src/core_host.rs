//! Tauri adapter for the UI-independent core services.
use crate::app::{self, App};
use nubi_core::host::{Host, Shared};
use nubi_core::protocol::{BrowserMsg, BrowserReply, HookMsg, Reply};
use tauri::Manager;
use tokio::sync::oneshot;

#[derive(Clone)]
pub struct TauriHost {
    app: App,
    shared: Shared,
}

impl TauriHost {
    pub fn new(app: App) -> Self {
        let shared = app.state::<Shared>().inner().clone();
        Self { app, shared }
    }
}

impl Host for TauriHost {
    fn shared(&self) -> &Shared {
        &self.shared
    }
    fn version(&self) -> String {
        self.app.package_info().version.to_string()
    }
    fn emit_state(&self) {
        app::emit_state(&self.app);
    }
    fn on_hook(&self, message: &HookMsg) -> bool {
        app::on_hook(&self.app, message)
    }
    fn on_browser(&self, message: BrowserMsg) -> BrowserReply {
        crate::desktime::on_browser(&self.app, message)
    }
    fn add_pending(&self, message: &HookMsg, reply: oneshot::Sender<Reply>, ttl_ms: u64) -> u64 {
        app::add_pending(&self.app, message, reply, ttl_ms)
    }
    fn pending_tick(&self, id: u64, step_ms: u64, hard_ms: u64) -> bool {
        app::pending_tick(&self.app, id, step_ms, hard_ms)
    }
    fn remove_pending(&self, id: u64, timed_out: bool) {
        app::remove_pending(&self.app, id, timed_out);
    }
}
