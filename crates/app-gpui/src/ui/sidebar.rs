//! The audio source list of the sidebar (port of `src/components/Sources.svelte`).

use super::{shell::Shell, theme::Palette, widgets::bare_button};
use crate::{
    logic::{
        i18n::{translate, Locale},
        sources::SourceView,
    },
    state::AppState,
};
use gpui::{
    div, prelude::*, px, relative, svg, Context, Div, FontWeight, IntoElement, ParentElement,
    SharedString, Styled,
};

fn status_label(source: &SourceView) -> &'static str {
    if source.available {
        if source.active {
            "Відтворює звук"
        } else {
            "Доступна програма"
        }
    } else {
        "Програма недоступна"
    }
}

/// App initial in a rounded square, name and status line.
fn source_label(source: &SourceView, p: &Palette, locale: Locale) -> Div {
    let initial: String = source.name.chars().take(1).flat_map(char::to_uppercase).collect();
    div()
        .flex()
        .items_center()
        .gap(px(9.0))
        .flex_1()
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .size(px(30.0))
                .rounded(px(8.0))
                .bg(p.panel)
                .text_color(p.text)
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(initial),
        )
        .child(
            div()
                .min_w_0()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(source.name.clone())
                .child(
                    div()
                        .mt(px(3.0))
                        .text_size(px(10.0))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(p.muted)
                        .child(translate(status_label(source), locale, &[])),
                ),
        )
}

