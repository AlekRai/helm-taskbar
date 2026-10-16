use crate::prelude::*;
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::platform::windows::WindowExtWindows;
use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

thread_local! {
    // Foreground window before we created our windows; their first show may steal focus.
    static PREV_FOREGROUND: Cell<isize> = const { Cell::new(0) };
}

pub(crate) fn native_taskbars() -> Vec<HWND> {
    let mut out = Vec::new();
    unsafe {
        if let Some(h) = explorer_tray() {
            out.push(h);
        }
        let mut prev: Option<HWND> = None;
        while let Ok(h) = FindWindowExW(None, prev, w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) {
            out.push(h);
            prev = Some(h);
        }
    }
    out
}

pub(crate) unsafe extern "system" fn monitor_enum(m: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
    unsafe {
        (*(lparam.0 as *mut Vec<HMONITOR>)).push(m);
    }
    TRUE
}

// One (monitor, bar rect) per monitor. The bar covers that monitor's native taskbar
// if there is one, else the bottom strip of the monitor.
pub(crate) unsafe fn monitor_targets() -> Vec<(isize, RECT)> {
    unsafe {
        let mut monitors: Vec<HMONITOR> = Vec::new();
        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum), LPARAM(&mut monitors as *mut _ as isize));
        let natives = native_taskbars();
        let mut out = Vec::new();
        for m in monitors {
            let mut native_rect = None;
            for n in &natives {
                let mut r = RECT::default();
                if GetWindowRect(*n, &mut r).is_ok()
                    && r.right > r.left
                    && r.bottom > r.top
                    && MonitorFromWindow(*n, MONITOR_DEFAULTTONEAREST) == m
                {
                    native_rect = Some(r);
                }
            }
            let rect = native_rect.unwrap_or_else(|| {
                let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(m, &mut mi);
                RECT { top: mi.rcMonitor.bottom - 48, ..mi.rcMonitor }
            });
            out.push((m.0 as isize, rect));
        }
        out
    }
}

// Logical height of the Windows taskbar on the primary monitor (what "bar height: auto" means).
pub(crate) fn native_bar_height() -> u32 {
    let primary = primary_monitor();
    let scale = with_bar(primary, |b| b.scale).unwrap_or(1.0).max(1.0);
    let rect = unsafe { monitor_targets() }.into_iter().find(|t| t.0 == primary).map(|t| t.1);
    rect.map(|r| ((r.bottom - r.top) as f32 / scale).round() as u32).unwrap_or(48)
}

pub(crate) fn winit_hwnd(window: &slint::Window) -> Option<HWND> {
    window
        .with_winit_window(|w| {
            w.set_skip_taskbar(true);
            match w.window_handle().ok()?.as_raw() {
                RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut c_void)),
                _ => None,
            }
        })
        .flatten()
}

// Makes a Slint window a non-activating, always-on-top tool window.
pub(crate) unsafe fn style_overlay(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let add = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | add) & !(WS_EX_APPWINDOW.0 as isize));
    }
}

// Creates bars for new monitors, repositions existing ones and drops bars whose
// monitor is gone. Does nothing when nothing changed.
pub(crate) unsafe fn sync_bars() {
    unsafe {
        let targets = monitor_targets();
        if targets.is_empty() {
            return;
        }
        let existing = bar_keys();
        for (mon, rect) in &targets {
            if !existing.contains(mon) {
                create_bar(*mon, *rect);
            }
        }
        for mon in existing {
            match targets.iter().find(|t| t.0 == mon) {
                Some((_, rect)) => {
                    let height = load_ui().bar_height;
                    let _ = with_bar(mon, |b| {
                        b.rect = *rect;
                        if !b.hwnd.0.is_null() {
                            b.scale = GetDpiForWindow(b.hwnd).max(96) as f32 / 96.0;
                        }
                        if height > 0 {
                            b.rect.top = b.rect.bottom - (height as f32 * b.scale).round() as i32;
                        }
                    });
                }
                None => {
                    let bar = STATE.with(|s| s.borrow_mut().bars.remove(&mon));
                    if let Some(bar) = bar {
                        let _ = bar.ui.hide();
                    }
                }
            }
        }
        hide_native_taskbars();
    }
}

