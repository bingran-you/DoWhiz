use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub fn set_shutdown_in_progress() {
    SHUTDOWN_IN_PROGRESS.store(true, Ordering::SeqCst);
}

pub fn is_shutdown_in_progress() -> bool {
    SHUTDOWN_IN_PROGRESS.load(Ordering::SeqCst)
}
