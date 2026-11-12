//! The translation page (port of `src/components/Translation.svelte`).

use super::{
    diagnostics::diagnostics_panel,
    shell::Shell,
    theme::Palette,
    widgets::{self, primary_button, status_dot, SEMI_BOLD},
};
use crate::{
    logic::{
        i18n::{translate, Locale},
        patch::Patch,
    },
    state::AppState,
};
use gpui::{
    deferred, div, prelude::*, px, relative, Context, Div, Entity, IntoElement, ParentElement,
    SharedString, Styled,
};
use gpui_component::slider::{Slider, SliderState};
use translator_core::{
    session::SessionPhase,
    settings::LANGUAGES,
};

fn session_label(s: &AppState, phase: SessionPhase) -> &'static str {
    if s.waiting.is_some() {
        return "Очікуємо на програму";
    }
    match phase {
        SessionPhase::Starting => "Підключаємося до Google…",
        SessionPhase::Stopping => {
            if s.is_windows {
                "Зупиняємо…"
            } else {
                "Повертаємо оригінальний звук…"
            }
        }
        SessionPhase::Running => {
            if s.status.received_audio_seconds > 0.0 {
                "Перекладаємо наживо"
            } else {
                "Слухаємо · очікуємо переклад"
            }
        }
        SessionPhase::Error => "Переклад зупинено",
        SessionPhase::Idle => "Готовий до перекладу",
    }
}

fn note(p: &Palette, text: String, margin_top: f32) -> Div {
    div()
        .mt(px(margin_top))
        .text_size(px(10.0))
        .text_color(p.muted)
        .line_height(relative(1.8))
        .child(text)
}

