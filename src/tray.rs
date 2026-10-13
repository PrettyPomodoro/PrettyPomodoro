//! System tray icon and native window hide/show.
//!
//! The window is hidden and shown through the platform API rather than egui's
//! viewport commands: a hidden window receives no redraw events, so egui would
//! never get to process a "show" command.

use crate::core::{lock, Shared};
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Clone)]
pub struct NativeWindow {
    handle: usize,
}

impl NativeWindow {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        let handle = match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as usize,
            RawWindowHandle::Win32(h) => h.hwnd.get() as usize,
            _ => return None,
        };
        Some(Self { handle })
    }

    /// Must be called on the main thread.
    pub fn hide(&self) {
        platform::set_visible(self.handle, false);
    }

    /// Must be called on the main thread.
    pub fn show(&self) {
        platform::set_visible(self.handle, true);
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    pub fn set_visible(ns_view: usize, visible: bool) {
        // SAFETY: `ns_view` is the live NSView of the app's only window, and
        // this runs on the main thread (egui update or a tray menu callback).
        unsafe {
            let view = ns_view as *mut AnyObject;
            let window: *mut AnyObject = msg_send![view, window];
            if window.is_null() {
                return;
            }
            let nil = std::ptr::null::<AnyObject>();
            if visible {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: true];
                let _: () = msg_send![window, makeKeyAndOrderFront: nil];
            } else {
                let _: () = msg_send![window, orderOut: nil];
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_HIDE, SW_SHOW,
    };

    pub fn set_visible(hwnd: usize, visible: bool) {
        // SAFETY: `hwnd` is the handle of the app's own top-level window.
        unsafe {
            let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
            if visible {
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
            } else {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn set_visible(_handle: usize, _visible: bool) {}
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use super::*;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    /// Keeps the tray icon alive for as long as it is held.
    pub struct Tray {
        _icon: TrayIcon,
    }

    pub fn create(shared: Shared, window: NativeWindow, ctx: egui::Context) -> Option<Tray> {
        let show = MenuItem::new("Show PrettyPomodoro", true, None);
        let toggle = MenuItem::new("Start / Pause", true, None);
        let skip = MenuItem::new("Skip Round", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &show,
            &PredefinedMenuItem::separator(),
            &toggle,
            &skip,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .ok()?;

        let (show_id, toggle_id, skip_id, quit_id) = (
            show.id().clone(),
            toggle.id().clone(),
            skip.id().clone(),
            quit.id().clone(),
        );
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let now = std::time::Instant::now();
            if event.id == show_id {
                window.show();
            } else if event.id == toggle_id {
                lock(&shared).toggle(now);
            } else if event.id == skip_id {
                lock(&shared).skip(now);
            } else if event.id == quit_id {
                let core = lock(&shared);
                crate::config::save_config(&core.cfg);
                crate::config::save_history(&core.history);
                std::process::exit(0);
            }
            ctx.request_repaint();
        }));

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("PrettyPomodoro")
            .with_icon(icon()?)
            // macOS draws a template in the menu bar's own text colour: white
            // on a dark bar, dark on a light one.
            .with_icon_as_template(true)
            .build()
            .ok()?;
        Some(Tray { _icon: icon })
    }

    /// The app icon's tomato on its own, in white.
    fn icon() -> Option<Icon> {
        let (rgba, side) = crate::icon::tray_image();
        Icon::from_rgba(rgba, side, side).ok()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::*;

    pub struct Tray;

    /// No tray on this platform; "minimize to tray" falls back to minimizing.
    pub fn create(_: Shared, _: NativeWindow, _: egui::Context) -> Option<Tray> {
        None
    }
}

pub use imp::{create, Tray};
