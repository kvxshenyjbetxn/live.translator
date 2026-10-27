use serde::Deserialize;
use std::sync::Mutex;
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};
use translator_core::{
    session::{diagnostics::DiagnosticIssue, SessionPhase, SessionStatus},
    settings::LANGUAGES,
};

fn localized(text: &str, language: &str) -> String {
    static CATALOG: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();
    if language != "en" {
        return text.into();
    }
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../src/lib/messages.json"))
                .expect("valid translation catalog")
        })
        .get(text)
        .map(String::as_str)
        .unwrap_or(text)
        .into()
}

#[cfg(test)]
mod label_tests {
    use super::*;
    #[test]
    fn waiting_label_precedes_running_and_idle_labels() {
        let mut model = Model {
            waiting: true,
            can_start: false,
            can_stop: true,
            ..Model::default()
        };
        assert_eq!(toggle_label(&model), "Скасувати очікування");
        model.waiting = false;
        assert_eq!(toggle_label(&model), "Зупинити переклад");
        model.can_stop = false;
        assert_eq!(toggle_label(&model), "Почати переклад");
        let legacy: Model = serde_json::from_str(r#"{"revision":1,"sources":[],"selectedSourceId":null,"targetLanguage":"uk","canSelect":true,"canStart":true,"canStop":false}"#).unwrap();
        assert!(!legacy.waiting);
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    #[test]
    fn tray_labels_and_cached_health_follow_interface_language() {
        assert_eq!(
            localized("Готовий до перекладу", "en"),
            "Ready to translate"
        );
        assert_eq!(
            localized("Готовий до перекладу", "uk"),
            "Готовий до перекладу"
        );
        assert_eq!(localized("Мова перекладу", "en"), "Translation language");
        assert_eq!(localized("Завантажуємо…", "en"), "Loading…");
        assert_eq!(localized("Chrome", "en"), "Chrome");
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Gray,
    Yellow,
    Green,
    Red,
}
pub fn health(status: &SessionStatus) -> (Color, &'static str) {
    match status.phase {
        SessionPhase::Idle => (Color::Gray, "Готовий до перекладу"),
        SessionPhase::Starting => (Color::Yellow, "Підключаємося до Google…"),
        SessionPhase::Stopping => (Color::Yellow, "Зупиняємо захоплення…"),
        SessionPhase::Error => (Color::Red, "Помилка · переклад зупинено"),
        SessionPhase::Running => {
            let d = &status.diagnostics;
            if d.network_rtt_ms.is_none() {
                (
                    Color::Yellow,
                    "Захоплення активне · якість зв’язку ще невідома",
                )
            } else if d.issues.contains(&DiagnosticIssue::NetworkUnstable) {
                (Color::Yellow, "Захоплення активне · нестабільний зв’язок")
            } else if d.issues.contains(&DiagnosticIssue::PlaybackQueue)
                || d.issues.contains(&DiagnosticIssue::SendingQueue)
            {
                (
                    Color::Green,
                    "Захоплення активне · зв’язок стабільний · черга аудіо",
                )
            } else if status.received_audio_seconds > 0. {
                (Color::Green, "Переклад активний · зв’язок стабільний")
            } else {
                (
                    Color::Green,
                    "Захоплення активне · очікуємо переклад · зв’язок стабільний",
                )
            }
        }
    }
}
fn icon(color: Color) -> Image<'static> {
    let dot = match color {
        Color::Gray => [145, 151, 160, 255],
        Color::Yellow => [244, 183, 45, 255],
        Color::Green => [42, 193, 111, 255],
        Color::Red => [237, 76, 86, 255],
    };
    let mut pixels = vec![0; 24 * 24 * 4];
    for y in 0..24_i32 {
        for x in 0..24_i32 {
            // Light outline/dark core remain visible on both light and dark menu bars.
            let distance = (x - 11).pow(2) + (y - 11).pow(2);
            let pixel = if distance <= 81 {
                Some(dot)
            } else if distance <= 100 {
                Some([245, 245, 245, 255])
            } else {
                None
            };
            if let Some(pixel) = pixel {
                let i = ((y * 24 + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&pixel);
            }
        }
    }
    Image::new_owned(pixels, 24, 24)
}
#[derive(Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub revision: u64,
    pub sources: Vec<Source>,
    pub selected_source_id: Option<String>,
    pub target_language: String,
    #[serde(default)]
    pub interface_language: String,
    pub can_select: bool,
    pub can_start: bool,
    pub can_stop: bool,
    /// An explicit start is waiting for a favorite application to appear. Kept last so older
    /// models stay readable; it is renderer state only and is never persisted.
    #[serde(default)]
    pub waiting: bool,
}
/// Tray toggle label: an explicit wait must be cancellable before it is a running stop.
pub fn toggle_label(model: &Model) -> &'static str {
    if model.waiting {
        "Скасувати очікування"
    } else if model.can_stop {
        "Зупинити переклад"
    } else {
        "Почати переклад"
    }
}
#[derive(Clone, Deserialize, PartialEq)]
pub struct Source {
    id: String,
    name: String,
}
struct Native {
    model: Model,
    status: MenuItem<tauri::Wry>,
    last_health: (Color, &'static str),
}
#[derive(Default)]
pub struct Control(Mutex<Option<Native>>);
fn menu(
    app: &AppHandle,
    model: &Model,
    status: &MenuItem<tauri::Wry>,
) -> tauri::Result<Menu<tauri::Wry>> {
    let tr = |text: &str| localized(text, &model.interface_language);
    let menu = Menu::new(app)?;
    menu.append(status)?;
    let languages = Submenu::new(app, tr("Мова перекладу"), model.can_select)?;
    for language in LANGUAGES {
        languages.append(&CheckMenuItem::with_id(
            app,
            format!("language:{}", language.code),
            tr(language.name),
            model.can_select,
            language.code == model.target_language,
            None::<&str>,
        )?)?;
    }
    menu.append(&languages)?;
    let sources = Submenu::new(app, tr("Програма"), model.can_select)?;
    if model.sources.is_empty() {
        sources.append(&MenuItem::new(
            app,
            tr("Немає доступних програм"),
            false,
            None::<&str>,
        )?)?;
    }
    for source in &model.sources {
        sources.append(&CheckMenuItem::with_id(
            app,
            format!("source:{}", source.id),
            &source.name,
            model.can_select,
            model.selected_source_id.as_ref() == Some(&source.id),
            None::<&str>,
        )?)?;
    }
    sources.append(&MenuItem::with_id(
        app,
        "refresh",
        tr("Оновити програми"),
        model.can_select,
        None::<&str>,
    )?)?;
    menu.append(&sources)?;
    menu.append(&MenuItem::with_id(
        app,
        "toggle",
        tr(toggle_label(model)),
        model.can_start || model.can_stop,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        tr("Відкрити вікно"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "exit",
        tr("Вийти"),
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}
pub fn initialize(app: &AppHandle) -> tauri::Result<()> {
    let store = app.state::<crate::ConfigStore>();
    let language = translator_core::settings::load_settings(&store.directory)
        .unwrap_or_default()
        .interface_language;
    let model = Model {
        interface_language: language,
        ..Model::default()
    };
    let loading = localized("Завантажуємо…", &model.interface_language);
    let status = MenuItem::new(app, &loading, false, None::<&str>)?;
    TrayIconBuilder::with_id("translator")
        .icon(icon(Color::Gray))
        .icon_as_template(false)
        .tooltip(format!("Live Translator · {loading}"))
        .menu(&menu(app, &model, &status)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            match id {
                "show" => super::show(app),
                "exit" => crate::lifecycle::request(app),
                "toggle" | "refresh" => super::emit(app, super::Action::new(id, None)),
                _ => {
                    if let Some(value) = id.strip_prefix("language:") {
                        super::emit(app, super::Action::new("language", Some(value.into())));
                    }
                    if let Some(value) = id.strip_prefix("source:") {
                        super::emit(app, super::Action::new("source", Some(value.into())));
                    }
                }
            }
        })
        .build(app)?;
    *app.state::<Control>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(Native {
        model,
        status,
        last_health: (Color::Gray, "Завантажуємо…"),
    });
    Ok(())
}
#[tauri::command]
pub fn sync_desktop(
    app: AppHandle,
    control: State<'_, Control>,
    model: Model,
) -> Result<(), String> {
    let mut state = control.0.lock().unwrap_or_else(|e| e.into_inner());
    let Some(native) = state.as_mut() else {
        return Ok(());
    };
    if native.model != model {
        let menu =
            menu(&app, &model, &native.status).map_err(|_| "Не вдалося оновити меню трея.")?;
        if let Some(tray) = app.tray_by_id("translator") {
            tray.set_menu(Some(menu))
                .map_err(|_| "Не вдалося оновити меню трея.")?;
        }
        let text = localized(native.last_health.1, &model.interface_language);
        native
            .status
            .set_text(&text)
            .map_err(|_| "Не вдалося оновити меню трея.")?;
        if let Some(tray) = app.tray_by_id("translator") {
            let _ = tray.set_tooltip(Some(format!("Live Translator · {text}")));
        }
        native.model = model;
    }
    Ok(())
}
pub fn update(app: &AppHandle, status: &SessionStatus) {
    let control = app.state::<Control>();
    let mut state = control.0.lock().unwrap_or_else(|e| e.into_inner());
    let Some(native) = state.as_mut() else {
        return;
    };
    let next = health(status);
    if native.last_health == next {
        return;
    }
    if let Some(tray) = app.tray_by_id("translator") {
        let _ = tray.set_icon(Some(icon(next.0)));
        let text = localized(next.1, &native.model.interface_language);
        let _ = tray.set_tooltip(Some(format!("Live Translator · {text}")));
        let _ = native.status.set_text(text);
    }
    native.last_health = next;
}
