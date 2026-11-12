//! A partial settings edit (the Rust counterpart of `Partial<Settings>` in `App.svelte`).

use translator_core::settings::{Settings, ShortcutMode, SourceRef};

#[derive(Clone, Default)]
pub struct Patch {
    pub api_key: Option<String>,
    pub target_language: Option<String>,
    pub theme: Option<String>,
    pub accent_color: Option<String>,
    pub interface_language: Option<String>,
    pub original_volume: Option<f32>,
    pub translation_volume: Option<f32>,
    pub selected_source_id: Option<Option<String>>,
    pub capture_shortcut: Option<Option<String>>,
    pub shortcut_mode: Option<ShortcutMode>,
    pub last_captured_source_id: Option<Option<String>>,
    pub favorite_sources: Option<Vec<SourceRef>>,
    pub hidden_sources: Option<Vec<SourceRef>>,
}

impl Patch {
    pub fn selected_source(id: Option<String>) -> Self {
        Self {
            selected_source_id: Some(id),
            ..Self::default()
        }
    }

    pub fn target_language(code: impl Into<String>) -> Self {
        Self {
            target_language: Some(code.into()),
            ..Self::default()
        }
    }

    pub fn volumes(&self) -> bool {
        self.original_volume.is_some() || self.translation_volume.is_some()
    }

    pub fn apply(&self, settings: &mut Settings) {
        let patch = self.clone();
        macro_rules! set {
            ($($field:ident),*) => { $(if let Some(value) = patch.$field { settings.$field = value; })* };
        }
        set!(
            api_key,
            target_language,
            theme,
            accent_color,
            interface_language,
            original_volume,
            translation_volume,
            selected_source_id,
            capture_shortcut,
            shortcut_mode,
            last_captured_source_id,
            favorite_sources,
            hidden_sources
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_present_fields_are_applied() {
        let mut settings = Settings::default();
        Patch {
            original_volume: Some(0.5),
            selected_source_id: Some(Some("a".into())),
            ..Patch::default()
        }
        .apply(&mut settings);
        assert_eq!(settings.original_volume, 0.5);
        assert_eq!(settings.selected_source_id.as_deref(), Some("a"));
        assert_eq!(settings.translation_volume, 0.7);
        Patch::selected_source(None).apply(&mut settings);
        assert_eq!(settings.selected_source_id, None);
    }
}
