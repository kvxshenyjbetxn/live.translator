//! The close overlay and dialog (the `closing` block of `src/App.svelte`).

use super::{
    shell::Shell,
    theme::{self, Palette},
    widgets::{self, SEMI_BOLD},
};
use crate::{logic::i18n::translate, state::AppState};
use gpui::{div, prelude::*, px, relative, Context, IntoElement, ParentElement, Styled};

impl Shell {
    pub(super) fn close_overlay(&self, s: &AppState, p: &Palette, cx: &Context<Self>) -> impl IntoElement {
        let locale = s.locale();
        let tr = |text: &str| translate(text, locale, &[]);
        let failed = s.close_failed;
        let (title, body) = if failed {
            (
                "Не вдалося завершити роботу",
                "Не вдалося зберегти зміни або завершити очищення аудіо. Можна повторити спробу чи вийти без збереження; очищення аудіо все одно обов’язкове.",
            )
        } else {
            (
                "Завершуємо роботу…",
                "Зберігаємо останні зміни та повертаємо оригінальний звук.",
            )
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::overlay())
            .occlude()
            .child(
                div()
                    .w(px(440.0))
                    .max_w(relative(0.9))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.accent_border)
                    .rounded(px(12.0))
                    .p(px(25.0))
                    .child(
                        div()
                            .mb(px(12.0))
                            .text_size(px(18.0))
                            .font_weight(SEMI_BOLD)
                            .child(tr(title)),
                    )
                    .child(
                        div()
                            .mb(px(12.0))
                            .text_size(px(12.0))
                            .text_color(p.muted)
                            .line_height(relative(1.65))
                            .child(tr(body)),
                    )
                    .when(failed, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.0))
                                .child(
                                    widgets::button("close-retry", p, false)
                                        .on_click(self.on_state(cx, |s, cx| s.request_close(false, cx)))
                                        .child(tr("Повторити")),
                                )
                                .child(
                                    widgets::button("close-discard", p, false)
                                        .on_click(self.on_state(cx, |s, cx| s.request_close(true, cx)))
                                        .child(tr("Вийти без збереження")),
                                )
                                .child(
                                    widgets::button("close-cancel", p, false)
                                        .on_click(self.on_state(cx, |s, cx| s.cancel_close(cx)))
                                        .child(tr("Залишитися")),
                                ),
                        )
                    }),
            )
    }
}