unsafe fn create_bar(monitor: isize, rect: RECT) {
    unsafe {
        let Ok(ui) = BarWindow::new() else { return };
        crate::theme::apply_bar_theme(&ui);
        let model: Rc<VecModel<AppItem>> = Rc::new(VecModel::default());
        ui.set_apps(ModelRc::from(model.clone()));
        ui.on_app_clicked(move |i| on_app_clicked(monitor, i as usize));
        ui.on_app_right_clicked(move |i| on_app_right_clicked(monitor, i as usize));
        ui.on_app_hovered(move |i, on, cx| crate::popup::preview_hovered(monitor, i as usize, on, cx));
        ui.on_app_reordered(move |i, c| on_app_reordered(monitor, i as usize, c));
        ui.on_app_drag_moved(move |i, c| on_app_drag_moved(monitor, i as usize, c));
        ui.on_clock_clicked(move || on_clock_clicked(monitor));
        ui.on_lang_clicked(move || crate::quick::lang_clicked(monitor));
        ui.on_lang_right_clicked(move || crate::quick::open_language_menu(monitor));
        ui.on_tray_clicked(move |i, b| on_tray_clicked(monitor, i as usize, b));
        ui.on_tray_hovered(move |i, on| crate::popup::tray_hovered(monitor, i, on));
        ui.on_overflow_clicked(move || crate::popup::toggle_tray_flyout(monitor));
        ui.on_svc_clicked(move |i| crate::popup::toggle_services_flyout(monitor, i as usize));
        ui.on_svc_right_clicked(move |k| crate::services::group_right_clicked(monitor, k as usize));
        ui.on_svc_signin_clicked(|k| crate::services::signin_clicked(k as usize));
        ui.on_clock_hovered(move |on| crate::popup::clock_hovered(monitor, on));
        ui.on_meet_clicked(move || crate::popup::toggle_meeting_flyout(monitor));
        ui.on_meet_right_clicked(move || crate::services::meeting_right_clicked(monitor));
        ui.on_strip_drag_moved(move |k, c| crate::services::strip_drag_moved(monitor, k, c));
        ui.on_strip_dropped(move |k, c| crate::services::strip_dropped(monitor, k, c));
        ui.on_svc_hovered(move |k, on| crate::popup::svc_hovered(monitor, k, on));
        ui.on_bar_pressed(|| {
            let _ = SetForegroundWindow(tray_hwnd());
        });
        ui.on_bar_right_clicked(move || on_bar_right_clicked(monitor));

        let device = monitor_device(HMONITOR(monitor as *mut c_void));
        ui.window().set_position(slint::PhysicalPosition::new(rect.left, rect.top));
        ui.window().set_size(slint::PhysicalSize::new(
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        ));
        STATE.with(|s| {
            s.borrow_mut().bars.insert(
                monitor,
                Bar {
                    monitor,
                    device,
                    hwnd: HWND::default(),
                    ui: ui.clone_strong(),
                    entries: Vec::new(),
                    shown: Vec::new(),
                    slots: Vec::new(),
                    drag: None,
                    drag_order: None,
                    model,
                    foreground: HWND::default(),
                    scale: 1.0,
                    rect,
                    minute: 61,
                },
            );
        });
        let fg = GetForegroundWindow();
        if !fg.0.is_null() && !is_ours(fg) {
            PREV_FOREGROUND.with(|p| p.set(fg.0 as isize));
        }
        let _ = ui.show();
    }
}

// The native handle only exists once the event loop has created the window.
pub(crate) unsafe fn finalize_bars() {
    unsafe {
        for mon in bar_keys() {
            let Some((ui, hwnd)) = with_bar(mon, |b| (b.ui.clone_strong(), b.hwnd)) else { continue };
            if !hwnd.0.is_null() {
                continue;
            }
            let Some(hwnd) = winit_hwnd(ui.window()) else { continue };
            style_overlay(hwnd);
            let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let _ = with_bar(mon, |b| {
                b.hwnd = hwnd;
                b.scale = scale;
            });
            position_bar(mon);
            let prev = HWND(PREV_FOREGROUND.with(|p| p.get()) as *mut c_void);
            if GetForegroundWindow() == hwnd && !prev.0.is_null() && IsWindow(Some(prev)).as_bool() {
                let _ = SetForegroundWindow(prev);
            }
        }
    }
}

pub(crate) fn all_finalized() -> bool {
    STATE.with(|s| {
        let s = s.borrow();
        !s.bars.is_empty() && s.bars.values().all(|b| !b.hwnd.0.is_null())
    })
}

pub(crate) unsafe fn quit_all() {
    unsafe {
        close_popups();
        let bars: Vec<Bar> = STATE.with(|s| s.borrow_mut().bars.drain().map(|(_, b)| b).collect());
        for b in bars {
            let _ = b.ui.hide();
        }
        restore_native_taskbars();
        let _ = slint::quit_event_loop();
    }
}

// Re-raise the bars the moment the foreground or z-order changes, instead of waiting
// for the timer, so another topmost window's shadow never shows over a bar.
unsafe extern "system" fn zorder_event(
    _: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    _: HWND,
    id_object: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    unsafe {
        if event == EVENT_OBJECT_REORDER && id_object != OBJID_WINDOW.0 {
            return;
        }
        for mon in bar_keys() {
            position_bar(mon);
            if event == EVENT_SYSTEM_FOREGROUND {
                refresh(mon);
            }
        }
    }
}

pub(crate) unsafe fn install_zorder_hooks() {
    use windows::Win32::UI::Accessibility::SetWinEventHook;
    unsafe {
        for event in [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_REORDER] {
            SetWinEventHook(event, event, None, Some(zorder_event), 0, 0, WINEVENT_OUTOFCONTEXT);
        }
    }
}

