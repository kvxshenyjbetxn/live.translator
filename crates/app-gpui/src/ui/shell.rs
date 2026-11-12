//! The window shell: sidebar, main column, footer and the close overlay (the markup of
//! `src/App.svelte`).

use super::{
    settings::SettingsPage,
    theme::{self, Palette},
    widgets::{self, SEMI_BOLD},
};
use crate::{
    logic::{
        autosave::SaveState,
        i18n::{t, Locale},
        patch::Patch,
    },
    native,
    state::{AppEvent, AppState, Tab},
};
use gpui::{
    div, prelude::*, px, relative, App, ClickEvent, Context, Entity, IntoElement, ParentElement,
    Render, ScrollHandle, SharedString, Styled, Window,
};
use gpui_component::slider::{SliderEvent, SliderState};

pub struct Shell {
    pub(super) state: Entity<AppState>,
    settings_page: Entity<SettingsPage>,
    pub(super) original_slider: Entity<SliderState>,
    pub(super) translation_slider: Entity<SliderState>,
    pub(super) language_open: bool,
    pub(super) show_hidden: bool,
    pub(super) input_scroll: ScrollHandle,
    pub(super) output_scroll: ScrollHandle,
}

fn volume_slider(value: f32, cx: &mut Context<Shell>) -> Entity<SliderState> {
    cx.new(|_| {
        SliderState::new()
            .min(0.0)
            .max(100.0)
            .step(1.0)
            .default_value(value)
    })
}

impl Shell {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (original, translation) = {
            let settings = &state.read(cx).settings;
            (settings.original_volume * 100.0, settings.translation_volume * 100.0)
        };
        let original_slider = volume_slider(original.round(), cx);
        let translation_slider = volume_slider(translation.round(), cx);

