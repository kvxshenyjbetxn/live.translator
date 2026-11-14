//! Live Translator on GPUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod logic;
mod native;
mod services;
mod state;
mod ui;

use gpui::{
    point, prelude::*, px, size, App, Bounds, QuitMode, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use gpui_component::{Root, Theme, ThemeMode};
use state::AppState;
use ui::{Equalizer, Shell, PILL_HEIGHT, PILL_WIDTH};

fn main() {
    gpui_platform::application()
        .with_assets(assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);

            let state = cx.new(AppState::new);
            let close_state = state.read(cx).close_state();

            let _window: WindowHandle<Root> = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(980.0), px(780.0)),
                            cx,
                        ))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Live Translator".into()),
                            ..Default::default()
                        }),
                        window_min_size: Some(size(px(780.0), px(660.0))),
                        ..Default::default()
                    },
                    |window, cx| {
                        let close_target = state.clone();
                        window.on_window_should_close(cx, move |window, cx| {
                            // With a tray available, the close button only hides the window;
                            // "Exit" lives in the tray menu. Without one, run the close flow.
                            if close_target.read(cx).has_tray() {
                                native::hide(window);
                                false
                            } else if close_state.should_intercept() {
                                close_target.update(cx, |s, cx| s.request_close(false, cx));
                                false
                            } else {
                                true
                            }
                        });
                        let view = cx.new(|cx| Shell::new(state.clone(), window, cx));
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("failed to open window");

            // Overlay: borderless transparent pop-up with the equalizer pill, always on top,
            // never activated. Created hidden; the Equalizer shows/hides it from AppState.
            let mut overlay_hwnd = 0;
            let overlay = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.0), px(0.0)),
                        size: size(px(PILL_WIDTH), px(PILL_HEIGHT)),
                    })),
                    titlebar: None,
                    focus: false,
                    show: false,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |window, cx| {
                    overlay_hwnd = native::hwnd_of(window);
                    cx.new(|cx| Equalizer::new(state.clone(), overlay_hwnd, cx))
                },
            );
            match overlay {
                Ok(_) => native::prepare_overlay(overlay_hwnd),
                Err(error) => eprintln!("Failed to create the overlay window: {error}"),
            }
        });
}
