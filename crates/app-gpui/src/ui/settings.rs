//! The Settings page (port of `Settings.svelte`, `ShortcutSettings.svelte` and
//! `AppearanceSettings.svelte`).

use super::{
    theme::{self, Palette},
    widgets::{self, SEMI_BOLD},
};
use crate::{
    logic::{
        appearance::{normalize_accent, DEFAULT_ACCENT},
        autosave::SaveState,
        i18n::{t, translate, Locale},
        patch::Patch,
        shortcut::{code_from_key, shortcut_from_key, Modifiers},
    },
    state::{AppState, Tab},
};
use gpui::{
    deferred, div, prelude::*, px, relative, rgb, ClickEvent, Context, Div, Entity,
    FocusHandle, Hsla, IntoElement, KeyDownEvent, KeyUpEvent, ParentElement, Render, Rgba,
    Styled, Window,
};
use gpui_component::{
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState},
};
use translator_core::settings::ShortcutMode;

#[cfg(target_os = "macos")]
const MONO: &str = "Menlo";
#[cfg(not(target_os = "macos"))]
const MONO: &str = "Consolas";

const API_KEY_LIMIT: usize = 4096;
const HEX_LIMIT: usize = 7;
const INPUT_HEIGHT: f32 = 41.0;
const KEY_PLACEHOLDER: &str = "Вставте ключ із Google AI Studio";

pub struct SettingsPage {
    state: Entity<AppState>,
    api_key: Entity<InputState>,
    accent_hex: Entity<InputState>,
    accent_picker: Entity<ColorPickerState>,
    revealed: bool,
    recording: bool,
    message: String,
    /// The key held during recording: its stored code and the finished shortcut string.
    candidate: Option<(String, String)>,
    recorder: FocusHandle,
    open_select: Option<&'static str>,
    synced_accent: String,
    locale: Locale,
}

fn to_hsla(hex: &str) -> Hsla {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    rgb(value).into()
}

fn to_hex(color: Hsla) -> String {
    let color: Rgba = color.into();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", channel(color.r), channel(color.g), channel(color.b))
}

fn limited(value: &str, limit: usize) -> Option<String> {
    (value.chars().count() > limit).then(|| value.chars().take(limit).collect())
}

