//! System tray icon and menu. Events are forwarded into an async channel so the
//! GPUI foreground task handles them on the UI thread.

use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    Show,
    Quit,
}

const ICON_PNG: &[u8] = include_bytes!("../../../src-tauri/icons/32x32.png");

/// Must be called on the main thread after the platform loop exists.
/// The returned `TrayIcon` must be kept alive for the app lifetime.
pub fn init() -> Result<(TrayIcon, UnboundedReceiver<TrayCommand>), String> {
    let image = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (width, height) = image.dimensions();
    let icon = Icon::from_rgba(image.into_raw(), width, height).map_err(|e| e.to_string())?;

    let show_item = MenuItem::new("Показати", true, None);
    let quit_item = MenuItem::new("Вийти", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show_item, &quit_item])
        .map_err(|e| e.to_string())?;
    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("Live Translator")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())?;

    let (tx, rx) = unbounded();

    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            let _ = menu_tx.unbounded_send(TrayCommand::Show);
        } else if event.id == quit_id {
            let _ = menu_tx.unbounded_send(TrayCommand::Quit);
        }
    }));

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = tx.unbounded_send(TrayCommand::Show);
        }
    }));

    Ok((tray, rx))
}
