#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{list_sources, resolve_source, source_still_valid, CaptureHandle};
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub use unsupported::*;
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CaptureError(pub &'static str);
