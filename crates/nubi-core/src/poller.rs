//! Background loop: usage polling (5-minute floor, persisted 429 backoff) and session sweep.
use crate::host::Host;
use crate::state::Usage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static KICK: AtomicBool = AtomicBool::new(false);
/// Set by the user's Refresh button: fetch every provider now, even during a 429 backoff and
/// while Claude's status line is live.
static FORCE: AtomicBool = AtomicBool::new(false);

pub fn force(_app: &impl Host) {
    FORCE.store(true, Ordering::SeqCst);
    KICK.store(true, Ordering::SeqCst);
}

/// Ask for a refresh on the next tick. Backoff from a 429 is still honoured.
pub fn kick(_app: &impl Host) {
    KICK.store(true, Ordering::SeqCst);
}

pub fn start(app: impl Host) {
    std::thread::spawn(move || {
        let mut next = crate::usage::load_backoff();
        loop {
            let kicked = KICK.swap(false, Ordering::SeqCst);
            let forced = FORCE.swap(false, Ordering::SeqCst);
            let now = (crate::state::now_ms() / 1000) as i64;
            let settings = {
                let st = app.shared();
                let mut g = st.0.lock().unwrap_or_else(|e| e.into_inner());
                g.sweep();
                g.settings.clone()
            };
            for p in ["claude", "codex"] {
                if !settings.wants(p) {
                    continue;
                }
                let due = next.get(p).copied().unwrap_or(0);
                let backing_off = {
                    let st = app.shared();
                    let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
                    g.usage.get(p).is_some_and(|u| u.status == "backoff")
                };
                if now < due && !forced && !(kicked && !backing_off) {
                    continue;
                }
                // Claude numbers arrive from the status line while a session is open; the API is
                // only a fallback when none has reported for 30 minutes.
                if p == "claude" && !forced {
                    let st = app.shared();
                    let live =
                        st.0.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .claude_live_ms;
                    if live > 0 && crate::state::now_ms().saturating_sub(live) < 30 * 60 * 1000 {
                        continue;
                    }
                }
                let out = crate::usage::fetch(p);
                let st = app.shared();
                let mut g = st.0.lock().unwrap_or_else(|e| e.into_inner());
                let (u, n): (Usage, i64) = crate::usage::apply(p, out, g.usage.get(p));
                g.usage.insert(p.to_string(), u);
                // Claude's API is only a fallback now: ask it at most every 30 minutes.
                let n = if p == "claude" {
                    n.max(now + 30 * 60)
                } else {
                    n
                };
                next.insert(p.to_string(), n);
                let mut persisted = next.clone();
                persisted.retain(|k, _| g.usage.get(k).is_some_and(|x| x.status == "backoff"));
                drop(g);
                crate::usage::save_backoff(&persisted);
            }
            app.emit_state();
            for _ in 0..30 {
                if KICK.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}
