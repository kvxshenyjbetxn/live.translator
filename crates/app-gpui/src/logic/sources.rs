//! Source catalog, favorites, hidden records and preference preflight (port of
//! `src/lib/sources.ts`).

use std::collections::HashSet;
use translator_core::{
    settings::{Settings, SourceRef},
    sources::AudioSource,
};

/// Mirrors `crates/translator-core/src/settings.rs`; the core stays authoritative.
pub const MAX_FAVORITES: usize = 32;
pub const MAX_HIDDEN: usize = 32;
pub const MAX_SOURCE_ID_BYTES: usize = 512;
pub const MAX_SOURCE_NAME_BYTES: usize = 128;
pub const MAX_SERIALIZED_BYTES: usize = 60 * 1024;

pub const TOO_LARGE: &str =
    "Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.";
const FAVORITES_FULL: &str = "Можна додати не більше 32 програм до обраних.";
const HIDDEN_FULL: &str = "Можна приховати не більше 32 програм.";

#[derive(Clone, Debug, PartialEq)]
pub struct SourceView {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub detail: Option<String>,
    pub favorite: bool,
    pub available: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceCatalog {
    pub visible: Vec<SourceView>,
    pub hidden: Vec<SourceView>,
    pub all: Vec<SourceView>,
}

/// The preference lists a patch would persist.
#[derive(Default)]
pub struct PreferencePatch {
    pub favorite_sources: Option<Vec<SourceRef>>,
    pub hidden_sources: Option<Vec<SourceRef>>,
}

pub fn source_ref(id: &str, name: &str) -> SourceRef {
    SourceRef {
        id: id.into(),
        name: name.into(),
    }
}

/// Compact JSON doubled over-counts the pretty form written by the core (indentation and per-line
/// keys are not counted). It is a conservative proxy, not a guarantee: the core stays
/// authoritative, and an allowed shrinking patch can still be refused there by the real limit.
fn serialized_estimate(settings: &Settings) -> usize {
    serde_json::to_vec(settings).map_or(usize::MAX, |bytes| bytes.len() * 2)
}

/// One list: favorites in starred order, then the remaining enumeration order, then hidden records
/// that are neither. Enumeration is the only availability truth; a record with no live source is
/// an unavailable placeholder. Hiding filters that order and never drops a row; first id wins.
pub fn source_catalog(
    available: &[AudioSource],
    favorites: &[SourceRef],
    hidden: &[SourceRef],
) -> SourceCatalog {
    let mut live: Vec<&AudioSource> = Vec::new();
    let mut seen = HashSet::new();
    for source in available {
        if seen.insert(source.id.as_str()) {
            live.push(source);
        }
    }
    let hidden_ids: HashSet<&str> = hidden.iter().map(|record| record.id.as_str()).collect();
    let mut done: HashSet<String> = HashSet::new();
    let mut all = Vec::new();
    let mut include = |id: &str, name: &str, favorite: bool| {
        if !done.insert(id.to_string()) {
            return;
        }
        all.push(match live.iter().find(|source| source.id == id) {
            Some(source) => SourceView {
                id: id.into(),
                name: source.name.clone(),
                active: source.active,
                detail: source.detail.clone(),
                favorite,
                available: true,
            },
            None => SourceView {
                id: id.into(),
                name: name.into(),
                active: false,
                detail: None,
                favorite,
                available: false,
            },
        });
    };
    for record in favorites {
        include(&record.id, &record.name, true);
    }
    for source in &live {
        include(&source.id, &source.name, false);
    }
    for record in hidden {
        include(&record.id, &record.name, false);
    }
    SourceCatalog {
        visible: all
            .iter()
            .filter(|view| !hidden_ids.contains(view.id.as_str()))
            .cloned()
            .collect(),
        hidden: all
            .iter()
            .filter(|view| hidden_ids.contains(view.id.as_str()))
            .cloned()
            .collect(),
        all,
    }
}

pub fn is_favorite(favorites: &[SourceRef], id: &str) -> bool {
    favorites.iter().any(|record| record.id == id)
}

pub fn toggle_favorite(favorites: &[SourceRef], reference: &SourceRef) -> Vec<SourceRef> {
    toggle(favorites, reference)
}

pub fn toggle_hidden(hidden: &[SourceRef], reference: &SourceRef) -> Vec<SourceRef> {
    toggle(hidden, reference)
}

fn toggle(records: &[SourceRef], reference: &SourceRef) -> Vec<SourceRef> {
    if records.iter().any(|record| record.id == reference.id) {
        records
            .iter()
            .filter(|record| record.id != reference.id)
            .cloned()
            .collect()
    } else {
        let mut next = records.to_vec();
        next.push(source_ref(&reference.id, &reference.name));
        next
    }
}

/// Name for a selection that may no longer be listed; the favorite record is consulted first.
pub fn record_name(favorites: &[SourceRef], hidden: &[SourceRef], id: &str) -> Option<String> {
    favorites
        .iter()
        .find(|record| record.id == id)
        .or_else(|| hidden.iter().find(|record| record.id == id))
        .map(|record| record.name.clone())
}

fn records_valid(records: &[SourceRef]) -> bool {
    let mut ids = HashSet::new();
    records.iter().all(|record| {
        !record.id.is_empty()
            && record.id.len() <= MAX_SOURCE_ID_BYTES
            && !record.name.is_empty()
            && record.name.len() <= MAX_SOURCE_NAME_BYTES
            && ids.insert(record.id.as_str())
    })
}

/// Refusal text for the preference lists the patch would persist, or `None`. Shape problems are
/// reported with the size wording; a patch that strictly shrinks an already oversized file is
/// allowed through so removal can always relieve it. The core re-checks the real bytes.
pub fn preference_error(settings: &Settings, patch: &PreferencePatch) -> Option<&'static str> {
    let favorites = patch
        .favorite_sources
        .as_ref()
        .unwrap_or(&settings.favorite_sources);
    let hidden = patch
        .hidden_sources
        .as_ref()
        .unwrap_or(&settings.hidden_sources);
    if favorites.len() > MAX_FAVORITES {
        return Some(FAVORITES_FULL);
    }
    if hidden.len() > MAX_HIDDEN {
        return Some(HIDDEN_FULL);
    }
    if !records_valid(favorites) || !records_valid(hidden) {
        return Some(TOO_LARGE);
    }
    let mut next = settings.clone();
    next.favorite_sources = favorites.clone();
    next.hidden_sources = hidden.clone();
    let estimate = serialized_estimate(&next);
    if estimate <= MAX_SERIALIZED_BYTES || estimate < serialized_estimate(settings) {
        return None;
    }
    Some(TOO_LARGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(id: &str, name: &str, active: bool) -> AudioSource {
        AudioSource {
            id: id.into(),
            name: name.into(),
            active,
            detail: None,
        }
    }
    fn chrome() -> AudioSource {
        audio("app:/Chrome.app", "Chrome", true)
    }
    const GHOST: &str = "app:/Closed.app";
    fn refs(items: &[(&str, &str)]) -> Vec<SourceRef> {
        items.iter().map(|(id, name)| source_ref(id, name)).collect()
    }
    fn many(count: usize) -> Vec<SourceRef> {
        (0..count)
            .map(|n| source_ref(&format!("app:/F{n}.app"), "F"))
            .collect()
    }
    fn favorites_patch(items: Vec<SourceRef>) -> PreferencePatch {
        PreferencePatch {
            favorite_sources: Some(items),
            hidden_sources: None,
        }
    }
    fn hidden_patch(items: Vec<SourceRef>) -> PreferencePatch {
        PreferencePatch {
            favorite_sources: None,
            hidden_sources: Some(items),
        }
    }
    fn quotes() -> Vec<SourceRef> {
        (0..31)
            .map(|n| source_ref(&format!("app:/{n}{}", "\"".repeat(500)), "A"))
            .collect()
    }

    #[test]
    fn favorites_lead_unavailable_favorites_are_placeholders_and_hidden_stay_listed() {
        let catalog = source_catalog(
            &[chrome()],
            &refs(&[(GHOST, "Closed"), ("app:/Chrome.app", "Chrome")]),
            &refs(&[("app:/Gone.app", "Gone")]),
        );
        let ids: Vec<_> = catalog.visible.iter().map(|view| view.id.as_str()).collect();
        assert_eq!(ids, [GHOST, "app:/Chrome.app"]);
        assert!(!catalog.visible[0].available && !catalog.visible[0].active && catalog.visible[0].favorite);
        assert_eq!(catalog.hidden.len(), 1);
        assert_eq!(catalog.hidden[0].id, "app:/Gone.app");
        assert!(!catalog.hidden[0].available && !catalog.hidden[0].favorite);
        assert_eq!(catalog.hidden[0].name, "Gone");
    }

    #[test]
    fn live_names_win_duplicates_collapse_and_hidden_favorites_keep_stars() {
        let renamed = audio("app:/Chrome.app", "Chrome Beta", true);
        let catalog = source_catalog(&[renamed], &refs(&[("app:/Chrome.app", "Old")]), &[]);
        assert_eq!(catalog.visible[0].name, "Chrome Beta");
        let catalog = source_catalog(
            &[chrome()],
            &refs(&[("app:/Chrome.app", "Old"), ("app:/Chrome.app", "Again")]),
            &refs(&[("app:/Chrome.app", "Old")]),
        );
        assert_eq!(catalog.all.len(), 1);
        assert!(catalog.visible.is_empty());
        assert!(catalog.hidden[0].favorite);
    }

    #[test]
    fn toggles_append_in_order_keep_the_list_untouched_and_report_membership() {
        let favorites = refs(&[(GHOST, "Closed")]);
        let chrome_ref = source_ref("app:/Chrome.app", "Chrome");
        assert_eq!(
            toggle_favorite(&favorites, &chrome_ref),
            refs(&[(GHOST, "Closed"), ("app:/Chrome.app", "Chrome")])
        );
        assert_eq!(favorites, refs(&[(GHOST, "Closed")]));
        assert_eq!(toggle_favorite(&toggle_favorite(&[], &chrome_ref), &chrome_ref), []);
        assert_eq!(
            toggle_hidden(&[], &source_ref("app:/Gone.app", "Gone")),
            refs(&[("app:/Gone.app", "Gone")])
        );
        assert_eq!(toggle_hidden(&refs(&[(GHOST, "Closed")]), &source_ref(GHOST, "Closed")), []);
        assert!(is_favorite(&favorites, GHOST));
        assert!(!is_favorite(&favorites, "app:/Chrome.app"));
        assert_eq!(
            record_name(&[], &refs(&[("app:/Gone.app", "Gone")]), "app:/Gone.app").as_deref(),
            Some("Gone")
        );
        assert_eq!(record_name(&[], &[], "app:/Gone.app"), None);
        assert_eq!(
            record_name(&refs(&[("app:/Chrome.app", "Chrome")]), &[], "app:/Chrome.app").as_deref(),
            Some("Chrome")
        );
    }

    #[test]
    fn preflight_refuses_counts_byte_length_duplicates_and_escaped_size() {
        let settings = Settings::default();
        let mut too_many = many(32);
        too_many.push(source_ref("app:/X.app", "X"));
        assert_eq!(
            preference_error(&settings, &favorites_patch(too_many)),
            Some("Можна додати не більше 32 програм до обраних.")
        );
        let mut too_hidden = many(32);
        too_hidden.push(source_ref("app:/A.app", "A"));
        assert_eq!(
            preference_error(&settings, &hidden_patch(too_hidden)),
            Some("Можна приховати не більше 32 програм.")
        );
        let long = vec![source_ref(&format!("app:/{}", "x".repeat(509)), "A")];
        assert_eq!(preference_error(&settings, &favorites_patch(long)), Some(TOO_LARGE));
        let duplicates = refs(&[("app:/A.app", "A"), ("app:/A.app", "A")]);
        assert_eq!(preference_error(&settings, &favorites_patch(duplicates)), Some(TOO_LARGE));
        let mut heavy = Settings::default();
        heavy.api_key = "k".repeat(4096);
        let mut with_quotes = heavy.clone();
        with_quotes.favorite_sources = quotes();
        assert!(serialized_estimate(&with_quotes) > MAX_SERIALIZED_BYTES);
        assert_eq!(preference_error(&heavy, &favorites_patch(quotes())), Some(TOO_LARGE));
        assert_eq!(
            preference_error(&settings, &favorites_patch(refs(&[("app:/A.app", "A")]))),
            None
        );
    }

    #[test]
    fn preflight_counts_utf8_bytes_not_characters_and_accepts_the_exact_caps() {
        let settings = Settings::default();
        let named = |name: String| favorites_patch(vec![source_ref("app:/A.app", &name)]);
        assert_eq!(preference_error(&settings, &named("і".repeat(65))), Some(TOO_LARGE));
        assert_eq!(preference_error(&settings, &named("і".repeat(64))), None);
        assert_eq!(MAX_SOURCE_NAME_BYTES, 128);
        let longest = format!("app:/{}", "x".repeat(MAX_SOURCE_ID_BYTES - 5));
        assert_eq!(
            preference_error(&settings, &favorites_patch(vec![source_ref(&longest, "A")])),
            None
        );
        assert_eq!(
            preference_error(&settings, &favorites_patch(vec![source_ref("", "A")])),
            Some(TOO_LARGE)
        );
        assert_eq!(
            preference_error(&settings, &hidden_patch(vec![source_ref("app:/A.app", "")])),
            Some(TOO_LARGE)
        );
        assert_eq!(preference_error(&settings, &favorites_patch(many(32))), None);
    }

    #[test]
    fn an_oversized_existing_file_can_still_shrink_so_preferences_stay_removable() {
        let mut oversized = Settings::default();
        oversized.api_key = "k".repeat(4096);
        oversized.favorite_sources = quotes();
        assert_eq!(
            preference_error(&oversized, &favorites_patch(quotes()[1..].to_vec())),
            None
        );
        assert_eq!(preference_error(&oversized, &favorites_patch(Vec::new())), None);
        let mut shrunk = oversized.clone();
        shrunk.favorite_sources = quotes()[1..].to_vec();
        assert_eq!(preference_error(&shrunk, &favorites_patch(quotes())), Some(TOO_LARGE));
    }
}
