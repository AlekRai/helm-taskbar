use super::*;

pub(super) fn owner_exe(hwnd: isize) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut c_void), Some(&mut pid));
        file_name(&exe_of(pid))
    }
}

pub(super) fn is_overflow(i: &TrayIcon, set: &HashSet<String>) -> bool {
    !i.exe.is_empty() && set.contains(&i.exe)
}

// Icons the bar shows directly; the rest wait behind the chevron.
pub(super) fn shown_icons(t: &[TrayIcon]) -> impl Iterator<Item = &TrayIcon> {
    let set = load_tray_overflow();
    t.iter().filter(move |i| !i.hidden && i.callback != 0 && !is_overflow(i, &set))
}

pub(super) fn overflow_icons(t: &[TrayIcon]) -> impl Iterator<Item = &TrayIcon> {
    let set = load_tray_overflow();
    t.iter().filter(move |i| !i.hidden && i.callback != 0 && is_overflow(i, &set))
}

// Number of bar slots the tray takes, chevron included.
pub(crate) fn visible_count() -> usize {
    TRAY.with(|t| {
        let t = t.borrow();
        shown_icons(&t).count() + usize::from(overflow_icons(&t).next().is_some())
    })
}

pub(crate) fn primary_monitor() -> isize {
    unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY).0 as isize }
}

pub(crate) fn is_primary(monitor: isize) -> bool {
    monitor == primary_monitor()
}

// Device name of the primary monitor's bar, or empty when it has none.
pub(crate) fn primary_device() -> String {
    with_bar(primary_monitor(), |b| b.device.clone()).unwrap_or_default()
}

pub(super) fn to_item(i: &TrayIcon) -> TrayItem {
    TrayItem { icon: i.image.clone(), tip: i.tip.as_str().into() }
}

pub(crate) fn overflow_items() -> Vec<TrayItem> {
    TRAY.with(|t| {
        overflow_icons(&t.borrow()).map(to_item).collect()
    })
}

// Updates a model in place so existing buttons keep their hover state; replaces it only when the count differs.
pub(crate) fn sync_model(current: &ModelRc<TrayItem>, items: Vec<TrayItem>, set: impl FnOnce(ModelRc<TrayItem>)) {
    if current.row_count() == items.len() {
        for (i, it) in items.into_iter().enumerate() {
            if current.row_data(i).is_none_or(|r| r.tip != it.tip || r.icon != it.icon) {
                current.set_row_data(i, it);
            }
        }
    } else {
        set(ModelRc::new(VecModel::from(items)));
    }
}

// Tooltip text for a bar/flyout icon (index among its group) or the chevron (-1).
pub(crate) fn tip_for(index: i32, overflow: bool) -> String {
    if index < 0 {
        return "Show hidden icons".into();
    }
    TRAY.with(|t| {
        let t = t.borrow();
        let icon = if overflow { overflow_icons(&t).nth(index as usize) } else { shown_icons(&t).nth(index as usize) };
        icon.map(|i| if i.tip.trim().is_empty() { fallback_name(&i.exe) } else { i.tip.clone() }).unwrap_or_default()
    })
}

// Apps like Ferdium send no tooltip; name them after their exe.
pub(super) fn fallback_name(exe: &str) -> String {
    let stem = exe.strip_suffix(".exe").unwrap_or(exe);
    let mut c = stem.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

// Fills a bar's tray model, or empties it when that bar doesn't show the tray.
pub(crate) fn sync_bar_tray(ui: &BarWindow, on: bool) {
    let (items, has_overflow) = if on {
        TRAY.with(|t| {
            let t = t.borrow();
            (shown_icons(&t).map(to_item).collect::<Vec<_>>(), overflow_icons(&t).next().is_some())
        })
    } else {
        (Vec::new(), false)
    };
    sync_model(&ui.get_tray(), items, |m| ui.set_tray(m));
    ui.set_has_overflow(has_overflow);
}

// Re-lays out every bar around the current tray icons.
pub(crate) fn push_tray() {
    for m in bar_keys() {
        push_ui(m);
    }
    crate::popup::refresh_tray_flyout();
}

pub(super) type Target = (isize, u32, u32, u32, String, bool);

pub(super) fn nth_target(index: usize, overflow: bool) -> Option<Target> {
    TRAY.with(|t| {
        let t = t.borrow();
        let pick = |i: &TrayIcon| (i.hwnd, i.uid, i.callback, i.version, i.exe.clone(), is_overflow_now(i));
        if overflow { overflow_icons(&t).nth(index).map(pick) } else { shown_icons(&t).nth(index).map(pick) }
    })
}

pub(super) fn is_overflow_now(i: &TrayIcon) -> bool {
    is_overflow(i, &load_tray_overflow())
}

// Every app with a tray icon right now plus those already set to overflow:
// (exe, icon, in overflow), sorted by exe.
pub(crate) fn known_apps() -> Vec<(String, slint::Image, bool)> {
    let over = load_tray_overflow();
    let mut out: Vec<(String, slint::Image, bool)> = Vec::new();
    TRAY.with(|t| {
        for i in t.borrow().iter().filter(|i| !i.hidden && !i.exe.is_empty()) {
            if !out.iter().any(|o| o.0 == i.exe) {
                out.push((i.exe.clone(), i.image.clone(), over.contains(&i.exe)));
            }
        }
    });
    for e in over {
        if !out.iter().any(|o| o.0 == e) {
            out.push((e, slint::Image::default(), true));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