impl Shell {
    fn language_select(
        &self,
        s: &AppState,
        p: &Palette,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let locale = s.locale();
        let current = LANGUAGES
            .iter()
            .find(|language| language.code == s.settings.target_language)
            .map_or(s.settings.target_language.as_str(), |language| language.name);
        let open = self.language_open && !disabled;
        let hover = p.hover;
        let items = LANGUAGES.iter().enumerate().map(|(index, language)| {
            let code = language.code;
            let chosen = language.code == s.settings.target_language;
            div()
                .id(("language", index))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(13.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .when(chosen, |this| this.bg(p.selected))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.language_open = false;
                    this.state
                        .update(cx, |s, cx| s.change(Patch::target_language(code), cx));
                    cx.notify();
                }))
                .child(translate(language.name, locale, &[]))
        });
        div()
            .relative()
            .child(
                div()
                    .id("target-language")
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
                        this.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                            this.language_open = !this.language_open;
                            cx.notify();
                        }))
                    })
                    .child(translate(current, locale, &[]))
                    .child(div().text_size(px(10.0)).text_color(p.muted).child("▾")),
            )
            .when(open, |this| {
                this.child(deferred(
                    div()
                        .id("language-menu")
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
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.language_open = false;
                            cx.notify();
                        }))
                        .children(items),
                ))
            })
    }

    fn volume_row(
        &self,
        label: String,
        value: f32,
        slider: &Entity<SliderState>,
        disabled: bool,
        compact: bool,
        p: &Palette,
    ) -> impl IntoElement {
        let percent = (value * 100.0).round() as i32;
        div()
            .flex()
            .items_center()
            .gap(px(if compact { 10.0 } else { 15.0 }))
            .my(px(21.0))
            .text_size(px(12.0))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(if compact { 75.0 } else { 86.0 }))
                    .text_color(p.muted)
                    .child(label),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(30.0))
                    .child(Slider::new(slider).horizontal().bg(p.accent).text_color(p.accent))
                    .when(disabled, |this| {
                        // A disabled range input is dimmed and ignores the pointer.
                        this.opacity(0.42)
                            .child(div().absolute().inset_0().occlude().cursor_not_allowed())
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(if compact { 33.0 } else { 35.0 }))
                    .text_right()
                    .child(format!("{percent}%")),
            )
    }

    pub(super) fn translation(
        &self,
        s: &AppState,
        p: &Palette,
        compact: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let locale: Locale = s.locale();
        let tr = |text: &str| translate(text, locale, &[]);
        let phase = s.display_phase();
        let busy = matches!(
            phase,
            SessionPhase::Starting | SessionPhase::Running | SessionPhase::Stopping
        );
        let transition = matches!(phase, SessionPhase::Starting | SessionPhase::Stopping);
        let disabled = !s.loaded || s.closing || s.checking || s.recording;
        let waiting = s.waiting.is_some();
        let source_name = s.selected_name();
        let status = &s.status;
        let level = (status.input_level.clamp(0.0, 1.0) * 100.0).round() as i32;
        let time = format!("{:02}:{:02}", status.elapsed_seconds / 60, status.elapsed_seconds % 60);
        let has_key = !s.settings.api_key.trim().is_empty();

        let action_button = if waiting {
            primary_button("cancel-wait", p, disabled, true)
                .on_click(self.on_state(cx, |s, cx| s.stop_translation(cx)))
                .child(format!("■ \u{a0} {}", tr("Скасувати очікування")))
                .into_any_element()
        } else if phase == SessionPhase::Running || transition {
            let stopping = phase == SessionPhase::Stopping;
            primary_button("stop", p, disabled || stopping, true)
                .on_click(self.on_state(cx, |s, cx| s.stop_translation(cx)))
                .child(format!(
                    "■ \u{a0} {}",
                    tr(if stopping { "Зупиняємо…" } else { "Зупинити переклад" })
                ))
                .into_any_element()
        } else {
            primary_button("start", p, disabled || !s.can_start_source(), false)
                .on_click(self.on_state(cx, |s, cx| s.start_selected(cx)))
                .child(format!("▶ \u{a0} {}", tr("Почати переклад")))
                .into_any_element()
        };

        let metric = |text: &str, value: String| {
            translate(text, locale, &[("value", value)])
        };

        div()
            .child(widgets::page_heading(
                tr("ЖИВИЙ ПЕРЕКЛАД"),
                tr("Слухай своєю мовою"),
                source_name
                    .clone()
                    .map(SharedString::from)
                    .unwrap_or_else(|| tr("Виберіть програму зі списку ліворуч").into()),
                p,
                compact,
            ))
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(11.0))
                    .text_color(p.muted)
                    .child(tr("Перекладати на")),
            )
            .child(self.language_select(s, p, disabled || busy, cx))
            .child(
                div()
                    .border_1()
                    .border_color(p.accent_border)
                    .bg(p.surface)
                    .rounded(px(11.0))
                    .px(px(20.0))
                    .py(px(19.0))
                    .mt(px(22.0))
                    .mb(px(25.0))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(9.0))
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(status_dot(p, phase == SessionPhase::Running))
                                    .child(tr(session_label(s, phase))),
                            )
                            .child(div().text_size(px(12.0)).text_color(p.muted).child(time)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .mt(px(30.0))
                            .mb(px(22.0))
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(6.0))
                                    .bg(p.line)
                                    .rounded(px(6.0))
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(level as f32 / 100.0))
                                            .bg(p.focus)
                                            .rounded(px(6.0)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(29.0))
                                    .text_right()
                                    .text_size(px(10.0))
                                    .text_color(p.muted)
                                    .child(format!("{level}%")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(px(12.0))
                            .text_size(px(9.0))
                            .text_color(p.muted)
                            .child(tr("Реальний рівень джерела"))
                            .child("Gemini Live Translate"),
                    ),
            )
            .when(phase == SessionPhase::Running, |this| {
                this.child(diagnostics_panel(&status.diagnostics, p, locale, compact))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(15.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(SEMI_BOLD)
                            .child(tr(if s.is_windows {
                                "Гучність перекладу"
                            } else {
                                "Баланс звуку"
                            })),
                    )
                    .when(!s.is_windows, |this| {
                        this.child(
                            div()
                                .text_size(px(9.0))
                                .text_color(p.muted)
                                .child(tr("Незалежна гучність")),
                        )
                    }),
            )
            .when(!s.is_windows, |this| {
                this.child(self.volume_row(
                    tr("Оригінал"),
                    s.settings.original_volume,
                    &self.original_slider,
                    disabled,
                    compact,
                    p,
                ))
            })
            .child(self.volume_row(
                tr("Переклад"),
                s.settings.translation_volume,
                &self.translation_slider,
                disabled,
                compact,
                p,
            ))
            .child(
                div()
                    .mt(px(20.0))
                    .mb(px(24.0))
                    .text_size(px(10.0))
                    .text_color(p.muted)
                    .line_height(relative(1.8))
                    .child(tr(if s.is_windows {
                        "Оригінал відтворюється програмою. Керуйте ним у програмі або мікшері Windows; тут регулюється лише переклад."
                    } else {
                        "0% оригіналу — лише переклад. Оригінал звучить наживо, переклад — із невеликим відставанням. За потреби озвучка плавно прискорюється до 1.12×, щоб наздогнати."
                    })),
            )
            .child(action_button)
            .child(
                div()
                    .mt(px(13.0))
                    .text_center()
                    .text_size(px(10.0))
                    .text_color(p.muted)
                    .line_height(relative(1.65))
                    .child(tr(if waiting {
                        "Аудіо ще не передається Google: переклад почнеться, коли програма з’явиться."
                    } else {
                        "↗ Аудіо вибраної програми передається Google"
                    })),
            )
            .when(waiting, |this| {
                this.child(note(
                    p,
                    tr("Переклад почнеться автоматично, щойно програма з’явиться серед джерел. Програма не відкривається сама."),
                    15.0,
                ))
            })
            .when(!waiting && !busy && !has_key, |this| {
                this.child(note(p, tr("Додайте API-ключ у налаштуваннях, щоб почати."), 15.0))
            })
            .when(!waiting && !busy && has_key && source_name.is_none(), |this| {
                this.child(note(
                    p,
                    tr("Виберіть джерело звуку. Якщо його немає — увімкніть звук у програмі й оновіть список."),
                    15.0,
                ))
            })
            .when(busy, |this| {
                this.child(note(
                    p,
                    tr(if s.is_windows {
                        "Щоб змінити джерело або мову, зупиніть переклад. Оригінальний звук залишається у програмі."
                    } else {
                        "Щоб змінити джерело або мову, зупиніть переклад. Зупинка повертає оригінал і відсікає хвіст озвучки."
                    }),
                    15.0,
                ))
            })
            .when_some(status.message.clone(), |this, message| {
                this.child(div().mt(px(15.0)).child(widgets::error_banner(p).child(tr(&message))))
            })
            .when(status.sent_audio_seconds > 0.0, |this| {
                this.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(10.0))
                        .mt(px(20.0))
                        .pt(px(13.0))
                        .border_t_1()
                        .border_color(p.line)
                        .text_size(px(9.0))
                        .text_color(p.muted)
                        .child(metric("Передано {value} с", format!("{:.1}", status.sent_audio_seconds)))
                        .child(metric("Озвучка {value} с", format!("{:.1}", status.received_audio_seconds)))
                        .child(metric("Буфер {value} с", format!("{:.1}", status.playback_backlog_seconds)))
                        .child(metric("Темп {value}×", format!("{:.2}", status.playback_speed))),
                )
            })
    }
}
