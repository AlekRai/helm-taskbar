use super::*;

pub(super) fn set_pinned(ui: &StartPopup) {
    let pins = load_start_pins();
    let items: Vec<StartItem> = INDEX.with(|i| {
        let mut i = i.borrow_mut();
        let idxs: Vec<usize> = pins.iter().filter_map(|p| i.apps.iter().position(|a| a.id == *p)).collect();
        idxs.into_iter().map(|idx| item_for(&mut i, idx, &pins)).collect()
    });
    ui.set_pinned(ModelRc::new(VecModel::from(items)));
}

pub(super) fn set_results(ui: &StartPopup) {
    let q = ui.get_query().trim().to_lowercase();
    if q.is_empty() {
        ui.set_results(ModelRc::default());
        return;
    }
    let pins = load_start_pins();
    let items: Vec<StartItem> = INDEX.with(|i| {
        let mut i = i.borrow_mut();
        let mut hits: Vec<(i32, usize)> =
            i.apps.iter().enumerate().filter_map(|(n, a)| score(&a.name_lc, &q).map(|s| (if pins.contains(&a.id) { s + 2000 } else { s }, n))).collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(i.apps[a.1].name_lc.cmp(&i.apps[b.1].name_lc)));
        hits.truncate(MAX_RESULTS);
        hits.into_iter().map(|(_, n)| item_for(&mut i, n, &pins)).collect()
    });
    ui.set_results(ModelRc::new(VecModel::from(items)));
}

pub(super) fn launch(id: &str) {
    launch_verb(id, w!("open"));
}

pub(super) fn launch_admin(id: &str) {
    launch_verb(id, w!("runas"));
}

fn launch_verb(id: &str, verb: PCWSTR) {
    // Desktop apps may be plain paths; everything else is an AppsFolder entry (AUMID, GUID forms).
    let target = if id.get(1..3) == Some(":\\") || id.starts_with("\\\\") {
        id.to_string()
    } else {
        format!("shell:AppsFolder\\{id}")
    };
    let wide = to_wide(&target);
    unsafe {
        ShellExecuteW(None, verb, PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

pub(super) fn power(action: i32) {
    use std::os::windows::process::CommandExt;
    let (exe, args): (&str, &[&str]) = match action {
        0 => ("rundll32.exe", &["user32.dll,LockWorkStation"]),
        1 => ("shutdown.exe", &["/l"]),
        2 => ("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"]),
        3 => ("shutdown.exe", &["/r", "/t", "0"]),
        _ => ("shutdown.exe", &["/s", "/t", "0"]),
    };
    let _ = std::process::Command::new(exe).args(args).creation_flags(0x0800_0000).spawn();
}

pub(crate) fn wire(ui: &StartPopup) {
    let weak = ui.as_weak();
    ui.on_query_changed({
        let weak = weak.clone();
        move |_| {
            if let Some(ui) = weak.upgrade() {
                set_results(&ui);
            }
        }
    });
    ui.on_launch(|id| {
        launch(&id);
        close_popups();
    });
    ui.on_launch_admin(|id| {
        launch_admin(&id);
        close_popups();
    });
    ui.on_toggle_pin({
        let weak = weak.clone();
        move |id| {
            let mut pins = load_start_pins();
            let id = id.to_string();
            match pins.iter().position(|p| *p == id) {
                Some(pos) => {
                    pins.remove(pos);
                }
                None => pins.push(id),
            }
            save_start_pins(pins);
            if let Some(ui) = weak.upgrade() {
                set_pinned(&ui);
                set_results(&ui);
            }
        }
    });
    ui.on_power(|a| {
        close_popups();
        power(a);
    });
}

// Prepares the launcher for a fresh open: empty search, current pins.
pub(crate) fn reset(ui: &StartPopup) {
    ensure_index();
    ui.set_query("".into());
    ui.set_sel(0);
    set_pinned(ui);
    set_results(ui);
}

// Builds the app index shortly after startup so the first open is instant.
pub(crate) fn prewarm() {
    slint::Timer::single_shot(Duration::from_secs(3), || {
        ensure_index();
    });
}

