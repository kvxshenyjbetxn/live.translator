use super::CaptureError;
use crate::{
    audio::{AudioMeters, CaptureQueues, FaultSignal},
    sources::{AudioSource, ResolvedSource},
};
use std::sync::Arc;
const MESSAGE: &str =
    "Захоплення програм на цій ОС ще не реалізовано. Перша версія працює на macOS 14.2+.";
pub fn list_sources() -> Result<Vec<AudioSource>, CaptureError> {
    Err(CaptureError(MESSAGE))
}
pub fn resolve_source(_: &str) -> Result<ResolvedSource, CaptureError> {
    Err(CaptureError(MESSAGE))
}
pub fn source_still_valid(_: &ResolvedSource) -> bool {
    false
}
pub struct CaptureHandle;
impl CaptureHandle {
    pub fn prepare(_: ResolvedSource) -> Result<Self, CaptureError> {
        Err(CaptureError(MESSAGE))
    }
    pub fn rate(&self) -> u32 {
        48000
    }
    pub fn start(
        &mut self,
        _: CaptureQueues,
        _: Arc<AudioMeters>,
        _: FaultSignal,
    ) -> Result<(), CaptureError> {
        Err(CaptureError(MESSAGE))
    }
    pub fn stop(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn no_fake_capture() {
        assert!(super::list_sources().is_err());
        assert!(super::resolve_source("fake").is_err());
    }
}
