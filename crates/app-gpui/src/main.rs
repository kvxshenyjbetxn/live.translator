//! Live Translator on GPUI.

mod assets;
mod logic;
mod native;
mod services;
mod state;
mod ui;

use gpui::{
    prelude::*, px, size, App, Bounds, QuitMode, TitlebarOptions, WindowBounds, WindowHandle,
    WindowOptions,
};
use gpui_component::{Root, Theme, ThemeMode};
use state::AppState;
use ui::Shell;

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
        });
}
