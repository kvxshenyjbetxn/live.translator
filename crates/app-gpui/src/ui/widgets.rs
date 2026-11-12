//! Small building blocks shared by the views: buttons, headings, banners, dots.

use super::theme::{self, Palette};
use gpui::{
    div, point, prelude::*, px, relative, BoxShadow, Div, ElementId, FontWeight, Hsla,
    SharedString, Stateful,
};

pub const SEMI_BOLD: FontWeight = FontWeight(550.0);

/// The global `button` rule: bordered, rounded, hover background, 0.42 opacity when disabled.
pub fn button(id: impl Into<ElementId>, p: &Palette, disabled: bool) -> Stateful<Div> {
    let hover = p.hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .border_1()
        .border_color(p.line)
        .rounded(px(8.0))
        .bg(p.button)
        .text_color(p.text)
        .px(px(13.0))
        .py(px(9.0))
        .when(disabled, |this| this.opacity(0.42).cursor_not_allowed())
        .when(!disabled, |this| {
            this.cursor_pointer().hover(move |style| style.bg(hover))
        })
}

/// A button without chrome (`border:0; background:none`) used by the small icon-like controls.
pub fn bare_button(id: impl Into<ElementId>, p: &Palette, disabled: bool) -> Stateful<Div> {
    let hover = p.hover;
    let text = p.text;
    div()
        .id(id)
        .flex()
        .items_center()
        .text_color(p.muted)
        .when(disabled, |this| this.opacity(0.42).cursor_not_allowed())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(hover).text_color(text))
        })
}

/// `.primary` start button, and `.primary.stop-button` when `stop` is set.
pub fn primary_button(id: impl Into<ElementId>, p: &Palette, disabled: bool, stop: bool) -> Stateful<Div> {
    let hover = p.hover;
    let accent_text = p.accent_text;
    let base = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .border_1()
        .border_color(p.accent_border)
        .rounded(px(8.0))
        .p(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(13.0));
    let base = if stop {
        base.bg(p.selected).text_color(p.text)
    } else {
        base.bg(p.accent).text_color(p.accent_text)
    };
    if disabled {
        base.opacity(0.42).cursor_not_allowed()
    } else if stop {
        base.cursor_pointer().hover(move |style| style.bg(hover))
    } else {
        // Stand-in for the inset 2px accent-text ring of `.primary:hover`.
        base.cursor_pointer()
            .hover(move |style| style.border_color(accent_text))
    }
}

pub fn eyebrow(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .text_size(px(9.0))
        .text_color(p.muted)
        .mb(px(13.0))
        .child(text.into())
}

/// `.page-heading`: eyebrow, h1 and a muted line.
pub fn page_heading(
    eyebrow_text: impl Into<SharedString>,
    title: impl Into<SharedString>,
    muted: impl Into<SharedString>,
    p: &Palette,
    compact: bool,
) -> Div {
    div()
        .mb(px(25.0))
        .child(eyebrow(eyebrow_text, p))
        .child(
            div()
                .text_size(px(if compact { 24.0 } else { 28.0 }))
                .font_weight(SEMI_BOLD)
                .line_height(relative(1.2))
                .mb(px(9.0))
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.muted)
                .line_height(relative(1.65))
                .child(muted.into()),
        )
}

fn banner(p: &Palette, border: Hsla, background: Hsla) -> Div {
    div()
        .border_1()
        .border_color(border)
        .rounded(px(8.0))
        .bg(background)
        .px(px(15.0))
        .py(px(13.0))
        .mb(px(20.0))
        .text_size(px(12.0))
        .line_height(relative(1.7))
        .text_color(p.text)
}

/// `.notice`
pub fn notice(p: &Palette) -> Div {
    banner(p, theme::notice_border(), p.panel)
}

/// `.error`
pub fn error_banner(p: &Palette) -> Div {
    banner(p, theme::error_border(), theme::error_background()).text_color(theme::error_text())
}

/// The 6px status dot; `live` adds the glowing accent ring.
pub fn status_dot(p: &Palette, live: bool) -> Div {
    let dot = div().size(px(6.0)).flex_shrink_0().rounded_full();
    if live {
        dot.bg(p.focus).shadow(vec![BoxShadow {
            color: p.accent_glow,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(4.0),
            inset: false,
        }])
    } else {
        dot.bg(theme::idle_dot())
    }
}
