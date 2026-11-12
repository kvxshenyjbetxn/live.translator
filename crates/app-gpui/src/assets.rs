//! Embedded UI assets (icons).

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

const EYE_OFF: &[u8] = include_bytes!("../assets/icons/eye-off.svg");

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "icons/eye-off.svg" => Some(Cow::Borrowed(EYE_OFF)),
            _ => None,
        })
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec!["icons/eye-off.svg".into()])
    }
}
