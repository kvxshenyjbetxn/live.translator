//! The live texts page (port of `src/components/Transcripts.svelte`).

use super::{
    shell::Shell,
    theme::Palette,
    widgets::{self, status_dot, SEMI_BOLD},
};
use crate::{
    logic::i18n::{translate, Locale},
    state::AppState,
};
use gpui::{
    div, prelude::*, px, relative, Context, Div, IntoElement, ParentElement, Pixels, ScrollHandle,
    SharedString, Styled, Window,
};
use translator_core::session::SessionPhase;

/// `nearBottom` threshold of the Svelte component.
const NEAR_BOTTOM_PX: f32 = 32.0;
/// `.pane-body` height is `calc(100vh - 300px)` with `min-height: 280px`.
const PANE_BODY_VIEWPORT_OFFSET: f32 = 300.0;
const PANE_BODY_MIN_HEIGHT: f32 = 280.0;

/// Keeps a pane pinned to the bottom unless the user scrolled up by more than 32px.
fn follow_bottom(handle: &ScrollHandle) {
    let distance = handle.max_offset().y + handle.offset().y;
    if distance <= px(NEAR_BOTTOM_PX) {
        handle.scroll_to_bottom();
    }
}

#[allow(clippy::too_many_arguments)]
fn pane(
    id: &'static str,
    label: String,
    text: &str,
    empty: String,
    live: bool,
    scroll: &ScrollHandle,
    body_height: Pixels,
    p: &Palette,
) -> Div {
    let shown: SharedString = if text.is_empty() {
        empty.into()
    } else {
        text.to_string().into()
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .border_1()
        .border_color(p.line)
        .bg(p.surface)
        .rounded(px(11.0))
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .px(px(15.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(p.line)
                .bg(p.sidebar)
                .child(div().text_size(px(11.0)).font_weight(SEMI_BOLD).child(label))
                .child(status_dot(p, live)),
        )
        .child(
            div()
                .id(id)
                .h(body_height)
                .min_h(px(PANE_BODY_MIN_HEIGHT))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .px(px(17.0))
                .py(px(15.0))
                .text_size(px(13.0))
                .line_height(relative(1.7))
                .child(shown),
        )
}

impl Shell {
    pub(super) fn transcripts(
        &self,
        s: &AppState,
        p: &Palette,
        compact: bool,
        window: &Window,
        _cx: &Context<Self>,
    ) -> impl IntoElement {
        let locale: Locale = s.locale();
        let tr = |text: &str| translate(text, locale, &[]);
        let phase = s.display_phase();
        let live = matches!(phase, SessionPhase::Running | SessionPhase::Starting);
        let phase_label = match phase {
            SessionPhase::Running => "Перекладаємо наживо",
            SessionPhase::Starting => "Підключаємося до Google…",
            SessionPhase::Stopping => "Повертаємо оригінальний звук…",
            SessionPhase::Error => "Переклад зупинено",
            SessionPhase::Idle => "Готовий до перекладу",
        };
        let transcripts = &s.status.transcripts;
        follow_bottom(&self.input_scroll);
        follow_bottom(&self.output_scroll);
        let body_height = (window.viewport_size().height - px(PANE_BODY_VIEWPORT_OFFSET))
            .max(px(PANE_BODY_MIN_HEIGHT));
        let empty = || tr("Поки порожньо. Запустіть переклад, щоб бачити текст наживо.");

        div()
            .child(widgets::page_heading(
                tr("ТЕКСТИ НАЖИВО"),
                tr("Вхідний і вихідний текст"),
                tr("Зліва — те, що говорить програма; справа — переклад, який озвучує Google."),
                p,
                compact,
            ))
            .child(
                div()
                    .mb(px(17.0))
                    .text_size(px(12.0))
                    .text_color(p.muted)
                    .line_height(relative(1.65))
                    .child(tr(phase_label)),
            )
            .when(phase == SessionPhase::Error, |this| {
                this.when_some(s.status.message.clone(), |this, message| {
                    this.child(widgets::error_banner(p).child(tr(&message)))
                })
            })
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .my(px(22.0))
                    .child(pane(
                        "input-body",
                        tr("Оригінал"),
                        &transcripts.original,
                        empty(),
                        live,
                        &self.input_scroll,
                        body_height,
                        p,
                    ))
                    .child(pane(
                        "output-body",
                        tr("Переклад"),
                        &transcripts.translated,
                        empty(),
                        live,
                        &self.output_scroll,
                        body_height,
                        p,
                    )),
            )
            .child(
                div()
                    .mt(px(13.0))
                    .text_size(px(10.0))
                    .text_color(p.muted)
                    .text_center()
                    .line_height(relative(1.65))
                    .child(tr("Тексти формує Google; вони залишаються лише в пам’яті після зупинки чи помилки та зникають після нового запуску або закриття програми.")),
            )
    }
}
