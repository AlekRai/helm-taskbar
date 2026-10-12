use super::*;

pub(super) fn buttons_down() -> bool {
    unsafe {
        let down = |vk: VIRTUAL_KEY| GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0;
        down(VK_LBUTTON) || down(VK_RBUTTON) || down(VK_MBUTTON)
    }
}

pub(super) fn start_dismiss() {
    TRACK.with(|t| t.prev_down.set(buttons_down()));
    DISMISS.with(|t| t.start(slint::TimerMode::Repeated, Duration::from_millis(30), dismiss_tick));
}

pub(super) fn open_rects() -> Vec<RECT> {
    POPUPS.with(|p| {
        let p = p.borrow();
        let Some(p) = p.as_ref() else { return Vec::new() };
        let mut v = Vec::new();
        for (open, hwnd) in [
            (p.menu_open, p.menu_hwnd),
            (p.start_open, p.start_hwnd),
            (p.quick_open, p.quick_hwnd),
            (p.fly_open, p.fly_hwnd),
        ] {
            let mut r = RECT::default();
            if open && unsafe { GetWindowRect(hwnd, &mut r) }.is_ok() {
                v.push(r);
            }
        }
        if let Some((l, t, r, b)) = webservices::panel_rect() {
            v.push(RECT { left: l, top: t, right: r, bottom: b });
        }
        v
    })
}

pub(super) fn dismiss_tick() {
    let rects = open_rects();
    if rects.is_empty() {
        DISMISS.with(|t| t.stop());
        return;
    }
    unsafe {
        if GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0 {
            close_popups();
            return;
        }
        let down = buttons_down();
        let edge = down && !TRACK.with(|t| t.prev_down.get());
        TRACK.with(|t| t.prev_down.set(down));
        if edge {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            if !rects.iter().any(|r| PtInRect(r, pt).as_bool()) {
                // Remember which popup this click closed so only it is blocked from instantly reopening.
                let kind = POPUPS.with(|p| {
                    p.borrow().as_ref().map_or(0, |p| {
                        if p.start_open { 1 } else if p.quick_open { 2 } else if p.fly_open { 3 } else { 0 }
                    })
                });
                close_popups();
                TRACK.with(|t| t.closed_at.set(Some((Instant::now(), kind, t.open_mon.get()))));
            }
        }
    }
}

pub(crate) fn close_popups() {
    let was_start = START_OPEN.load(std::sync::atomic::Ordering::SeqCst);
    let keep_fly = TRACK.with(|t| t.keep_fly.replace(false));
    let hwnds = POPUPS.with(|p| {
        let mut p = p.borrow_mut();
        let Some(p) = p.as_mut() else { return Vec::new() };
        let fly = if keep_fly && p.fly_open { HWND::default() } else { p.fly_hwnd };
        let v = vec![p.menu_hwnd, p.start_hwnd, p.quick_hwnd, fly, p.tip_hwnd];
        p.menu_open = false;
        p.quick_open = false;
        p.start_open = false;
        p.fly_open = keep_fly && p.fly_open;
        START_OPEN.store(false, std::sync::atomic::Ordering::SeqCst);
        v
    });
    let start_hwnd = hwnds.get(1).copied();
    if was_start {
        // Hand focus back before parking, while we still own the foreground; otherwise the
        // hidden SearchHost becomes foreground and is very hard to dislodge.
        let prev = HWND(PREV_FG.load(std::sync::atomic::Ordering::SeqCst) as *mut _);
        unsafe {
            let fg = GetForegroundWindow();
            if (fg == start_hwnd.unwrap_or_default() || fg.0.is_null() || fg == search_window())
                && IsWindow(Some(prev)).as_bool()
            {
                let _ = SetForegroundWindow(prev);
            }
        }
    }
    for h in hwnds {
        if !h.0.is_null() {
            unsafe {
                style_overlay(h);
                // Parking offscreen instead of hiding keeps the software renderer's buffer valid.
                let _ = SetWindowPos(
                    h,
                    None,
                    -32000,
                    -32000,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                );
            }
        }
    }
    webservices::hide_panel();
    OPEN_MENU.with(|m| m.borrow_mut().take());
    TIP_TIMER.with(|t| t.stop());
    ENTER.with(|t| t.stop());
    ANIMS.with(|a| a.borrow_mut().clear());
    crate::calendar::stop();
}

