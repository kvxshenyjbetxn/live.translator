#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{list_sources, resolve_source, source_still_valid, CaptureHandle};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use unsupported::*;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{
    foreground_process, list_sources, resolve_source, source_still_valid, CaptureHandle,
};
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("{0}")]
pub struct CaptureError(pub &'static str);
