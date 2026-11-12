//! GPUI prototype for Live Translator: tray + global hotkey + Transcripts screen.

mod hotkey;
mod theme;
mod transcripts;
mod tray;

use futures::StreamExt;
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, IntoElement, ParentElement, QuitMode,
    Render, SharedString, Styled, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowOptions,
};
use gpui_component::{Root, Theme, ThemeMode};
use transcripts::{TranscriptsModel, TranscriptsView};

/// Hide the window without destroying it (minimize to tray).
#[cfg(target_os = "windows")]
mod native {
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const SW_HIDE: i32 = 0;
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;

    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
    }

    fn hwnd(window: &Window) -> Option<isize> {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        }
    }

    pub fn hide(window: &Window) {
        if let Some(hwnd) = hwnd(window) {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    pub fn show(window: &Window) {
        if let Some(hwnd) = hwnd(window) {
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                } else {
                    ShowWindow(hwnd, SW_SHOW);
                }
                SetForegroundWindow(hwnd);
            }
        }
    }
}

/// Non-Windows fallback (untested): minimize / activate.
#[cfg(not(target_os = "windows"))]
mod native {
    use gpui::Window;

    pub fn hide(window: &Window) {
        window.minimize_window();
    }

    pub fn show(window: &Window) {
        window.activate_window();
    }
}

struct AppView {
    transcripts: Entity<TranscriptsView>,
    hotkey_presses: u32,
    hotkey_error: Option<String>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let debug: SharedString = match &self.hotkey_error {
            Some(error) => format!("hotkey Ctrl+Alt+T: {error}").into(),
            None => format!("hotkey Ctrl+Alt+T: pressed {} times", self.hotkey_presses).into(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::background())
            .text_color(theme::text())
            .font_family(theme::FONT_FAMILY)
            .text_size(px(14.0))
            .child(
                div()
                    .id("page")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(theme::MAIN_MAX_WIDTH))
                            .mx_auto()
                            .pt(px(theme::MAIN_PADDING_TOP))
                            .pb(px(theme::MAIN_PADDING_BOTTOM))
                            .px(px(theme::MAIN_PADDING_X))
                            .child(self.transcripts.clone()),
                    ),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(6.0))
                    .text_size(px(10.0))
                    .text_color(theme::muted())
                    .child(debug),
            )
    }
}

fn main() {
    let demo = std::env::args().any(|arg| arg == "--demo");

    gpui_platform::application()
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);

            let model = cx.new(|_| TranscriptsModel::default());
            if demo {
                transcripts::spawn_demo_feeder(model.clone(), cx);
            }

            let hotkey = hotkey::init();
            let hotkey_error = hotkey.error.clone();
            if let Some(error) = &hotkey_error {
                eprintln!("hotkey registration failed: {error}");
            }

            let mut app_view = None;
            let window: WindowHandle<Root> = cx
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
                        // Close button hides the window; the app keeps running in the tray.
                        window.on_window_should_close(cx, |window, _cx| {
                            native::hide(window);
                            false
                        });
                        let transcripts = cx.new(|cx| TranscriptsView::new(model, cx));
                        let view = cx.new(|_| AppView {
                            transcripts,
                            hotkey_presses: 0,
                            hotkey_error,
                        });
                        app_view = Some(view.clone());
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("failed to open window");
            let app_view = app_view.expect("window build closure ran");

            // Tray events -> GPUI foreground task.
            match tray::init() {
                Ok((tray_icon, mut commands)) => {
                    cx.spawn(async move |cx| {
                        let _tray_icon = tray_icon; // keep the tray alive
                        while let Some(command) = commands.next().await {
                            let _ = cx.update(|cx| match command {
                                tray::TrayCommand::Show => {
                                    let _ = window.update(cx, |_, window, _| native::show(window));
                                }
                                tray::TrayCommand::Quit => cx.quit(),
                            });
                        }
                    })
                    .detach();
                }
                Err(error) => eprintln!("tray init failed: {error}"),
            }

            // Hotkey events -> GPUI foreground task.
            let hotkey::Hotkey {
                manager,
                mut presses,
                ..
            } = hotkey;
            cx.spawn(async move |cx| {
                let _manager = manager; // keep the hotkey registered
                while presses.next().await.is_some() {
                    eprintln!("hotkey Ctrl+Alt+T pressed");
                    let _ = app_view.update(cx, |view, cx| {
                        view.hotkey_presses += 1;
                        cx.notify();
                    });
                }
            })
            .detach();
        });
}
