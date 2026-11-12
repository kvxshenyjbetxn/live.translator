//! Transcripts screen, ported from `src/components/Transcripts.svelte`.

use gpui::{
    div, point, prelude::*, px, relative, App, BoxShadow, Context, Div, Entity, FontWeight,
    IntoElement, ParentElement, Pixels, Render, ScrollHandle, SharedString, Styled, Window,
};

use crate::theme;

const EMPTY_TEXT: &str = "Поки порожньо. Запустіть переклад, щоб бачити текст наживо.";
const IDLE_LABEL: &str = "Готовий до перекладу";
const LIVE_LABEL: &str = "Перекладаємо наживо";
const PRIVACY_NOTE: &str = "Тексти формує Google; вони залишаються лише в пам’яті після зупинки чи помилки та зникають після нового запуску або закриття програми.";

/// Mirrors `Transcripts` in `translator-core` (original / translated).
#[derive(Default)]
pub struct TranscriptsModel {
    pub input: String,
    pub output: String,
    /// True while a session is running (drives the live dot and status text).
    pub live: bool,
}

pub struct TranscriptsView {
    model: Entity<TranscriptsModel>,
    input_scroll: ScrollHandle,
    output_scroll: ScrollHandle,
}

impl TranscriptsView {
    pub fn new(model: Entity<TranscriptsModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            input_scroll: ScrollHandle::new(),
            output_scroll: ScrollHandle::new(),
        }
    }
}

/// Keeps a pane pinned to the bottom unless the user scrolled up by more than 32px.
fn follow_bottom(handle: &ScrollHandle) {
    let distance = handle.max_offset().y + handle.offset().y;
    if distance <= px(theme::NEAR_BOTTOM_PX) {
        handle.scroll_to_bottom();
    }
}

fn pane(
    id: &'static str,
    label: &'static str,
    text: &str,
    live: bool,
    scroll: &ScrollHandle,
    body_height: Pixels,
) -> Div {
    let dot = div().size(px(6.0)).flex_shrink_0().rounded_full();
    let dot = if live {
        dot.bg(theme::accent()).shadow(vec![BoxShadow {
            color: theme::accent_glow(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(4.0),
            inset: false,
        }])
    } else {
        dot.bg(theme::idle_dot())
    };
    let shown: SharedString = if text.is_empty() {
        EMPTY_TEXT.into()
    } else {
        text.to_string().into()
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .border_1()
        .border_color(theme::line())
        .bg(theme::surface())
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
                .border_color(theme::line())
                .bg(theme::sidebar())
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight(550.0))
                        .child(label),
                )
                .child(dot),
        )
        .child(
            div()
                .id(id)
                .h(body_height)
                .min_h(px(theme::PANE_BODY_MIN_HEIGHT))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .px(px(17.0))
                .py(px(15.0))
                .text_size(px(13.0))
                .line_height(relative(1.7))
                .child(shown),
        )
}

impl Render for TranscriptsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.read(cx);
        let live = model.live;
        follow_bottom(&self.input_scroll);
        follow_bottom(&self.output_scroll);

        let body_height = (window.viewport_size().height - px(theme::PANE_BODY_VIEWPORT_OFFSET))
            .max(px(theme::PANE_BODY_MIN_HEIGHT));

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(25.0))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::eyebrow())
                            .mb(px(13.0))
                            .child("ТЕКСТИ НАЖИВО"),
                    )
                    .child(
                        div()
                            .text_size(px(28.0))
                            .font_weight(FontWeight(550.0))
                            .line_height(relative(1.2))
                            .mb(px(9.0))
                            .child("Вхідний і вихідний текст"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::muted())
                            .child("Зліва — те, що говорить програма; справа — переклад, який озвучує Google."),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme::muted())
                    .mb(px(17.0))
                    .child(if live { LIVE_LABEL } else { IDLE_LABEL }),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .my(px(22.0))
                    .child(pane(
                        "input-body",
                        "Оригінал",
                        &model.input,
                        live,
                        &self.input_scroll,
                        body_height,
                    ))
                    .child(pane(
                        "output-body",
                        "Переклад",
                        &model.output,
                        live,
                        &self.output_scroll,
                        body_height,
                    )),
            )
            .child(
                div()
                    .mt(px(13.0))
                    .text_size(px(10.0))
                    .text_color(theme::privacy_note())
                    .text_center()
                    .child(PRIVACY_NOTE),
            )
    }
}

/// Demo feeder: appends a few words to both panes every ~200 ms.
pub fn spawn_demo_feeder(model: Entity<TranscriptsModel>, cx: &mut App) {
    const INPUT: &[&str] = &[
        "So", "today", "we", "are", "going", "to", "talk", "about", "how", "live", "translation",
        "works", "in", "practice,", "and", "why", "latency", "matters", "more", "than", "anything",
        "else", "when", "two", "people", "speak", "different", "languages.",
    ];
    const OUTPUT: &[&str] = &[
        "Отже,", "сьогодні", "ми", "поговоримо", "про", "те,", "як", "працює", "синхронний",
        "переклад", "на", "практиці,", "і", "чому", "затримка", "важить", "більше", "за", "все",
        "інше,", "коли", "двоє", "людей", "говорять", "різними", "мовами.",
    ];
    model.update(cx, |m, _| m.live = true);
    cx.spawn(async move |cx| {
        let mut i = 0usize;
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;
            model.update(cx, |m, cx| {
                for _ in 0..2 {
                    m.input.push_str(INPUT[i % INPUT.len()]);
                    m.input.push(' ');
                    m.output.push_str(OUTPUT[i % OUTPUT.len()]);
                    m.output.push(' ');
                    i += 1;
                    if i % 12 == 0 {
                        m.input.push('\n');
                        m.output.push('\n');
                    }
                }
                cx.notify();
            });
        }
    })
    .detach();
}