impl Shell {
    pub(super) fn sources_panel(&self, s: &AppState, p: &Palette, cx: &Context<Self>) -> impl IntoElement {
        let locale = s.locale();
        let catalog = s.catalog();
        let tr = |text: &str| translate(text, locale, &[]);
        let disabled = !s.loaded || s.busy() || s.starting || s.checking || s.closing;
        let controls_disabled = !s.loaded || s.closing || s.checking;
        let hidden_count = catalog.hidden.len();

        let rows = catalog.visible.iter().enumerate().map(|(index, source)| {
            let selected = s.settings.selected_source_id.as_deref() == Some(source.id.as_str());
            let (select_id, favorite_view, hide_view) = (source.id.clone(), source.clone(), source.clone());
            let hover = p.hover;
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .my(px(4.0))
                .child(
                    div()
                        .id(SharedString::from(format!("source-{index}")))
                        .flex()
                        .items_center()
                        .flex_1()
                        .min_w_0()
                        .px(px(9.0))
                        .py(px(10.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(if selected { p.accent_border } else { gpui::transparent_black() })
                        .when(selected, |this| this.bg(p.selected))
                        .when(disabled, |this| this.opacity(0.42).cursor_not_allowed())
                        .when(!disabled, |this| {
                            this.cursor_pointer()
                                .hover(move |style| style.bg(hover))
                                .on_click(self.on_state(cx, move |s, cx| {
                                    s.select_source(select_id.clone(), cx)
                                }))
                        })
                        .child(source_label(source, p, locale))
                        .when(source.active, |this| {
                            this.child(
                                div()
                                    .ml_auto()
                                    .flex_shrink_0()
                                    .size(px(5.0))
                                    .rounded_full()
                                    .bg(p.focus),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(1.0))
                        .flex_shrink_0()
                        .child(
                            bare_button(
                                SharedString::from(format!("favorite-{index}")),
                                p,
                                controls_disabled,
                            )
                            .px(px(6.0))
                            .py(px(4.0))
                            .text_size(px(13.0))
                            .line_height(relative(1.0))
                            .when(source.favorite, |this| this.text_color(p.focus))
                            .when(!controls_disabled, |this| {
                                this.on_click(self.on_state(cx, move |s, cx| {
                                    s.toggle_favorite(&favorite_view, cx)
                                }))
                            })
                            .child(if source.favorite { "★" } else { "☆" }),
                        )
                        .child(
                            bare_button(
                                SharedString::from(format!("hide-{index}")),
                                p,
                                controls_disabled,
                            )
                            .px(px(6.0))
                            .py(px(4.0))
                            .when(!controls_disabled, |this| {
                                this.on_click(self.on_state(cx, move |s, cx| {
                                    s.hide_source(&hide_view, cx)
                                }))
                            })
                            .child(
                                svg()
                                    .path("icons/eye-off.svg")
                                    .size(px(14.0))
                                    .flex_shrink_0()
                                    .text_color(p.muted),
                            ),
                        ),
                )
        });

        let hidden_rows = self.show_hidden.then(|| {
            catalog
                .hidden
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    let restore_view = source.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .my(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_1()
                                .min_w_0()
                                .px(px(9.0))
                                .py(px(10.0))
                                .border_1()
                                .border_color(gpui::transparent_black())
                                .child(source_label(source, p, locale)),
                        )
                        .child(
                            bare_button(
                                SharedString::from(format!("restore-{index}")),
                                p,
                                controls_disabled,
                            )
                            .px(px(6.0))
                            .py(px(4.0))
                            .text_size(px(13.0))
                            .line_height(relative(1.0))
                            .when(!controls_disabled, |this| {
                                this.on_click(self.on_state(cx, move |s, cx| {
                                    s.restore_source(&restore_view, cx)
                                }))
                            })
                            .child("↩"),
                        )
                })
                .collect::<Vec<_>>()
        });

        let toggle_hidden = cx.listener(|this, _, _, cx| {
            this.show_hidden = !this.show_hidden;
            cx.notify();
        });
        let hover = p.hover;
        let text = p.text;

        div()
            .mt(px(22.0))
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(9.0))
                    .mb(px(9.0))
                    .text_size(px(9.0))
                    .text_color(p.muted)
                    .child(tr("ДЖЕРЕЛО ЗВУКУ"))
                    .child(
                        div()
                            .id("refresh-sources")
                            .px(px(5.0))
                            .text_size(px(22.0))
                            .line_height(relative(1.2))
                            .text_color(p.muted)
                            .when(disabled, |this| this.opacity(0.42).cursor_not_allowed())
                            .when(!disabled, |this| {
                                this.cursor_pointer()
                                    .hover(move |style| style.bg(hover))
                                    .on_click(self.on_state(cx, |s, cx| s.refresh(cx)))
                            })
                            .child("↻"),
                    ),
            )
            .child(
                div()
                    .id("source-list")
                    .min_h_0()
                    .pr(px(12.0))
                    .overflow_y_scroll()
                    .children(rows)
                    .when(catalog.visible.is_empty(), |this| {
                        this.child(
                            div()
                                .px(px(10.0))
                                .py(px(8.0))
                                .text_size(px(11.0))
                                .line_height(relative(1.65))
                                .text_color(p.muted)
                                .child(tr(
                                    "Немає доступних джерел. Увімкніть звук у потрібній програмі й оновіть список.",
                                )),
                        )
                    }),
            )
            .when(hidden_count > 0, |this| {
                this.child(
                    div()
                        .id("hidden-toggle")
                        .flex_shrink_0()
                        .w_full()
                        .mt(px(8.0))
                        .px(px(9.0))
                        .py(px(7.0))
                        .text_size(px(10.0))
                        .text_color(p.muted)
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover).text_color(text))
                        .on_click(toggle_hidden)
                        .child(translate(
                            "Приховані ({count})",
                            locale,
                            &[("count", hidden_count.to_string())],
                        )),
                )
            })
            .when_some(hidden_rows, |this, rows| {
                this.child(
                    div()
                        .id("hidden-list")
                        .min_h_0()
                        .mt(px(5.0))
                        .pt(px(5.0))
                        .pr(px(12.0))
                        .border_t_1()
                        .border_color(p.line)
                        .overflow_y_scroll()
                        .children(rows),
                )
            })
    }
}
