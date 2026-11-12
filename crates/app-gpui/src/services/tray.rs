//! System tray icon and menu (port of `src-tauri/src/desktop/tray.rs` onto `tray-icon`/`muda`).
//! Events are forwarded into an async channel so a GPUI foreground task handles them.

use crate::logic::{
    desktop::{ActionKind, DesktopAction, MenuModel},
    i18n::{t, Locale},
};
use futures::channel::mpsc::UnboundedSender;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, Submenu},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use translator_core::{
    session::{diagnostics::DiagnosticIssue, SessionPhase, SessionStatus},
    settings::LANGUAGES,
};

#[derive(Debug)]
pub enum TrayCommand {
    Show,
    Exit,
    Action(DesktopAction),
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

fn icon(color: Color) -> Result<Icon, String> {
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
    Icon::from_rgba(pixels, 24, 24).map_err(|e| e.to_string())
}

/// Tray toggle label: an explicit wait must be cancellable before it is a running stop.
pub fn toggle_label(model: &MenuModel) -> &'static str {
    if model.waiting {
        "Скасувати очікування"
    } else if model.can_stop {
        "Зупинити переклад"
    } else {
        "Почати переклад"
    }
}

const MENU_FAILED: &str = "Не вдалося оновити меню трея.";

pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    model: MenuModel,
    last_health: (Color, &'static str),
}

fn build_menu(model: &MenuModel, status: &MenuItem) -> Result<Menu, tray_icon::menu::Error> {
    let locale = Locale::from_code(&model.interface_language);
    let tr = |text: &str| t(text, locale);
    let menu = Menu::new();
    menu.append(status)?;
    let languages = Submenu::new(tr("Мова перекладу"), model.can_select);
    for language in LANGUAGES {
        languages.append(&CheckMenuItem::with_id(
            format!("language:{}", language.code),
            tr(language.name),
            model.can_select,
            language.code == model.target_language,
            None,
        ))?;
    }
    menu.append(&languages)?;
    let sources = Submenu::new(tr("Програма"), model.can_select);
    if model.sources.is_empty() {
        sources.append(&MenuItem::new(tr("Немає доступних програм"), false, None))?;
    }
    for source in &model.sources {
        sources.append(&CheckMenuItem::with_id(
            format!("source:{}", source.id),
            &source.name,
            model.can_select,
            model.selected_source_id.as_ref() == Some(&source.id),
            None,
        ))?;
    }
    sources.append(&MenuItem::with_id(
        "refresh",
        tr("Оновити програми"),
        model.can_select,
        None,
    ))?;
    menu.append(&sources)?;
    menu.append(&MenuItem::with_id(
        "toggle",
        tr(toggle_label(model)),
        model.can_start || model.can_stop,
        None,
    ))?;
    menu.append(&MenuItem::with_id("show", tr("Відкрити вікно"), true, None))?;
    menu.append(&MenuItem::with_id("exit", tr("Вийти"), true, None))?;
    Ok(menu)
}

fn tooltip(text: &str) -> String {
    format!("Live Translator · {text}")
}

