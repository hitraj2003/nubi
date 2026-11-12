//! Explicit process-wide isolation, selected before starting any services.
use std::sync::atomic::{AtomicBool, Ordering};

static DEVELOPMENT: AtomicBool = AtomicBool::new(false);

/// Call once during startup, before spawning threads or opening configuration.
pub fn enable_development() {
    DEVELOPMENT.store(true, Ordering::SeqCst);
}

pub fn development() -> bool {
    DEVELOPMENT.load(Ordering::SeqCst)
}