impl SettingsPage {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (key, accent, locale) = {
            let s = state.read(cx);
            (s.settings.api_key.clone(), s.settings.accent_color.clone(), s.locale())
        };
        let api_key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(t(KEY_PLACEHOLDER, locale))
                .default_value(key)
        });
        let accent_hex = cx.new(|cx| InputState::new(window, cx).default_value(accent.clone()));
        let accent_picker = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(to_hsla(
                &normalize_accent(&accent).unwrap_or_else(|| DEFAULT_ACCENT.into()),
            ))
        });

        cx.subscribe_in(&api_key, window, |this, input, event: &InputEvent, window, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let mut value = input.read(cx).value().to_string();
            if let Some(cut) = limited(&value, API_KEY_LIMIT) {
                input.update(cx, |input, cx| input.set_value(cut.clone(), window, cx));
                value = cut;
            }
            this.state.update(cx, |s, cx| {
                s.change(
                    Patch {
                        api_key: Some(value),
                        ..Patch::default()
                    },
                    cx,
                )
            });
        })
        .detach();

        cx.subscribe_in(&accent_hex, window, |this, input, event: &InputEvent, window, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let mut value = input.read(cx).value().to_string();
            if let Some(cut) = limited(&value, HEX_LIMIT) {
                input.update(cx, |input, cx| input.set_value(cut.clone(), window, cx));
                value = cut;
            }
            if let Some(color) = normalize_accent(&value) {
                this.state.update(cx, |s, cx| {
                    s.change(
                        Patch {
                            accent_color: Some(color),
                            ..Patch::default()
                        },
                        cx,
                    )
                });
            }
            cx.notify();
        })
        .detach();

        cx.subscribe(&accent_picker, |this, _, event: &ColorPickerEvent, cx| {
            let ColorPickerEvent::Change(Some(color)) = event else {
                return;
            };
            let hex = to_hex(*color);
            if this.state.read(cx).settings.accent_color == hex {
                return;
            }
            this.state.update(cx, |s, cx| {
                s.change(
                    Patch {
                        accent_color: Some(hex),
                        ..Patch::default()
                    },
                    cx,
                )
            });
        })
        .detach();

        // Settings change from outside the inputs too (load, delete-key, reset).
        cx.observe_in(&state, window, |this, state, window, cx| {
            let (key, accent, tab, locale) = {
                let s = state.read(cx);
                (
                    s.settings.api_key.clone(),
                    s.settings.accent_color.clone(),
                    s.tab,
                    s.locale(),
                )
            };
            if this.api_key.read(cx).value().as_ref() != key.as_str() {
                this.api_key.update(cx, |input, cx| input.set_value(key, window, cx));
            }
            if accent != this.synced_accent {
                this.synced_accent = accent.clone();
                this.accent_hex
                    .update(cx, |input, cx| input.set_value(accent.clone(), window, cx));
                if let Some(color) = normalize_accent(&accent) {
                    this.accent_picker
                        .update(cx, |picker, cx| picker.set_value(to_hsla(&color), window, cx));
                }
            }
            if locale != this.locale {
                this.locale = locale;
                this.api_key.update(cx, |input, cx| {
                    input.set_placeholder(t(KEY_PLACEHOLDER, locale), window, cx)
                });
            }
            // Leaving the page cancels a running recording (`onDestroy` in the Svelte view).
            if tab != Tab::Settings && this.recording {
                this.finish(None, cx);
            }
            cx.notify();
        })
        .detach();

        Self {
            state,
            api_key,
            accent_hex,
            accent_picker,
            revealed: false,
            recording: false,
            message: String::new(),
            candidate: None,
            recorder: cx.focus_handle(),
            open_select: None,
            synced_accent: accent,
            locale,
        }
    }

    // ---- shortcut recording ------------------------------------------------------------------

    fn begin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording {
            return;
        }
        self.message.clear();
        self.candidate = None;
        let result = self.state.update(cx, |s, cx| s.record_shortcut(true, cx));
        match result {
            Ok(()) => {
                self.recording = true;
                self.recorder.focus(window, cx);
            }
            Err(reason) => self.message = reason,
        }
        cx.notify();
    }

    fn finish(&mut self, value: Option<String>, cx: &mut Context<Self>) {
        self.recording = false;
        self.candidate = None;
        let result = self.state.update(cx, |s, cx| s.record_shortcut(false, cx));
        match result {
            Ok(()) => {
                if let Some(value) = value {
                    self.state.update(cx, |s, cx| {
                        s.change(
                            Patch {
                                capture_shortcut: Some(Some(value)),
                                ..Patch::default()
                            },
                            cx,
                        )
                    });
                }
            }
            Err(_) => self.message = "Не вдалося завершити запис. Спробуйте ще раз.".into(),
        }
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }
        cx.stop_propagation();
        if event.is_held {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.finish(None, cx);
            return;
        }
        let modifiers = &event.keystroke.modifiers;
        let modifiers = Modifiers {
            ctrl: modifiers.control,
            meta: modifiers.platform,
            alt: modifiers.alt,
            shift: modifiers.shift,
        };
        let found = code_from_key(key)
            .and_then(|code| shortcut_from_key(&code, &modifiers, false).map(|value| (code, value)));
        match found {
            Some(candidate) => {
                self.candidate = Some(candidate);
                self.message = "Відпустіть клавішу, щоб завершити запис.".into();
            }
            None => {
                self.message =
                    "Натисніть модифікатор разом із клавішею (наприклад Ctrl + Shift + T).".into();
            }
        }
        cx.notify();
    }

    fn key_up(&mut self, event: &KeyUpEvent, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }
        cx.stop_propagation();
        let released = code_from_key(&event.keystroke.key);
        if let (Some((code, value)), Some(released)) = (&self.candidate, released) {
            if *code == released {
                let value = value.clone();
                self.message.clear();
                self.finish(Some(value), cx);
            }
        }
    }

    // ---- small pieces ------------------------------------------------------------------------

    fn h2(&self, text: String) -> Div {
        div()
            .text_size(px(13.0))
            .font_weight(SEMI_BOLD)
            .mb(px(12.0))
            .child(text)
    }

    fn muted(&self, text: String, p: &Palette) -> Div {
        div()
            .text_size(px(11.0))
            .text_color(p.muted)
            .line_height(relative(1.65))
            .mb(px(17.0))
            .child(text)
    }

    fn field_label(&self, text: String, p: &Palette) -> Div {
        div().text_size(px(11.0)).text_color(p.muted).mt(px(22.0)).mb(px(8.0)).child(text)
    }

    fn subtle(&self, text: String, p: &Palette) -> Div {
        div()
            .text_size(px(10.0))
            .text_color(p.muted)
            .line_height(relative(1.8))
            .mb(px(10.0))
            .child(text)
    }

    fn card(&self, p: &Palette) -> Div {
        div()
            .bg(p.surface)
            .border_1()
            .border_color(p.line)
            .rounded(px(10.0))
            .p(px(21.0))
            .mb(px(18.0))
    }

    fn text_input(&self, state: &Entity<InputState>, disabled: bool, p: &Palette) -> Input {
        Input::new(state)
            .disabled(disabled)
            .bg(p.input)
            .border_color(p.line)
            .rounded(px(8.0))
            .px(px(12.0))
            .text_size(px(13.0))
    }

    /// A `<select>`: the trigger plus an anchored list of options, each applying a patch.
    fn dropdown(
        &self,
        id: &'static str,
        current: String,
        options: Vec<(String, bool, Patch)>,
        disabled: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.open_select == Some(id) && !disabled;
        let hover = p.hover;
        let items = options.into_iter().enumerate().map(|(index, (label, chosen, patch))| {
            div()
                .id((id, index))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(13.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .when(chosen, |this| this.bg(p.selected))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_select = None;
                    let patch = patch.clone();
                    this.state.update(cx, |s, cx| s.change(patch, cx));
                    cx.notify();
                }))
                .child(label)
        });
        div()
            .relative()
            .child(
                div()
                    .id((id, 99usize))
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .px(px(12.0))
                    .py(px(11.0))
                    .bg(p.input)
                    .border_1()
                    .border_color(p.line)
                    .rounded(px(8.0))
                    .text_size(px(13.0))
                    .when(disabled, |this| this.opacity(0.5).cursor_not_allowed())
                    .when(!disabled, |this| {
                        this.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                            this.open_select =
                                if this.open_select == Some(id) { None } else { Some(id) };
                            cx.notify();
                        }))
                    })
                    .child(current)
                    .child(div().text_size(px(10.0)).text_color(p.muted).child("▾")),
            )
            .when(open, |this| {
                this.child(deferred(
                    div()
                        .id((id, 100usize))
                        .absolute()
                        .top(px(44.0))
                        .left_0()
                        .w_full()
                        .max_h(px(280.0))
                        .overflow_y_scroll()
                        .occlude()
                        .bg(p.input)
                        .border_1()
                        .border_color(p.line)
                        .rounded(px(8.0))
                        .py(px(4.0))
                        .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                            if this.open_select == Some(id) {
                                this.open_select = None;
                                cx.notify();
                            }
                        }))
                        .children(items),
                ))
            })
    }

    // ---- cards -------------------------------------------------------------------------------

    fn connection_card(
        &mut self,
        disabled: bool,
        p: &Palette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> Div {
        let s = self.state.read(cx);
        let checking = s.checking;
        let blocked = s.busy() || s.starting || s.waiting.is_some();
        let key = s.settings.api_key.clone();
        let message = s.connection_message.clone();
        let message_error = s.connection_error;
        let revealed = self.revealed;
        let key_empty = key.trim().is_empty();
        let tr = |text: &str| t(text, locale);

        let check_disabled = disabled || checking || blocked || key_empty;
        let delete_disabled = disabled || checking || key.is_empty();
        let hover = p.hover;
        let text = p.text;

        self.card(p)
            .child(self.h2(tr("Підключення до Google")))
            .child(self.muted(
                tr("Офіційний Gemini Live Translation API. Ваш ключ, ваші квоти."),
                p,
            ))
            .child(self.field_label(tr("Gemini API-ключ"), p).mt(px(22.0)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_stretch()
                    .gap(px(7.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(120.0))
                            .h(px(INPUT_HEIGHT))
                            .child(self.text_input(&self.api_key, disabled || checking, p)),
                    )
                    .child(
                        widgets::button("reveal-key", p, disabled)
                            .text_size(px(10.0))
                            .px(px(9.0))
                            .py(px(8.0))
                            .when(!disabled, |this| {
                                this.on_click(cx.listener(|this, _, window, cx| {
                                    this.revealed = !this.revealed;
                                    let masked = !this.revealed;
                                    this.api_key.update(cx, |input, cx| {
                                        input.set_masked(masked, window, cx)
                                    });
                                    cx.notify();
                                }))
                            })
                            .child(tr(if revealed { "Приховати" } else { "Показати" })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .mt(px(13.0))
                    .mb(px(10.0))
                    .child(
                        widgets::button("check-connection", p, check_disabled)
                            .text_size(px(11.0))
                            .when(!check_disabled, |this| {
                                this.on_click(cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |s, cx| s.check_connection(cx));
                                }))
                            })
                            .child(tr(if checking {
                                "Перевіряємо…"
                            } else {
                                "Перевірити з’єднання"
                            })),
                    )
                    .child(
                        div().text_size(px(10.0)).text_color(p.muted).line_height(relative(1.8)).child(
                            tr(if blocked {
                                "Спочатку зупиніть переклад"
                            } else {
                                "Коротка Live-сесія без передавання аудіо"
                            }),
                        ),
                    ),
            )
            .when(!message.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(11.0))
                        .line_height(relative(1.7))
                        .mb(px(11.0))
                        .text_color(if message_error { theme::error_accent() } else { p.focus })
                        .child(translate(&message, locale, &[])),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap(px(14.0))
                    .mt(px(9.0))
                    .mb(px(15.0))
                    .child(div().text_size(px(10.0)).text_color(p.muted).line_height(relative(1.8)).child(
                        tr(if key_empty {
                            "Ключ ще не додано"
                        } else {
                            "Ключ додано · доступ перевіряється під час запуску"
                        }),
                    ))
                    .child(
                        div()
                            .id("delete-key")
                            .py(px(4.0))
                            .text_size(px(10.0))
                            .text_color(p.muted)
                            .flex_shrink_0()
                            .when(delete_disabled, |this| this.opacity(0.42).cursor_not_allowed())
                            .when(!delete_disabled, |this| {
                                this.cursor_pointer()
                                    .hover(move |style| style.text_color(text).bg(hover))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.revealed = false;
                                        this.api_key.update(cx, |input, cx| {
                                            input.set_masked(true, window, cx)
                                        });
                                        this.state.update(cx, |s, cx| {
                                            s.change(
                                                Patch {
                                                    api_key: Some(String::new()),
                                                    ..Patch::default()
                                                },
                                                cx,
                                            )
                                        });
                                        cx.notify();
                                    }))
                            })
                            .child(tr("Видалити ключ")),
                    ),
            )
            .child(
                widgets::notice(p)
                    .text_size(px(10.0))
                    .mb(px(13.0))
                    .child(tr(
                        "Ключ зберігається відкритим текстом у локальному файлі. Не діліться цим файлом. Новий ключ застосовується з наступного запуску перекладу.",
                    )),
            )
            .child(self.subtle(
                tr("Безкоштовний рівень має ліміти. Програма не вмикає платний білінг; тариф залежить від вашого проєкту Google. На безкоштовному рівні Google може використовувати дані для покращення продуктів."),
                p,
            ))
    }

    fn shortcut_card(
        &mut self,
        disabled: bool,
        p: &Palette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> Div {
        let s = self.state.read(cx);
        let mode = s.settings.shortcut_mode;
        let shortcut = s.settings.capture_shortcut.clone();
        let status = s.shortcut_status.clone();
        let recording = self.recording;
        let tr = |text: &str| t(text, locale);
        let mode_label = tr(if mode == ShortcutMode::ActiveApplication {
            "Активна програма"
        } else {
            "Остання захоплена"
        });
        let options = [
            (ShortcutMode::ActiveApplication, "Активна програма"),
            (ShortcutMode::LastCaptured, "Остання захоплена"),
        ]
        .into_iter()
        .map(|(value, label)| {
            (
                tr(label),
                value == mode,
                Patch {
                    shortcut_mode: Some(value),
                    ..Patch::default()
                },
            )
        })
        .collect();

        let status_line = format!(
            "{} {}",
            match &status.active {
                Some(active) => translate(
                    "Активна комбінація: {shortcut}",
                    locale,
                    &[("shortcut", active.clone())]
                ),
                None => tr("Глобальна клавіша вимкнена."),
            },
            if recording {
                tr("Під час запису захоплення клавішею призупинене. Escape — скасувати.")
            } else {
                String::new()
            }
        );
        let problem = if self.message.is_empty() {
            status.error.clone().unwrap_or_default()
        } else {
            self.message.clone()
        };
        let record_disabled = disabled;
        let clear_disabled = disabled || recording || shortcut.is_none();

        self.card(p)
            .child(self.h2(tr("Глобальна гаряча клавіша")))
            .child(self.muted(
                tr("Одне натискання запускає захоплення, повторне — зупиняє, навіть після переходу в інше вікно."),
                p,
            ))
            .child(self.field_label(tr("Дія гарячої клавіші"), p))
            .child(
                self.dropdown("shortcut-mode", mode_label, options, disabled || recording, p, cx)
                    .mb(px(4.0)),
            )
            .child(self.subtle(
                tr("Активна програма — це програма переднього вікна в момент натискання, не окрема вкладка браузера. Live Translator не захоплює себе. На непідтримуваних системах оберіть останню захоплену програму."),
                p,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .text_size(px(10.0))
                    .text_color(p.muted)
                    .child(
                        widgets::button("record-shortcut", p, record_disabled)
                            .track_focus(&self.recorder)
                            .text_size(px(14.0))
                            .text_color(p.text)
                            .when(!record_disabled, |this| {
                                this.on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.begin(window, cx);
                                }))
                            })
                            .child(tr(if recording {
                                "Натисніть комбінацію…"
                            } else {
                                "Обрати гарячу клавішу"
                            })),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_color(p.muted)
                            .child(shortcut.clone().unwrap_or_else(|| tr("Не задана"))),
                    )
                    .when(recording, |this| {
                        this.child(
                            widgets::button("cancel-recording", p, false)
                                .text_size(px(11.0))
                                .text_color(p.text)
                                .on_click(cx.listener(|this, _, _, cx| this.finish(None, cx)))
                                .child(tr("Скасувати запис")),
                        )
                    })
                    .child(
                        widgets::button("clear-shortcut", p, clear_disabled)
                            .text_size(px(11.0))
                            .text_color(p.text)
                            .when(!clear_disabled, |this| {
                                this.on_click(cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |s, cx| {
                                        s.change(
                                            Patch {
                                                capture_shortcut: Some(None),
                                                ..Patch::default()
                                            },
                                            cx,
                                        )
                                    });
                                }))
                            })
                            .child(tr("Вимкнути гарячу клавішу")),
                    ),
            )
            .child(self.subtle(status_line, p).mt(px(10.0)))
            .when(!problem.is_empty(), |this| {
                this.child(widgets::error_banner(p).child(format!(
                    "{} {}",
                    translate(&problem, locale, &[]),
                    tr("Попередня робоча комбінація залишається активною, якщо заміна не вдалася.")
                )))
            })
            .when_some(status.integration_error.clone(), |this, error| {
                this.child(
                    widgets::notice(p)
                        .text_size(px(10.0))
                        .mb(px(13.0))
                        .child(translate(&error, locale, &[])),
                )
            })
    }

    fn local_card(&mut self, disabled: bool, p: &Palette, locale: Locale, cx: &mut Context<Self>) -> Div {
        let s = self.state.read(cx);
        let directory = s.directory.clone();
        let save_state = s.save_state;
        let tr = |text: &str| t(text, locale);
        let open_disabled = disabled || directory.is_empty();
        let save_text = match save_state {
            SaveState::Error => "Не збережено. Перевірте доступ до папки.",
            SaveState::Saving | SaveState::Unsaved => "Зберігаємо…",
            SaveState::Loading => "Завантажуємо…",
            SaveState::Saved => "Усі зміни збережено",
        };
        self.card(p)
            .child(self.h2(tr("Локальні налаштування")))
            .child(self.muted(
                tr("Ключ, мова, джерело та баланс звуку відновляться після перезапуску. Переклад самостійно не запускається."),
                p,
            ))
            .child(
                div()
                    .bg(p.input)
                    .p(px(12.0))
                    .rounded(px(6.0))
                    .text_color(p.muted)
                    .text_size(px(10.0))
                    .line_height(relative(1.7))
                    .font_family(MONO)
                    .my(px(16.0))
                    .child(if directory.is_empty() {
                        tr("Директорія доступна в desktop-програмі")
                    } else {
                        directory
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .text_size(px(10.0))
                    .text_color(p.muted)
                    .child(
                        div()
                            .when(save_state == SaveState::Error, |this| {
                                this.text_color(theme::error_accent())
                            })
                            .child(tr(save_text)),
                    )
                    .when(save_state == SaveState::Error, |this| {
                        this.child(
                            widgets::button("retry-save", p, false)
                                .text_size(px(14.0))
                                .text_color(p.text)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |s, cx| s.retry_save(cx));
                                }))
                                .child(tr("Повторити збереження")),
                        )
                    })
                    .child(
                        widgets::button("open-directory", p, open_disabled)
                            .text_size(px(11.0))
                            .text_color(p.text)
                            .when(!open_disabled, |this| {
                                this.on_click(cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |s, cx| s.open_directory(cx));
                                }))
                            })
                            .child(tr("Відкрити папку")),
                    ),
            )
    }

    fn appearance_card(&mut self, disabled: bool, p: &Palette, locale: Locale, cx: &mut Context<Self>) -> Div {
        let s = self.state.read(cx);
        let theme_value = s.settings.theme.clone();
        let language = s.settings.interface_language.clone();
        let tr = |text: &str| t(text, locale);
        let invalid = normalize_accent(self.accent_hex.read(cx).value().as_ref()).is_none();

        let theme_options = [("default", "Поточна темна"), ("amoled", "Чорна AMOLED")];
        let theme_label = theme_options
            .iter()
            .find(|(value, _)| *value == theme_value)
            .map_or("Поточна темна", |(_, label)| *label);
        let theme_options = theme_options
            .iter()
            .map(|(value, label)| {
                (
                    tr(label),
                    *value == theme_value,
                    Patch {
                        theme: Some((*value).into()),
                        ..Patch::default()
                    },
                )
            })
            .collect();
        let language_options = [("uk", "Українська"), ("en", "English")];
        let language_label = if language == "en" { "English" } else { "Українська" };
        let language_options = language_options
            .iter()
            .map(|(value, label)| {
                (
                    (*label).to_string(),
                    *value == language,
                    Patch {
                        interface_language: Some((*value).into()),
                        ..Patch::default()
                    },
                )
            })
            .collect();

        self.card(p)
            .child(self.h2(tr("Вигляд і мова")))
            .child(self.muted(tr("Зміни застосовуються одразу та зберігаються автоматично."), p))
            .child(self.field_label(tr("Тема"), p))
            .child(
                self.dropdown("theme", tr(theme_label), theme_options, disabled, p, cx)
                    .mb(px(12.0)),
            )
            .child(self.field_label(tr("Колір акценту"), p))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.0))
                    .mb(px(12.0))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(48.0))
                            .h(px(40.0))
                            .when(disabled, |this| this.opacity(0.42))
                            .child(ColorPicker::new(&self.accent_picker))
                            .when(disabled, |this| {
                                this.child(div().absolute().inset_0().occlude().cursor_not_allowed())
                            }),
                    )
                    .child(div().text_size(px(11.0)).text_color(p.muted).child("HEX"))
                    .child(
                        div()
                            .w(px(110.0))
                            .h(px(INPUT_HEIGHT))
                            .font_family(MONO)
                            .child(self.text_input(&self.accent_hex, disabled, p)),
                    )
                    .child(
                        widgets::button("reset-accent", p, disabled)
                            .text_size(px(11.0))
                            .when(!disabled, |this| {
                                this.on_click(cx.listener(|this, _, window, cx| {
                                    this.accent_hex.update(cx, |input, cx| {
                                        input.set_value(DEFAULT_ACCENT, window, cx)
                                    });
                                    this.state.update(cx, |s, cx| {
                                        s.change(
                                            Patch {
                                                accent_color: Some(DEFAULT_ACCENT.into()),
                                                ..Patch::default()
                                            },
                                            cx,
                                        )
                                    });
                                    cx.notify();
                                }))
                            })
                            .child(tr("Відновити типовий акцент")),
                    ),
            )
            .when(invalid, |this| {
                this.child(
                    self.subtle(tr("Введіть колір у форматі #RRGGBB."), p)
                        .text_color(theme::error_accent()),
                )
            })
            .child(self.field_label(tr("Мова інтерфейсу"), p))
            .child(
                self.dropdown("interface-language", language_label.into(), language_options, disabled, p, cx)
                    .mb(px(12.0)),
            )
            .child(self.subtle(tr("Не змінює мову перекладу аудіо."), p).mb_0())
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, locale, loaded_open, checking) = {
            let s = self.state.read(cx);
            (
                Palette::from_settings(&s.settings),
                s.locale(),
                s.loaded && !s.closing,
                s.checking,
            )
        };
        let _ = window;
        let disabled = !loaded_open;
        let heading = div()
            .mb(px(25.0))
            .child(widgets::eyebrow(t("ВАШ ПРОСТІР", locale), &p))
            .child(
                div()
                    .text_size(px(26.0))
                    .font_weight(SEMI_BOLD)
                    .line_height(relative(1.2))
                    .mb(px(9.0))
                    .child(t("Налаштування", locale)),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(p.muted)
                    .line_height(relative(1.65))
                    .child(t("Зміни зберігаються автоматично.", locale)),
            );

        let connection = self.connection_card(disabled, &p, locale, cx);
        let shortcut = self.shortcut_card(disabled, &p, locale, cx);
        let local = self.local_card(disabled, &p, locale, cx);
        let appearance = self.appearance_card(disabled || checking, &p, locale, cx);

        div()
            .w_full()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key_down(event, cx)))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| this.key_up(event, cx)))
            .child(heading)
            .child(connection)
            .child(shortcut)
            .child(local)
            .child(appearance)
    }
}
