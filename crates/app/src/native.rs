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

    /// Native handle of a window (0 if unavailable).
    pub fn hwnd_of(window: &Window) -> isize {
        hwnd(window).unwrap_or(0)
    }

    /// Never activated, hidden from the taskbar and Alt+Tab, no DWM frame.
    pub fn prepare_overlay(hwnd: isize) {
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_BORDER,
            WS_CAPTION, WS_DLGFRAME, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU,
            WS_THICKFRAME,
        };
        if hwnd == 0 {
            return;
        }
        let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let wanted = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | wanted);
            // GPUI creates the pop-up with WS_CAPTION: its frame shrinks the client area
            // (content clipped) and draws a 1 px top border. Make it a plain frameless popup.
            let frame = (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_BORDER | WS_DLGFRAME) as isize;
            let base = GetWindowLongPtrW(hwnd, GWL_STYLE);
            SetWindowLongPtrW(hwnd, GWL_STYLE, (base & !frame) | WS_POPUP as isize);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let none: u32 = DWMWA_COLOR_NONE;
            DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &none as *const u32 as _, 4);
            let corner: i32 = DWMWCP_ROUNDSMALL;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                &corner as *const i32 as _,
                4,
            );
        }
    }

    /// Places the overlay (logical `width` x `height`) at the bottom centre of the primary
    /// monitor's work area (20 px margin) and shows it without taking focus.
    pub fn show_overlay(hwnd: isize, width: f64, height: f64) {
        use windows_sys::Win32::{
            Foundation::POINT,
            Graphics::Gdi::{
                GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
            },
            UI::{
                HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
                WindowsAndMessaging::{SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SW_SHOWNOACTIVATE},
            },
        };
        if hwnd == 0 {
            return;
        }
        let overlay = hwnd as windows_sys::Win32::Foundation::HWND;
        unsafe {
            let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            if monitor.is_null() {
                return;
            }
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return;
            }
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) != 0 {
                dpi_x = 96;
            }
            let scale = f64::from(dpi_x) / 96.0;
            let w = (width * scale).round() as i32;
            let h = (height * scale).round() as i32;
            let margin = (20.0 * scale).round() as i32;
            let work = info.rcWork;
            let x = work.left + ((work.right - work.left - w).max(0)) / 2;
            let y = work.top + (work.bottom - work.top - h - margin).max(0);
            // Position twice: crossing monitors with another DPI makes the first move resize.
            SetWindowPos(overlay, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
            SetWindowPos(overlay, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
            ShowWindow(overlay, SW_SHOWNOACTIVATE);
        }
    }

    pub fn hide_overlay(hwnd: isize) {
        if hwnd != 0 {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
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

    pub fn hwnd_of(_: &Window) -> isize {
        0
    }

    pub fn prepare_overlay(_: isize) {}

    pub fn show_overlay(_: isize, _: f64, _: f64) {}

    pub fn hide_overlay(_: isize) {}
}

pub use imp::{hide, hwnd_of, hide_overlay, prepare_overlay, show, show_overlay};
