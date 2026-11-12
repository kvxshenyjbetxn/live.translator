//! Hide / show the main window without destroying it (minimize to tray).

#[cfg(target_os = "windows")]
mod imp {
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
mod imp {
    use gpui::Window;

    pub fn hide(window: &Window) {
        window.minimize_window();
    }

    pub fn show(window: &Window) {
        window.activate_window();
    }
}

pub use imp::{hide, show};