        cx.subscribe(&original_slider, |this, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(value) = event {
                let volume = (value.start() / 100.0).clamp(0.0, 1.0);
                this.state.update(cx, |s, cx| {
                    s.change(
                        Patch {
                            original_volume: Some(volume),
                            ..Patch::default()
                        },
                        cx,
                    )
                });
            }
        })
        .detach();
        cx.subscribe(&translation_slider, |this, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(value) = event {
                let volume = (value.start() / 100.0).clamp(0.0, 1.0);
                this.state.update(cx, |s, cx| {
                    s.change(
                        Patch {
                            translation_volume: Some(volume),
                            ..Patch::default()
                        },
                        cx,
                    )
                });
            }
        })
        .detach();

        // Settings can change from outside the sliders (initial load, edits elsewhere).
        cx.observe_in(&state, window, |this, state, window, cx| {
            let (original, translation) = {
                let settings = &state.read(cx).settings;
                (
                    (settings.original_volume * 100.0).round(),
                    (settings.translation_volume * 100.0).round(),
                )
            };
            for (slider, wanted) in [
                (&this.original_slider, original),
                (&this.translation_slider, translation),
            ] {
                if slider.read(cx).value().start().round() != wanted {
                    slider.update(cx, |slider, cx| slider.set_value(wanted, window, cx));
                }
            }
            cx.notify();
        })
        .detach();

        // Tray and failure paths ask for the window to be shown.
        cx.subscribe_in(&state, window, |_, _, event: &AppEvent, window, _| match event {
            AppEvent::ShowWindow => native::show(window),
        })
        .detach();

        let settings_page = cx.new(|cx| SettingsPage::new(state.clone(), window, cx));

        Self {
            state,
            settings_page,
            original_slider,
            translation_slider,
            language_open: false,
            show_hidden: false,
            input_scroll: ScrollHandle::new(),
            output_scroll: ScrollHandle::new(),
        }
    }

    /// A click handler that runs `action` on the application state.
    pub(super) fn on_state(
        &self,
        cx: &Context<Self>,
        action: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let state = self.state.clone();
        let _ = cx;
        move |_, _, cx| {
            state.update(cx, |s, cx| action(s, cx));
        }
    }

    fn navigation_item(
        &self,
        id: &'static str,
        label: SharedString,
        tab: Tab,
        s: &AppState,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let chosen = s.tab == tab;
        let hover = p.hover;
        div()
            .id(id)
            .flex()
            .items_center()
            .w_full()
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .text_size(px(12.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(chosen, |this| {
                this.bg(p.selected).text_color(p.text).border_color(p.accent_border)
            })
            .when(!chosen, |this| {
                this.border_color(gpui::transparent_black()).text_color(p.muted)
            })
            .on_click(self.on_state(cx, move |s, cx| s.set_tab(tab, cx)))
            .child(label)
    }

    fn brand(&self, p: &Palette, locale: Locale, compact: bool) -> impl IntoElement {
        div()
            .flex()
            .gap(px(10.0))
            .items_center()
            .pb(px(26.0))
            .pr(px(9.0))
            .pl(px(if compact { 4.0 } else { 9.0 }))
            .font_weight(SEMI_BOLD)
            .text_size(px(if compact { 14.0 } else { 15.0 }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .w(px(32.0))
                    .h(px(34.0))
                    .border_1()
                    .border_color(p.accent_border)
                    .bg(p.selected)
                    .text_color(p.focus)
                    .text_size(px(24.0))
                    .rounded(px(9.0))
                    .child("≋"),
            )
            .child(
                div()
                    .child("Live Translator")
                    .child(
                        div()
                            .mt(px(5.0))
                            .text_size(px(if compact { 7.0 } else { 8.0 }))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(p.muted)
                            .child(t("ТВІЙ ЖИВИЙ ПЕРЕКЛАД", locale)),
                    ),
            )
    }

    fn sidebar(&self, s: &AppState, p: &Palette, compact: bool, cx: &Context<Self>) -> impl IntoElement {
        let locale = s.locale();
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(if compact { 205.0 } else { 230.0 }))
            .h_full()
            .min_h(px(550.0))
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.line)
            .pt(px(if compact { 23.0 } else { 26.0 }))
            .pb(px(if compact { 16.0 } else { 17.0 }))
            .px(px(if compact { 10.0 } else { 14.0 }))
            .child(self.brand(p, locale, compact))
            .child(self.navigation_item(
                "nav-translation",
                format!("◉ \u{a0} {}", t("Переклад", locale)).into(),
                Tab::Translation,
                s,
                p,
                cx,
            ))
            .child(self.navigation_item(
                "nav-texts",
                format!("☰ \u{a0} {}", t("Тексти", locale)).into(),
                Tab::Texts,
                s,
                p,
                cx,
            ))
            .child(self.sources_panel(s, p, cx))
            .child(div().flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .pt(px(20.0))
                    .child(
                        div()
                            .px(px(10.0))
                            .pt(px(12.0))
                            .pb(px(6.0))
                            .mb(px(10.0))
                            .border_t_1()
                            .border_color(p.line)
                            .text_size(px(10.0))
                            .line_height(relative(1.7))
                            .text_color(p.muted)
                            .child(t(
                                "Захоплюється лише вибрана програма. Інші звуки не змінюються.",
                                locale,
                            )),
                    )
                    .child(self.navigation_item(
                        "nav-settings",
                        format!("⚙ \u{a0} {}", t("Налаштування", locale)).into(),
                        Tab::Settings,
                        s,
                        p,
                        cx,
                    ))
                    .child(
                        div()
                            .mt(px(15.0))
                            .text_center()
                            .text_size(px(9.0))
                            .text_color(p.muted)
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
    }

    fn footer(&self, s: &AppState, p: &Palette) -> impl IntoElement {
        let locale = s.locale();
        let (text, failed) = if !s.loaded {
            ("Налаштування недоступні", false)
        } else if s.save_state == SaveState::Error {
            ("● Не збережено · відкрийте налаштування", true)
        } else if matches!(s.save_state, SaveState::Saving | SaveState::Unsaved) {
            ("◌ Зберігаємо…", false)
        } else {
            ("● Усі зміни збережено", false)
        };
        div()
            .flex()
            .justify_between()
            .gap(px(12.0))
            .px(px(20.0))
            .py(px(12.0))
            .bg(p.sidebar)
            .border_t_1()
            .border_color(p.line)
            .text_size(px(9.0))
            .text_color(p.muted)
            .child(
                div()
                    .when(failed, |this| this.text_color(theme::error_accent()))
                    .child(t(text, locale)),
            )
            .child(div().child(t("Аудіозапис не зберігається", locale)))
    }

    fn banners(&self, s: &AppState, p: &Palette, cx: &Context<Self>) -> impl IntoElement {
        let locale = s.locale();
        div()
            .when(!s.error.is_empty(), |this| {
                this.child(widgets::error_banner(p).child(t(&s.error, locale)))
            })
            .when(!s.source_error.is_empty() && s.tab == Tab::Translation, |this| {
                this.child(widgets::notice(p).child(t(&s.source_error, locale)))
            })
            .when(!s.loaded, |this| {
                this.child(
                    widgets::notice(p)
                        .child(t(
                            if s.loading {
                                "Завантажуємо налаштування…"
                            } else {
                                "Налаштування не завантажено. Файл не буде перезаписано."
                            },
                            locale,
                        ))
                        .when(!s.loading, |notice| {
                            notice.child(
                                div().mt(px(10.0)).child(
                                    widgets::button("retry-load", p, false)
                                        .text_size(px(11.0))
                                        .on_click(self.on_state(cx, |s, cx| s.retry_load(cx)))
                                        .child(t("Повторити завантаження", locale)),
                                ),
                            )
                        }),
                )
            })
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let s = state.read(cx);
        let p = Palette::from_settings(&s.settings);
        let viewport = window.viewport_size();
        let compact = viewport.width <= px(theme::COMPACT_WIDTH);
        let cx: &Context<Self> = cx;

        let page = match s.tab {
            Tab::Settings => self.settings_page.clone().into_any_element(),
            Tab::Texts => self.transcripts(s, &p, compact, window, cx).into_any_element(),
            Tab::Translation => self.translation(s, &p, compact, cx).into_any_element(),
        };

        let main_column = div()
            .id("page")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .child(
                div()
                    .min_h(viewport.height)
                    .flex()
                    .flex_col()
                    .child(
                        div().flex_1().w_full().child(
                            div()
                                .w_full()
                                .max_w(px(830.0))
                                .mx_auto()
                                .px(px(if compact { 24.0 } else { 34.0 }))
                                .pt(px(if compact { 25.0 } else { 30.0 }))
                                .pb(px(if compact { 25.0 } else { 24.0 }))
                                .child(self.banners(s, &p, cx))
                                .child(page),
                        ),
                    )
                    .child(self.footer(s, &p)),
            );

        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(p.background)
            .text_color(p.text)
            .font_family(theme::FONT_FAMILY)
            .text_size(px(14.0))
            .line_height(relative(1.3))
            .child(self.sidebar(s, &p, compact, cx))
            .child(main_column)
            .when(s.closing, |this| this.child(self.close_overlay(s, &p, cx)))
    }
}
