//! GPUI views. `Shell` is the root view; the other modules add the pieces of its markup.

mod close_dialog;
mod diagnostics;
mod equalizer;
mod settings;
mod shell;
mod sidebar;
mod theme;
mod transcripts;
mod translation;
mod widgets;

pub use equalizer::{Equalizer, PILL_HEIGHT, PILL_WIDTH};
pub use shell::Shell;
