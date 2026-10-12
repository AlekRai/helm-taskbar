use super::*;

pub(super) fn fly_size(n: usize, scale: f32) -> (i32, i32, i32) {
    let max_cols = load_ui().tray_cols.clamp(2, 10) as usize;
    let cols = n.clamp(1, max_cols);
    let rows = n.max(1).div_ceil(cols);
    (cols as i32, ((cols as f32 * 38.0 + 10.0) * scale).round() as i32, ((rows as f32 * 38.0 + 10.0) * scale).round() as i32)
}

pub(crate) fn toggle_tray_flyout(monitor: isize) {
    hide_tip();
    if POPUPS.with(|p| p.borrow().as_ref().is_some_and(|p| p.fly_open)) {
        close_popups();
        return;
    }
    let recent = TRACK
        .with(|t| t.closed_at.take())
        .is_some_and(|(t, k, m)| k == 3 && m == monitor && t.elapsed() < Duration::from_millis(400));
    let items = crate::tray::overflow_items();
    if recent || items.is_empty() {
        return;
    }
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    TRACK.with(|t| t.open_mon.set(monitor));
    TRACK.with(|t| t.fly_mon.set(monitor));
    close_popups();
    let Some((ui, hwnd)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.fly.clone_strong(), p.fly_hwnd))) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    let (cols, w, h) = fly_size(items.len(), scale);
    ui.set_cols(cols);
    ui.set_items(ModelRc::new(VecModel::from(items)));
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let x = (cursor.x - w / 2).clamp(rect.left + GAP, (rect.right - w - GAP).max(rect.left + GAP));
    place(hwnd, x, rect.top - h - GAP, w, h, scale);
    POPUPS.with(|p| p.borrow_mut().as_mut().map(|p| p.fly_open = true));
    start_dismiss();
}

// Keeps an open flyout in sync when overflow icons come, go or change.
pub(crate) fn refresh_tray_flyout() {
    let Some((ui, hwnd, open)) =
        POPUPS.with(|p| p.try_borrow().ok().and_then(|p| p.as_ref().map(|p| (p.fly.clone_strong(), p.fly_hwnd, p.fly_open))))
    else {
        return;
    };
    if !open {
        return;
    }
    let items = crate::tray::overflow_items();
    if items.is_empty() {
        close_popups();
        return;
    }
    let scale = with_bar(TRACK.with(|t| t.fly_mon.get()), |b| b.scale).unwrap_or(1.0);
    let (cols, w, h) = fly_size(items.len(), scale);
    ui.set_cols(cols);
    crate::tray::sync_model(&ui.get_items(), items.clone(), |m| ui.set_items(m));
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            let _ = SetWindowPos(hwnd, None, r.left, r.bottom - h, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }
}

