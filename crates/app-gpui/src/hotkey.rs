//! Global hotkey Ctrl+Alt+T. Presses are forwarded into an async channel.

use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

pub struct Hotkey {
    /// Keep alive for the app lifetime; dropping it unregisters the hotkey.
    pub manager: Option<GlobalHotKeyManager>,
    /// Registration error, if any.
    pub error: Option<String>,
    /// One item per `Pressed` event.
    pub presses: UnboundedReceiver<()>,
}

/// Must be called on the main thread after the platform loop exists.
pub fn init() -> Hotkey {
    let (tx, presses) = unbounded();
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT);
    let hotkey_id = hotkey.id();

    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.id == hotkey_id && event.state == HotKeyState::Pressed {
            let _ = tx.unbounded_send(());
        }
    }));

    let result = GlobalHotKeyManager::new()
        .map_err(|e| e.to_string())
        .and_then(|manager| {
            manager
                .register(hotkey)
                .map(|_| manager)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(manager) => Hotkey {
            manager: Some(manager),
            error: None,
            presses,
        },
        Err(error) => Hotkey {
            manager: None,
            error: Some(error),
            presses,
        },
    }
}