// Classic popup menus (tray icon menus included) must stay above the bar.
unsafe fn is_menu(hwnd: HWND) -> bool {
    let mut buf = [0u16; 16];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n]) == "#32768"
}

// True if another topmost window (not one of ours) sits above us in z-order,
// e.g. an always-on-top app whose shadow would then draw over the bar.
unsafe fn covered(hwnd: HWND) -> bool {
    unsafe {
        let mut bar = RECT::default();
        let _ = GetWindowRect(hwnd, &mut bar);
        let mut covers = false;
        let mut h = GetWindow(hwnd, GW_HWNDPREV).ok();
        while let Some(w) = h {
            if IsWindowVisible(w).as_bool() && !is_ours(w) && !is_menu(w) && !is_recent_tray_popup(w) {
                // A window spanning the whole bar is an intentional overlay (screenshot
                // tools, fullscreen apps); yield to it instead of fighting for z-order.
                let mut r = RECT::default();
                let _ = GetWindowRect(w, &mut r);
                if r.left <= bar.left && r.top <= bar.top && r.right >= bar.right && r.bottom >= bar.bottom {
                    return false;
                }
                if r.left < bar.right && r.right > bar.left && r.top < bar.bottom && r.bottom > bar.top {
                    covers = true;
                }
            }
            h = GetWindow(w, GW_HWNDPREV).ok();
        }
        covers
    }
}

thread_local! {
    static FS_LAST: std::cell::RefCell<std::collections::HashMap<isize, bool>> = std::cell::RefCell::new(Default::default());
}

// Logs each change of a bar's fullscreen state with the window that decided it.
fn log_fullscreen(bar: HWND, full: bool, class: &str, r: &RECT) {
    if FS_LAST.with(|f| f.borrow_mut().insert(bar.0 as isize, full)) != Some(full) {
        crate::util::log(&format!("fullscreen {full} on bar {:?}: {class} {},{} {},{}", bar.0, r.left, r.top, r.right, r.bottom));
    }
}

// The topmost large window on the bar's monitor covering the whole monitor (fullscreen video, games) means the bar must
// get out of the way. Using z-order rather than the foreground window keeps it hidden while another monitor has focus.
unsafe fn fullscreen_on(bar: HWND) -> bool {
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    unsafe {
        let mon = MonitorFromWindow(bar, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        let mon_area = (m.right - m.left) as i64 * (m.bottom - m.top) as i64;
        let mut h = GetTopWindow(None).ok();
        while let Some(w) = h {
            h = GetWindow(w, GW_HWNDNEXT).ok();
            if !IsWindowVisible(w).as_bool() || IsIconic(w).as_bool() || is_ours(w) {
                continue;
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(w, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
            if cloaked != 0 {
                continue;
            }
            let mut buf = [0u16; 32];
            let n = GetClassNameW(w, &mut buf) as usize;
            let class = String::from_utf16_lossy(&buf[..n]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
                continue;
            }
            if MonitorFromWindow(w, MONITOR_DEFAULTTONEAREST) != mon {
                continue;
            }
            let mut r = RECT::default();
            if GetWindowRect(w, &mut r).is_err() {
                continue;
            }
            let area = (r.right - r.left).max(0) as i64 * (r.bottom - r.top).max(0) as i64;
            if area * 2 < mon_area {
                continue;
            }
            let slack = 2;
            let full = r.left <= m.left + slack && r.top <= m.top + slack && r.right >= m.right - slack && r.bottom >= m.bottom - slack;
            log_fullscreen(bar, full, &class, &r);
            return full;
        }
        log_fullscreen(bar, false, "-", &RECT::default());
        false
    }
}
// Not an appbar: registering one changes the work area, which makes Explorer
// shuffle desktop icons onto the other monitor. Instead we sit exactly where the
// native taskbars were, and hide those.
pub(crate) unsafe fn position_bar(monitor: isize) {
    unsafe {
        let Some((hwnd, r)) = with_bar(monitor, |b| (b.hwnd, b.rect)) else { return };
        if hwnd.0.is_null() {
            return;
        }
        let visible = IsWindowVisible(hwnd).as_bool();
        // The bar stays up while the Start menu is open, even over a fullscreen window.
        if fullscreen_on(hwnd) && !crate::popup::start_menu_open() {
            // Sinking below the fullscreen window (rather than hiding) keeps the bar's rendering intact.
            if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 {
                let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            return;
        }
        let mut cur = RECT::default();
        let _ = GetWindowRect(hwnd, &mut cur);
        let topmost = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0;
        if visible && topmost && cur == r && !covered(hwnd) {
            return;
        }
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

pub(crate) unsafe fn hide_native_taskbars() {
    unsafe {
        for native in native_taskbars() {
            if IsWindowVisible(native).as_bool() {
                let _ = ShowWindow(native, SW_HIDE);
            }
        }
    }
}

pub(crate) unsafe fn restore_native_taskbars() {
    unsafe {
        for native in native_taskbars() {
            let _ = ShowWindow(native, SW_SHOWNA);
        }
        crate::start::resume_search_host();
    }
}
