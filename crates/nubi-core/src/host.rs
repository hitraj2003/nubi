//! Host boundary for domain services. Implementations marshal UI work to their event loop.
use crate::protocol::{BrowserMsg, BrowserReply, HookMsg, Reply};
use crate::state::Inner;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Clone, Default)]
pub struct Shared(pub Arc<Mutex<Inner>>);

/// The pipe preserves ordered decisions; hosts may coalesce replaceable state snapshots.
/// Callbacks run on workers and must not depend on a particular UI runtime.
pub trait Host: Clone + Send + Sync + 'static {
    fn shared(&self) -> &Shared;
    fn version(&self) -> String;
    fn emit_state(&self);
    fn on_hook(&self, message: &HookMsg) -> bool;
    fn on_browser(&self, message: BrowserMsg) -> BrowserReply;
    fn add_pending(&self, message: &HookMsg, reply: oneshot::Sender<Reply>, ttl_ms: u64) -> u64;
    fn pending_tick(&self, id: u64, step_ms: u64, hard_ms: u64) -> bool;
    fn remove_pending(&self, id: u64, timed_out: bool);

    fn set_pipe_error(&self, error: String) {
        eprintln!("[nubi] {error}");
        with(self, |state| state.pipe_error = Some(error));
        self.emit_state();
    }

    fn questions_mode(&self) -> String {
        with(self, |state| state.settings.questions_mode.clone())
    }

    fn takeover_secs(&self) -> u64 {
        with(self, |state| state.settings.takeover_secs as u64)
    }
}

pub fn with<H: Host, R>(host: &H, f: impl FnOnce(&mut Inner) -> R) -> R {
    let mut state = host
        .shared()
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    f(&mut state)
}
