#![windows_subsystem = "windows"]
//! Custom Windows taskbar. Slint draws the bar and popups; Win32 calls do the shell work.
//!
//! Everything runs on the single UI thread. Global state lives in `thread_local!` cells
//! (`STATE`, `TRAY`, the config store, the popup windows). Never hold a `borrow_mut()` across a
//! call that can re-enter (Slint callbacks, `push_ui`, window creation), and use
//! `try_with_popup_uis` where popups may already be borrowed.

mod actions;
mod autostart;
mod calendar;
mod config;
mod icons;
mod menu;
mod popup;
mod prelude;
mod quick;
mod services;
mod shell;
mod state;
mod settings;
mod start;
mod tasks;
mod theme;
mod tray;
mod ui {
    slint::include_modules!();
}
mod util;

use prelude::*;

fn refresh_all() {
    for mon in bar_keys() {
        unsafe {
            position_bar(mon);
            refresh(mon);
        }
    }
}

fn tick() {
    sync_clock_timer();
    if reload_if_changed() {
        apply_to_state();
        crate::theme::apply_theme_all();
        STATE.with(|s| s.borrow_mut().file_icons.clear());
    }
    unsafe {
        hide_native_taskbars();
        sync_bars();
        finalize_bars();
        finalize_popups();
    }
    raise_tray();
    if prune_dead() {
        push_tray();
    }
    forget_flashing_dead();
    crate::popup::refresh_services_flyout();
    crate::settings::sync_open_window();
    refresh_all();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--watchdog") {
        if let Some(pid) = args.get(2).and_then(|p| p.parse().ok()) {
            autostart::run_watchdog(pid);
        }
        return;
    }
    if !autostart::acquire_single_instance() {
        return;
    }
    autostart::init();
    let mut watchdog = autostart::spawn_watchdog();
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        apply_to_state();
        init_popups();
        sync_bars();
        install_zorder_hooks();
    }
    init_tray();
    services::start();
    start::install_win_key_hook();
    start::prewarm();
    // Native handles only exist once the event loop runs, so finish setup quickly there.
    let setup = std::rc::Rc::new(slint::Timer::default());
    let setup2 = setup.clone();
    setup.start(slint::TimerMode::Repeated, Duration::from_millis(30), move || {
        unsafe {
            finalize_bars();
            finalize_popups();
        }
        refresh_all();
        if all_finalized() && popups_ready() {
            setup2.stop();
        }
    });

    let refresh_timer = slint::Timer::default();
    refresh_timer.start(slint::TimerMode::Repeated, Duration::from_millis(500), tick);
    sync_clock_timer();

    let _ = slint::run_event_loop_until_quit();
    shutdown_tray();
    unsafe { restore_native_taskbars() };
    if let Some(w) = watchdog.as_mut() {
        let _ = w.kill();
    }
}
