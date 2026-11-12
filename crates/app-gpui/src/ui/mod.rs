//! GPUI views. `Shell` is the root view; the other modules add the pieces of its markup.

mod close_dialog;
mod diagnostics;
mod shell;
mod sidebar;
mod theme;
mod transcripts;
mod translation;
mod widgets;

pub use shell::Shell;
