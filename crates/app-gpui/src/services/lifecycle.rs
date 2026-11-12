//! Close gating (port of `src-tauri/src/lifecycle.rs`).

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct CloseState {
    ready: AtomicBool,
    allowed: AtomicBool,
}

impl CloseState {
    pub fn should_intercept(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && !self.allowed.load(Ordering::SeqCst)
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// The UI is attached and can handle desktop actions and close requests.
    pub fn set_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    /// The close sequence finished; the process may exit without being intercepted again.
    pub fn allow_exit(&self) {
        self.allowed.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unready_or_finished_window_does_not_trap_exit() {
        let state = CloseState::default();
        assert!(!state.should_intercept());
        state.set_ready();
        assert!(state.should_intercept());
        state.allow_exit();
        assert!(!state.should_intercept());
    }
}
