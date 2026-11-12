//! The latency diagnostics panel (port of `src/components/Diagnostics.svelte`).

use super::{
    theme::{self, Palette},
    widgets::SEMI_BOLD,
};
use crate::logic::{
    diagnostics::{
        diagnostic_messages, diagnostic_rows, format_measurement, DiagnosticRow,
        Graph, RowColor,
    },
    i18n::{translate, Locale},
};
use gpui::{
    canvas, div, fill, point, prelude::*, px, relative, size, Bounds, Hsla, IntoElement,
    ParentElement, PathBuilder, Pixels, Styled, Window,
};
use translator_core::session::diagnostics::DiagnosticsSnapshot;

fn row_color(color: RowColor, p: &Palette) -> Hsla {
    match color {
        RowColor::Fixed([r, g, b]) => {
            gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)).into()
        }
        RowColor::Focus => p.focus,
    }
}

/// Paints the 100×30 view (the SVG `viewBox="-2 -3 104 36"`, stretched without keeping aspect).
fn paint_chart(bounds: Bounds<Pixels>, graph: &Graph, is_network: bool, color: Hsla, line: Hsla, window: &mut Window) {
    let sx = bounds.size.width / 104.0;
    let sy = bounds.size.height / 36.0;
    let at = |x: f64, y: f64| point(bounds.origin.x + sx * (x as f32 + 2.0), bounds.origin.y + sy * (y as f32 + 3.0));

    // Dashed grid (stroke-dasharray "2 3", half-width lines).
    for y in [0.0, 15.0, 30.0] {
        let mut x = 0.0;
        while x < 100.0 {
            let end = (x + 2.0_f32).min(100.0);
            let start = at(f64::from(x), y);
            let stop = at(f64::from(end), y);
            window.paint_quad(fill(
                Bounds::new(start, size(stop.x - start.x, px(1.0))),
                line,
            ));
            x += 5.0;
        }
    }
    for path in &graph.paths {
        let mut builder = PathBuilder::stroke(px(1.5));
        for (index, point_) in path.iter().enumerate() {
            let target = at(point_.x, point_.y);
            if index == 0 {
                builder.move_to(target);
            } else {
                builder.line_to(target);
            }
        }
        if path.len() > 1 {
            if let Ok(built) = builder.build() {
                window.paint_path(built, color);
            }
        }
    }
    for point_ in &graph.points {
        if is_network {
            let top_left = at(point_.x - 4.0, point_.y);
            let bottom_right = at(point_.x + 4.0, 30.0);
            window.paint_quad(fill(
                Bounds::new(top_left, size(bottom_right.x - top_left.x, bottom_right.y - top_left.y)),
                color,
            ));
        } else {
            let centre = at(point_.x, point_.y);
            window.paint_quad(
                fill(Bounds::new(point(centre.x - px(0.75), centre.y - px(0.75)), size(px(1.5), px(1.5))), color)
                    .corner_radii(px(0.75)),
            );
        }
    }
}

fn row_element(
    row: &DiagnosticRow,
    p: &Palette,
    locale: Locale,
    compact: bool,
) -> impl IntoElement {
    let color = row_color(row.color, p);
    let line = p.line;
    let graph = row.graph.clone();
    let is_network = row.is_network;
    let display = match row.value {
        Some(value) => format_measurement(value, row.unit, locale),
        None => translate("Немає даних", locale, &[]),
    };
    let scale = translate(
        "Шкала: 0–{value}",
        locale,
        &[("value", format_measurement(row.max, row.unit, locale))],
    );
    div()
        .flex()
        .items_center()
        .gap(px(if compact { 7.0 } else { 10.0 }))
        .px(px(if compact { 10.0 } else { 14.0 }))
        .py(px(if compact { 10.0 } else { 12.0 }))
        .border_b_1()
        .border_color(p.line)
        .child(
            div()
                .flex_shrink_0()
                .w(px(if compact { 95.0 } else { 116.0 }))
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .line_height(relative(1.5))
                        .mb(px(4.0))
                        .child(translate(row.label, locale, &[])),
                )
                .when(row.is_network, |this| {
                    this.child(
                        div()
                            .text_size(px(8.0))
                            .text_color(p.muted)
                            .line_height(relative(1.4))
                            .child(translate("Останні 10 вимірювань", locale, &[])),
                    )
                })
                .child(
                    div()
                        .text_size(px(8.0))
                        .text_color(p.muted)
                        .line_height(relative(1.4))
                        .child(scale),
                ),
        )
        .child(
            div().flex_1().min_w(px(if compact { 25.0 } else { 35.0 })).h(px(39.0)).child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        paint_chart(bounds, &graph, is_network, color, line.opacity(0.9), window)
                    },
                )
                .size_full(),
            ),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(if compact { 70.0 } else { 85.0 }))
                .text_right()
                .text_size(px(if compact { 10.0 } else { 12.0 }))
                .text_color(color)
                .child(display),
        )
}

