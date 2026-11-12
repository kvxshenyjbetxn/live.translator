//! Close sequence (port of `src/lib/close.ts`). The caller performs the steps in order; this
//! module owns the ordering rules so they stay testable.

/// One step of the shutdown sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseStep {
    /// Gate new starts and ask the session to stop.
    Begin,
    /// Persist the latest settings (skipped when discarding).
    Flush,
    /// Stop capture and wait for audio cleanup.
    Stop,
    /// Exit the process.
    Finish,
}

/// Steps of a close request; `save = false` is the explicit "exit without saving".
pub fn close_steps(save: bool) -> Vec<CloseStep> {
    let mut steps = vec![CloseStep::Begin];
    if save {
        steps.push(CloseStep::Flush);
    }
    steps.push(CloseStep::Stop);
    steps.push(CloseStep::Finish);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_gates_flushes_stops_and_then_exits() {
        assert_eq!(
            close_steps(true),
            [CloseStep::Begin, CloseStep::Flush, CloseStep::Stop, CloseStep::Finish]
        );
    }

    #[test]
    fn discard_skips_the_save_but_still_waits_for_audio_cleanup() {
        assert_eq!(
            close_steps(false),
            [CloseStep::Begin, CloseStep::Stop, CloseStep::Finish]
        );
    }
}