impl Tray {
    /// Must run on the UI thread after the platform loop exists. The result must be kept alive.
    pub fn initialize(
        interface_language: &str,
        commands: UnboundedSender<TrayCommand>,
    ) -> Result<Self, String> {
        let model = MenuModel {
            interface_language: interface_language.into(),
            ..MenuModel::default()
        };
        let locale = Locale::from_code(interface_language);
        let loading = t("Завантажуємо…", locale);
        let status = MenuItem::new(&loading, false, None);
        let menu = build_menu(&model, &status).map_err(|e| e.to_string())?;

        let menu_commands = commands.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let id = event.id.as_ref();
            let command = match id {
                "show" => Some(TrayCommand::Show),
                "exit" => Some(TrayCommand::Exit),
                "toggle" => Some(TrayCommand::Action(DesktopAction::new(ActionKind::Toggle, None))),
                "refresh" => Some(TrayCommand::Action(DesktopAction::new(ActionKind::Refresh, None))),
                _ => id
                    .strip_prefix("language:")
                    .map(|value| (ActionKind::Language, value))
                    .or_else(|| id.strip_prefix("source:").map(|value| (ActionKind::Source, value)))
                    .map(|(kind, value)| {
                        TrayCommand::Action(DesktopAction::new(kind, Some(value.into())))
                    }),
            };
            if let Some(command) = command {
                let _ = menu_commands.unbounded_send(command);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                let _ = commands.unbounded_send(TrayCommand::Show);
            }
        }));

        let tray = TrayIconBuilder::new()
            .with_id("translator")
            .with_icon(icon(Color::Gray)?)
            .with_tooltip(tooltip(&loading))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            icon: tray,
            status,
            model,
            last_health: (Color::Gray, "Завантажуємо…"),
        })
    }

    /// Applies a semantic menu model: rebuilds the menu and relocalizes the status line.
    pub fn sync(&mut self, model: &MenuModel) -> Result<(), String> {
        if &self.model == model {
            return Ok(());
        }
        let menu = build_menu(model, &self.status).map_err(|_| MENU_FAILED.to_string())?;
        self.icon.set_menu(Some(Box::new(menu)));
        let text = t(self.last_health.1, Locale::from_code(&model.interface_language));
        self.status.set_text(&text);
        let _ = self.icon.set_tooltip(Some(tooltip(&text)));
        self.model = model.clone();
        Ok(())
    }

    /// Reflects the session status in the icon colour, tooltip and status line.
    pub fn update(&mut self, status: &SessionStatus) {
        let next = health(status);
        if self.last_health == next {
            return;
        }
        if let Ok(image) = icon(next.0) {
            let _ = self.icon.set_icon(Some(image));
        }
        let text = t(next.1, Locale::from_code(&self.model.interface_language));
        let _ = self.icon.set_tooltip(Some(tooltip(&text)));
        self.status.set_text(&text);
        self.last_health = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_label_precedes_running_and_idle_labels() {
        let mut model = MenuModel {
            waiting: true,
            can_start: false,
            can_stop: true,
            ..MenuModel::default()
        };
        assert_eq!(toggle_label(&model), "Скасувати очікування");
        model.waiting = false;
        assert_eq!(toggle_label(&model), "Зупинити переклад");
        model.can_stop = false;
        assert_eq!(toggle_label(&model), "Почати переклад");
    }

    #[test]
    fn tray_labels_and_cached_health_follow_interface_language() {
        assert_eq!(t("Готовий до перекладу", Locale::En), "Ready to translate");
        assert_eq!(t("Готовий до перекладу", Locale::Uk), "Готовий до перекладу");
        assert_eq!(t("Мова перекладу", Locale::En), "Translation language");
        assert_eq!(t("Завантажуємо…", Locale::En), "Loading…");
        assert_eq!(t("Chrome", Locale::En), "Chrome");
    }

    #[test]
    fn health_never_invents_good_network_or_confuses_local_queue_with_network() {
        let mut s = SessionStatus::default();
        assert_eq!(health(&s).0, Color::Gray);
        s.phase = SessionPhase::Starting;
        assert_eq!(health(&s).0, Color::Yellow);
        s.phase = SessionPhase::Running;
        assert_eq!(health(&s).0, Color::Yellow);
        s.diagnostics.network_rtt_ms = Some(80.);
        assert_eq!(health(&s).0, Color::Green);
        s.diagnostics.issues.push(DiagnosticIssue::PlaybackQueue);
        assert_eq!(health(&s).0, Color::Green);
        s.diagnostics.issues.push(DiagnosticIssue::NetworkUnstable);
        assert_eq!(health(&s).0, Color::Yellow);
        s.phase = SessionPhase::Error;
        assert_eq!(health(&s).0, Color::Red);
    }

    #[test]
    fn every_tray_label_exists_in_the_catalog() {
        let model = MenuModel::default();
        for label in [
            toggle_label(&model),
            "Скасувати очікування",
            "Мова перекладу",
            "Програма",
            "Немає доступних програм",
            "Оновити програми",
            "Відкрити вікно",
            "Вийти",
        ] {
            assert_ne!(t(label, Locale::En), label, "{label}");
        }
    }
}
