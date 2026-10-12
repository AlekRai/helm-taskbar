use super::*;

pub(crate) fn tray_hovered(monitor: isize, index: i32, on: bool) {
    hover_tip(monitor, index, on, false);
}

pub(crate) fn fly_hovered(index: i32, on: bool) {
    hover_tip(TRACK.with(|t| t.fly_mon.get()), index, on, true);
}

pub(super) fn hover_tip(monitor: isize, index: i32, on: bool, fly: bool) {
    hover_text(monitor, on, fly, move || crate::tray::tip_for(index, fly));
}

pub(crate) fn svc_hovered(monitor: isize, kind: i32, on: bool) {
    hover_text(monitor, on, false, move || crate::services::group_tip(kind as usize));
}

pub(crate) fn svc_row_hovered(index: i32, on: bool) {
    let kind = TRACK.with(|t| t.fly_kind.get());
    hover_text(TRACK.with(|t| t.fly_mon.get()), on, true, move || crate::services::row_tip(kind as usize, index as usize));
}

pub(crate) fn mail_hovered(index: usize, on: bool) {
    let id = TRACK.with(|t| t.mail_id.borrow().clone());
    hover_text(TRACK.with(|t| t.fly_mon.get()), on, true, move || crate::services::mail_tip(&id, index));
}

fn hover_text(monitor: isize, on: bool, fly: bool, text: impl Fn() -> String + 'static) {
    TIP_TIMER.with(|t| t.stop());
    let delay = load_ui().tip_delay_ms;
    if !on {
        hide_tip();
        return;
    }
    if delay == 0 || (!fly && !open_rects().is_empty()) {
        return;
    }
    TIP_TIMER.with(|t| {
        t.start(slint::TimerMode::SingleShot, Duration::from_millis(delay as u64), move || {
            show_tip(monitor, text(), fly)
        })
    });
}

pub(super) fn show_tip(monitor: isize, text: String, fly: bool) {
    if text.trim().is_empty() {
        return;
    }
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    let Some((ui, hwnd, fly_hwnd)) =
        POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.tip.clone_strong(), p.tip_hwnd, p.fly_hwnd)))
    else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    let mut top = rect.top;
    if fly {
        let mut r = RECT::default();
        if unsafe { GetWindowRect(fly_hwnd, &mut r) }.is_err() || r.left < -10000 {
            return;
        }
        top = r.top;
    }
    ui.set_text(text.as_str().into());
    let (w, h) = ((ui.get_want_w() * scale).round() as i32, (ui.get_want_h() * scale).round() as i32);
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let x = (cursor.x - w / 2).clamp(rect.left + GAP, (rect.right - w - GAP).max(rect.left + GAP));
    unsafe { style_overlay(hwnd) };
    animate_in(hwnd, x, top - h - 6, w, h, (4.0 * scale).round() as i32);
}

pub(crate) fn hide_tip() {
    TIP_TIMER.with(|t| t.stop());
    let hwnd = POPUPS.with(|p| p.try_borrow().ok().and_then(|p| p.as_ref().map(|p| p.tip_hwnd)));
    let Some(hwnd) = hwnd else { return };
    if hwnd.0.is_null() {
        return;
    }
    ANIMS.with(|a| a.borrow_mut().retain(|e| e.0 != hwnd));
    unsafe {
        let _ = SetWindowPos(hwnd, None, -32000, -32000, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER);
    }
}

