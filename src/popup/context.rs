use super::*;

pub(crate) fn open_menu(monitor: isize, providers: Vec<Box<dyn MenuProvider>>, info: AppInfo) {
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    let entries = menu_entries(&providers, &info);
    if entries.is_empty() {
        return;
    }
    let fly = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.fly_open, p.fly_hwnd))).filter(|f| f.0);
    TRACK.with(|t| t.keep_fly.set(fly.is_some()));
    close_popups();
    let logical_h = 8.0 + entries.iter().map(|e| if e.separator { 9.0 } else { 32.0 }).sum::<f32>();
    let (w, h) = ((MENU_WIDTH * scale).round() as i32, (logical_h * scale).round() as i32);
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let x = (cursor.x - w / 2).clamp(rect.left + GAP, (rect.right - w - GAP).max(rect.left + GAP));
    let mut top = rect.top;
    if let Some((_, fh)) = fly {
        let mut r = RECT::default();
        if unsafe { GetWindowRect(fh, &mut r) }.is_ok() {
            top = r.top;
        }
    }
    let y = top - h - GAP;

    let Some((ui, hwnd)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.menu.clone_strong(), p.menu_hwnd))) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    ui.set_entries(ModelRc::new(VecModel::from(entries)));
    OPEN_MENU.with(|m| *m.borrow_mut() = Some((providers, info)));
    place(hwnd, x, y, w, h, scale);
    POPUPS.with(|p| p.borrow_mut().as_mut().map(|p| p.menu_open = true));
    start_dismiss();
}

pub(super) fn menu_activated(id: usize) {
    let open = OPEN_MENU.with(|m| m.borrow_mut().take());
    TRACK.with(|t| t.keep_fly.set(true));
    close_popups();
    TRACK.with(|t| t.keep_fly.set(false));
    if let Some((providers, info)) = open {
        run_menu_command(&providers, &info, id);
        for mon in bar_keys() {
            unsafe { refresh(mon) };
        }
    }
}

pub(crate) fn toggle_calendar(monitor: isize) {
    toggle_quick(monitor);
}

