//! Ordering of session status snapshots (port of `src/lib/session-store.ts`).

use translator_core::session::SessionStatus;

/// A late status of an older session never replaces a newer one.
pub fn newer_status(current: SessionStatus, incoming: SessionStatus) -> SessionStatus {
    if incoming.session_id < current.session_id {
        current
    } else {
        incoming
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_core::session::SessionPhase;

    #[test]
    fn late_events_cannot_replace_a_newer_session() {
        let mut current = SessionStatus::default();
        current.session_id = 2;
        current.phase = SessionPhase::Running;
        let mut late = SessionStatus::default();
        late.session_id = 1;
        assert_eq!(newer_status(current.clone(), late).session_id, 2);
        let mut next = SessionStatus::default();
        next.session_id = 3;
        assert_eq!(newer_status(current, next).session_id, 3);
    }
}
