use crate::prelude::*;

// ---------- click handlers (called from the Slint bars) ----------

pub(crate) const START_EXE: &str = "::start";

pub(crate) unsafe fn activate(monitor: isize, entry: &Entry, foreground: HWND) {
    unsafe {
        if entry.exe == START_EXE {
            toggle_start(monitor);
            return;
        }
        if entry.hwnd.0.is_null() {
            let wide = to_wide(&entry.exe);
            ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
            return;
        }
        let target = entry.hwnd;
        if IsIconic(target).as_bool() {
            let _ = ShowWindow(target, SW_RESTORE);
            let _ = SetForegroundWindow(target);
        } else if target == foreground {
            let _ = ShowWindow(target, SW_MINIMIZE);
        } else {
            let _ = SetForegroundWindow(target);
        }
        refresh(monitor);
    }
}

pub(crate) fn on_app_clicked(monitor: isize, index: usize) {
    crate::popup::hide_preview();
    let target = with_bar(monitor, |b| {
        b.shown.get(index).filter(|s| !s.closing).map(|s| (s.entry.clone(), b.foreground))
    })
    .flatten();
    if target.as_ref().is_none_or(|(e, _)| e.exe != START_EXE) {
        close_popups();
    }
    if let Some((entry, fg)) = target {
        unsafe { activate(monitor, &entry, fg) };
    }
}

// Previews the order while a button is dragged; `center` is its middle x in the button area.
pub(crate) fn on_app_drag_moved(monitor: isize, index: usize, center: f32) {
    unsafe {
        if let Ok(c) = LoadCursorW(None, IDC_ARROW) {
            SetCursor(Some(c));
        }
    }
    let changed = with_bar(monitor, |b| {
        let key = b.shown.get(index).filter(|s| !s.closing)?.key.clone();
        if b.drag.is_none() {
            let x = b.model.row_data(index)?.x;
            b.drag = Some((key.clone(), x));
            // Capture keeps our arrow cursor when the pointer leaves the bar mid-drag.
            unsafe {
                let _ = SetCapture(b.hwnd);
            }
        }
        let (target_key, _) = b
            .slots
            .iter()
            .min_by(|a, c| (a.1 - center).abs().total_cmp(&(c.1 - center).abs()))?
            .clone();
        let mut list: Vec<String> = match &b.drag_order {
            Some(cur) => cur.clone(),
            None => b.entries.iter().map(entry_key).collect(),
        };
        let f = list.iter().position(|e| *e == key)?;
        let t = list.iter().position(|e| *e == target_key)?;
        if f == t {
            return None;
        }
        let e = list.remove(f);
        list.insert(t, e);
        b.drag_order = Some(list);
        Some(())
    })
    .flatten();
    if changed.is_some() {
        push_ui(monitor);
    }
}

// Called when a dragged button is dropped: commits the previewed order.
pub(crate) fn on_app_reordered(monitor: isize, index: usize, center: f32) {
    on_app_drag_moved(monitor, index, center);
    unsafe {
        let _ = ReleaseCapture();
    }
    let done = with_bar(monitor, |b| {
        b.drag = None;
        let keys = b.drag_order.take()?;
        let ordered: Vec<&Entry> =
            keys.iter().filter_map(|k| b.entries.iter().find(|e| entry_key(e) == *k)).collect();
        let hwnds: Vec<isize> =
            ordered.iter().filter(|e| !e.hwnd.0.is_null()).map(|e| e.hwnd.0 as isize).collect();
        let exes: Vec<String> = ordered.iter().map(|e| e.exe.to_lowercase()).collect();
        Some((b.device.clone(), exes, hwnds))
    })
    .flatten();
    if let Some((device, list, hwnds)) = done {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            for (rank, h) in hwnds.into_iter().enumerate() {
                s.win_rank.insert(h, rank);
            }
            s.layout.retain(|(d, _)| *d != device);
            s.layout.extend(list.into_iter().map(|e| (device.clone(), e)));
            save_order(&s.layout);
        });
        let recs: Vec<WinRec> = STATE.with(|s| {
            let s = s.borrow();
            let mut v = Vec::new();
            for b in s.bars.values() {
                for e in b.entries.iter().filter(|e| !e.hwnd.0.is_null()) {
                    if let Some(&rank) = s.win_rank.get(&(e.hwnd.0 as isize)) {
                        v.push(WinRec { exe: e.exe.clone(), title: e.title.clone(), hwnd: e.hwnd.0 as isize, pid: e.pid, rank });
                    }
                }
            }
            v
        });
        STATE.with(|s| s.borrow_mut().saved_windows = recs.clone());
        save_windows(recs);
    }
    unsafe { refresh(monitor) };
    push_ui(monitor);
}

pub(crate) fn on_app_right_clicked(monitor: isize, index: usize) {
    crate::popup::hide_preview();
    let target = with_bar(monitor, |b| {
        b.shown.get(index).filter(|s| !s.closing).map(|s| (s.entry.clone(), b.device.clone()))
    })
    .flatten();
    if let Some((entry, device)) = target {
        open_menu(monitor, app_providers(), app_info(&entry, &device));
    }
}

pub(crate) fn on_bar_right_clicked(monitor: isize) {
    let device = with_bar(monitor, |b| b.device.clone()).unwrap_or_default();
    open_menu(monitor, bar_providers(), AppInfo { device, ..Default::default() });
}

pub(crate) fn on_clock_clicked(monitor: isize) {
    if is_primary(monitor) {
        toggle_quick(monitor);
    } else {
        toggle_calendar(monitor);
    }
}
