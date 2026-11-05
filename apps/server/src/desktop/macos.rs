//! macOS: AppKit only works on the main thread, so the server runs on a
//! helper thread while the main thread shows the menu bar item. The process
//! exits once the server has shut down.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use super::{Command, TrayContext, autostart, item};
use crate::Config;

pub fn serve(runtime: tokio::runtime::Runtime, config: Config) -> anyhow::Result<()> {
    let (ctx_tx, ctx_rx) = mpsc::channel();
    let server = std::thread::Builder::new().name("pley-server".into()).spawn(move || {
        let launcher: super::TrayLauncher = Box::new(move |ctx| {
            ctx.open_setup_if_new();
            let _ = ctx_tx.send(ctx);
            None
        });
        crate::run_on(runtime, config, Some(launcher))
    })?;
    let Ok(ctx) = ctx_rx.recv() else {
        // The server stopped before listening (e.g. the port is taken).
        return server.join().unwrap_or_else(|_| Err(anyhow::anyhow!("the server thread panicked")));
    };
    std::thread::spawn(move || {
        let code = match server.join() {
            Ok(Ok(())) => 0,
            Ok(Err(err)) => {
                tracing::error!("{err:#}");
                1
            }
            Err(_) => 1,
        };
        std::process::exit(code);
    });
    run_menu_bar(ctx)
}

fn run_menu_bar(ctx: TrayContext) -> ! {
    let mtm = MainThreadMarker::new().expect("AppKit runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // A menu bar item only: no Dock icon, no app menu.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let _tray = match build(&ctx) {
        Ok(tray) => Some(tray),
        Err(err) => {
            tracing::warn!("no menu bar icon: {err}");
            None
        }
    };
    app.run();
    // `run` only returns when AppKit is told to stop; the server thread owns
    // the process lifetime, so just wait for it.
    loop {
        std::thread::park();
    }
}

fn build(ctx: &TrayContext) -> Result<TrayIcon, String> {
    let autostart_on = autostart::is_enabled();
    let menu = Menu::new();
    menu.append_items(&[
        // macOS's menus are in title case in English: `tray.mac` has them so.
        &MenuItem::with_id(item::OPEN, ctx.plain_label("open"), true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(item::SCAN, ctx.plain_label("mac.scan"), true, None),
        &MenuItem::with_id(item::LOGS, ctx.plain_label("mac.logs"), true, None),
        &PredefinedMenuItem::separator(),
        &CheckMenuItem::with_id(item::AUTOSTART, ctx.plain_label("mac.autostart"), true, autostart_on, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(item::QUIT, ctx.plain_label("quit"), true, None),
    ])
    .map_err(|e| e.to_string())?;

    // Menu items are main-thread objects; the handler only needs the context
    // and a copy of the autostart state (AppKit flips the checkmark itself).
    let handler_ctx = ctx.clone();
    let autostart_state = Arc::new(AtomicBool::new(autostart_on));
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
        item::OPEN => handler_ctx.open_web_ui(),
        item::SCAN => handler_ctx.send(Command::ScanLibraries),
        item::LOGS => handler_ctx.open_logs(),
        item::AUTOSTART => {
            let wanted = !autostart_state.load(Ordering::Relaxed);
            autostart_state.store(handler_ctx.toggle_autostart(wanted), Ordering::Relaxed);
        }
        item::QUIT => handler_ctx.send(Command::Quit),
        _ => {}
    }));

    let (pixels, width, height) =
        super::rgba(include_bytes!("../../assets/tray-template.png")).ok_or("bad embedded icon")?;
    let icon = Icon::from_rgba(pixels, width, height).map_err(|e| e.to_string())?;
    TrayIconBuilder::new()
        .with_id("pley")
        .with_icon_templated(icon)
        .with_tooltip(ctx.tooltip())
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| e.to_string())
}
