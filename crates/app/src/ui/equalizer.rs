//! A floating pill that visualises the translated speech while translation is active.

use super::theme::Palette;
use crate::{native, state::AppState};
use gpui::{
    div, px, Context, Div, Entity, IntoElement, ParentElement, Render, Styled, Task, Window,
};
use std::{collections::VecDeque, time::Duration};
use translator_core::session::SessionPhase;

pub const BAR_COUNT: usize = 30;
const BAR_WIDTH: f32 = 3.0;
const BAR_GAP: f32 = 2.0;
const MAX_BAR: f32 = 32.0;
const MIN_BAR: f32 = 2.0;
pub const PILL_HEIGHT: f32 = 48.0;
const PILL_PADDING: f32 = 16.0;
const TICK_MS: u64 = 50;
const FADE_BARS: usize = 8;
const SILENCE_LEVEL: f32 = 0.02;
const SILENCE_ALPHA: f32 = 0.25;
const ATTACK: f32 = 0.6;
const RELEASE: f32 = 0.25;
const FLOOR_DB: f32 = 55.0;
const RANGE_DB: f32 = 50.0;

/// Full width of the pill (border-box); the overlay window is exactly this wide.
pub const PILL_WIDTH: f32 = PILL_PADDING * 2.0
    + 2.0
    + BAR_COUNT as f32 * BAR_WIDTH
    + (BAR_COUNT as f32 - 1.0) * BAR_GAP;

pub struct Equalizer {
    state: Entity<AppState>,
    columns: VecDeque<f32>,
    smoothed: f32,
    task: Option<Task<()>>,
    /// Native handle of the overlay window this view is the root of.
    hwnd: isize,
    shown: bool,
}

impl Equalizer {
    pub fn new(state: Entity<AppState>, hwnd: isize, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, state, cx| {
            let busy = state.read(cx).busy();
            if busy && this.task.is_none() {
                this.task = Some(cx.spawn(async move |this, cx| loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(TICK_MS))
                        .await;
                    if this.update(cx, |eq, cx| eq.tick(cx)).is_err() {
                        break;
                    }
                }));
            } else if !busy && this.task.is_some() {
                this.task = None;
                this.columns.clear();
                this.smoothed = 0.0;
            }
            let visible = Self::visible(state.read(cx));
            if visible != this.shown {
                this.shown = visible;
                // Win32 show/move re-enters GPUI's window procedure; do it outside this
                // observer (which runs inside an App update), in a foreground task.
                let hwnd = this.hwnd;
                cx.spawn(async move |_, _| {
                    if visible {
                        native::show_overlay(hwnd, f64::from(PILL_WIDTH), f64::from(PILL_HEIGHT));
                    } else {
                        native::hide_overlay(hwnd);
                    }
                })
                .detach();
            }
            cx.notify();
        })
        .detach();
        Self {
            state,
            columns: VecDeque::with_capacity(BAR_COUNT + 1),
            smoothed: 0.0,
            task: None,
            hwnd,
            shown: false,
        }
    }

    /// Whether the pill is drawn (translation is active and not shutting down).
    pub fn visible(state: &AppState) -> bool {
        state.busy() && state.display_phase() != SessionPhase::Stopping
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let rms = self.state.read(cx).output_level();
        let db = 20.0 * rms.max(1e-6).log10();
        let target = ((db + FLOOR_DB) / RANGE_DB).clamp(0.0, 1.0);
        let k = if target > self.smoothed { ATTACK } else { RELEASE };
        self.smoothed += (target - self.smoothed) * k;
        self.columns.push_back(self.smoothed);
        while self.columns.len() > BAR_COUNT {
            self.columns.pop_front();
        }
        cx.notify();
    }
}

fn bar_alpha(index: usize, level: f32) -> f32 {
    let silence = if level < SILENCE_LEVEL { SILENCE_ALPHA } else { 1.0 };
    let fade = if index < FADE_BARS {
        0.15 + 0.85 * index as f32 / FADE_BARS as f32
    } else {
        1.0
    };
    silence * fade
}

impl Render for Equalizer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        if !Self::visible(s) {
            return div();
        }
        let p = Palette::from_settings(&s.settings);
        let padding = BAR_COUNT - self.columns.len();
        let bars = (0..BAR_COUNT).map(|i| {
            let level = if i < padding { 0.0 } else { self.columns[i - padding] };
            div()
                .w(px(BAR_WIDTH))
                .h(px((level * MAX_BAR).max(MIN_BAR)))
                .rounded(px(BAR_WIDTH / 2.0))
                .bg(p.accent.opacity(bar_alpha(i, level)))
        });
        div()
            .flex()
            .items_center()
            .size_full()
            .px(px(PILL_PADDING))
            .rounded(px(PILL_HEIGHT / 2.0))
            .bg(p.surface.opacity(0.92))
            .border_1()
            .border_color(p.line)
            .child(bars_row(bars))
    }
}

fn bars_row(bars: impl Iterator<Item = Div>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(BAR_GAP))
        .h(px(MAX_BAR))
        .children(bars)
}