pub fn diagnostics_panel(
    diagnostics: &DiagnosticsSnapshot,
    p: &Palette,
    locale: Locale,
    compact: bool,
) -> impl IntoElement {
    let tr = |text: &str| translate(text, locale, &[]);
    let rows = diagnostic_rows(diagnostics);
    let messages = diagnostic_messages(diagnostics);
    let has_issues = !diagnostics.issues.is_empty();
    let note = |text: String| {
        div()
            .mt(px(7.0))
            .text_size(px(9.0))
            .text_color(p.muted)
            .line_height(relative(1.65))
            .child(text)
    };
    let (background, border, text) = if has_issues {
        (theme::issue_background(), theme::issue_border(), theme::issue_text())
    } else {
        (p.panel, p.accent_border, p.text)
    };
    let mut explanation = div()
        .bg(background)
        .border_1()
        .border_color(border)
        .text_color(text)
        .rounded(px(8.0))
        .px(px(12.0))
        .py(px(10.0))
        .my(px(12.0))
        .text_size(px(10.0))
        .line_height(relative(1.65));
    let mut lines: Vec<String> = messages.iter().map(|message| tr(message)).collect();
    if diagnostics.network_rtt_ms.is_none() {
        lines.push(tr("Відгук з’єднання ще невідомий: чекаємо відповіді на Ping. Її відсутність не доводить проблему з інтернетом."));
    }
    for (index, line) in lines.into_iter().enumerate() {
        explanation = explanation.child(div().when(index > 0, |this| this.mt(px(7.0))).child(line));
    }
    let axis_margin = if compact { (112.0, 87.0) } else { (140.0, 109.0) };

    div()
        .mb(px(25.0))
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .mb(px(12.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(SEMI_BOLD)
                        .child(tr("Діагностика затримки")),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(p.muted)
                        .child(tr("Черги · останні 10 с")),
                ),
        )
        .child(
            div()
                .border_1()
                .border_color(p.line)
                .bg(p.surface)
                .rounded(px(9.0))
                .overflow_hidden()
                .children(
                    rows.iter()
                        .map(|row| row_element(row, p, locale, compact)),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(5.0))
                        .mt(px(9.0))
                        .mb(px(10.0))
                        .ml(px(axis_margin.0))
                        .mr(px(axis_margin.1))
                        .text_size(px(8.0))
                        .text_color(p.muted)
                        .child(tr("−10 с"))
                        .child(tr("−5 с"))
                        .child(tr("зараз")),
                ),
        )
        .child(explanation)
        .child(note(tr("Ping → Pong — відгук з’єднання, не час перекладу. Кожен стовпчик — фактичне вимірювання приблизно раз на 4 с. Історія пінгу зсувається лише з новим результатом; давні стовпчики не означають свіжий відгук.")))
        .when_some(diagnostics.send_duration_ms, |this, duration| {
            this.child(note(translate(
                "Останнє надсилання: {value} · завершення запису не означає обробку Google.",
                locale,
                &[("value", format_measurement(duration, "мс", locale))],
            )))
        })
        .child(note(tr("Ці показники не вимірюють точне відставання від відео.")))
}
