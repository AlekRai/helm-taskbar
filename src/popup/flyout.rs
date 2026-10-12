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
        .is_some_and(|(t, k, m)| k == 3 && m == monitor && t.elapsed() < Duration::from_millis(400))
        && TRACK.with(|t| t.fly_kind.get()) == TRAY_KIND;
    let items = crate::tray::overflow_items();
    if recent || items.is_empty() {
        return;
    }
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    TRACK.with(|t| t.open_mon.set(monitor));
    TRACK.with(|t| t.fly_mon.set(monitor));
    TRACK.with(|t| t.fly_kind.set(TRAY_KIND));
    close_popups();
    let Some((ui, hwnd)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.fly.clone_strong(), p.fly_hwnd))) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    ui.set_svcs(ModelRc::default());
    let (cols, w, h) = fly_size(items.len(), scale);
    ui.set_cols(cols);
    ui.set_items(ModelRc::new(VecModel::from(items)));
    show_fly(hwnd, rect, w, h, scale);
}

fn show_fly(hwnd: HWND, rect: RECT, w: i32, h: i32, scale: f32) {
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let x = (cursor.x - w / 2).clamp(rect.left + GAP, (rect.right - w - GAP).max(rect.left + GAP));
    place(hwnd, x, rect.top - h - GAP, w, h, scale);
    POPUPS.with(|p| p.borrow_mut().as_mut().map(|p| p.fly_open = true));
    start_dismiss();
}

const SVC_FLY_WIDTH: f32 = 220.0;

fn svc_fly_size(n: usize, scale: f32) -> (i32, i32) {
    ((SVC_FLY_WIDTH * scale).round() as i32, ((n as f32 * 34.0 + 10.0) * scale).round() as i32)
}

// Opens the flyout listing the services of one bar group (0 personal, 1 work).
pub(crate) fn toggle_services_flyout(monitor: isize, kind: usize) {
    hide_tip();
    if POPUPS.with(|p| p.borrow().as_ref().is_some_and(|p| p.fly_open)) {
        close_popups();
        return;
    }
    let recent = TRACK
        .with(|t| t.closed_at.take())
        .is_some_and(|(t, k, m)| k == 3 && m == monitor && t.elapsed() < Duration::from_millis(400))
        && TRACK.with(|t| t.fly_kind.get()) == kind as i32;
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    let rows = crate::services::members(kind, scale);
    if recent || rows.is_empty() {
        return;
    }
    TRACK.with(|t| {
        t.open_mon.set(monitor);
        t.fly_mon.set(monitor);
        t.fly_kind.set(kind as i32);
    });
    close_popups();
    let Some((ui, hwnd)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.fly.clone_strong(), p.fly_hwnd))) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    ui.set_items(ModelRc::default());
    ui.set_svcs(ModelRc::new(VecModel::from(rows.into_iter().map(|r| r.0).collect::<Vec<_>>())));
    let (w, h) = svc_fly_size(ui.get_svcs().row_count(), scale);
    show_fly(hwnd, rect, w, h, scale);
}

// Opens the n-th service of the open services flyout and closes it.
pub(crate) fn svc_row_clicked(index: usize) {
    let kind = TRACK.with(|t| t.fly_kind.get());
    if kind < 0 {
        return;
    }
    let id = crate::services::members(kind as usize, 1.0).get(index).map(|r| r.1.clone());
    close_popups();
    if let Some(id) = id {
        crate::services::open(&id);
    }
}

// Keeps an open services flyout's counts and flash in sync.
pub(crate) fn refresh_services_flyout() {
    let open = POPUPS.with(|p| p.try_borrow().ok().is_some_and(|p| p.as_ref().is_some_and(|p| p.fly_open)));
    let kind = TRACK.with(|t| t.fly_kind.get());
    if !open || kind < 0 {
        return;
    }
    refresh_group(kind as usize);
}

fn refresh_group(kind: usize) {
    let Some(ui) = POPUPS.with(|p| p.try_borrow().ok().and_then(|p| p.as_ref().map(|p| p.fly.clone_strong()))) else {
        return;
    };
    let scale = with_bar(TRACK.with(|t| t.fly_mon.get()), |b| b.scale).unwrap_or(1.0);
    let rows: Vec<SvcItem> = crate::services::members(kind, scale).into_iter().map(|r| r.0).collect();
    let current = ui.get_svcs();
    if current.row_count() != rows.len() {
        close_popups();
        return;
    }
    for (i, r) in rows.into_iter().enumerate() {
        if current.row_data(i).is_none_or(|c| c != r) {
            current.set_row_data(i, r);
        }
    }
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
    let kind = TRACK.with(|t| t.fly_kind.get());
    if kind != TRAY_KIND {
        refresh_group(kind as usize);
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

