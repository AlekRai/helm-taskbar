use super::*;

pub(super) fn sorted_icon_only() -> Vec<String> {
    let mut v: Vec<String> = load_icon_only().into_iter().collect();
    v.sort();
    v
}

// Full path of a known exe: pinned, or currently running on any bar.
pub(super) fn exe_path(name: &str) -> Option<String> {
    thread_local! {
        static SEEN: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    }
    let live = STATE
        .with(|s| s.borrow().pinned.iter().find(|p| file_name(&p.1) == name).map(|p| p.1.clone()))
        .or_else(|| {
            bar_keys().into_iter().find_map(|m| {
                with_bar(m, |b| b.entries.iter().find(|e| file_name(&e.exe) == name).map(|e| e.exe.clone())).flatten()
            })
        });
    if let Some(p) = live {
        SEEN.with(|m| m.borrow_mut().insert(name.to_string(), p.clone()));
        return Some(p);
    }
    if let Some(p) = SEEN.with(|m| m.borrow().get(name).cloned()) {
        return Some(p);
    }
    // Not running: look in the usual per-user and system install folders.
    let roots = [
        std::env::var("LOCALAPPDATA").ok().map(|d| std::path::PathBuf::from(d).join("Programs")),
        std::env::var("ProgramFiles").ok().map(std::path::PathBuf::from),
        std::env::var("ProgramFiles(x86)").ok().map(std::path::PathBuf::from),
    ];
    let found = roots.into_iter().flatten().find_map(|root| {
        std::fs::read_dir(root).ok()?.flatten().find_map(|d| {
            let p = d.path().join(name);
            p.is_file().then(|| p.to_string_lossy().into_owned())
        })
    });
    if let Some(p) = &found {
        SEEN.with(|m| m.borrow_mut().insert(name.to_string(), p.clone()));
    }
    found
}

pub(super) fn exe_image(name: &str) -> slint::Image {
    let Some(path) = exe_path(name) else { return slint::Image::default() };
    unsafe { icon_image(exe_icon(&path), 48) }.unwrap_or_default()
}

pub(super) fn file_image(name: &str) -> slint::Image {
    let p = std::path::PathBuf::from(name);
    let p = if p.is_absolute() { p } else { config_dir().unwrap_or_default().join("icons").join(p) };
    let wide = to_wide(&p.to_string_lossy());
    unsafe {
        let Ok(h) = LoadImageW(None, PCWSTR(wide.as_ptr()), IMAGE_ICON, 48, 48, LR_LOADFROMFILE) else {
            return slint::Image::default();
        };
        let icon = HICON(h.0);
        let img = icon_image(icon, 48);
        let _ = DestroyIcon(icon);
        img.unwrap_or_default()
    }
}

pub(super) fn monitor_title(monitor: isize, device: &str) -> String {
    let n: String = device.chars().filter(|c| c.is_ascii_digit()).collect();
    let primary = if is_primary(monitor) { " (primary)" } else { "" };
    format!("Monitor {n}{primary}")
}

// (device name, title, pinned exes) for every bar, ordered by device.
pub(super) fn monitor_pins() -> Vec<(String, String, Vec<String>)> {
    let pins = STATE.with(|s| s.borrow().pinned.clone());
    let mut bars: Vec<(String, String)> = bar_keys()
        .into_iter()
        .filter_map(|m| with_bar(m, |b| b.device.clone()).map(|d| (monitor_title(m, &d), d)))
        .collect();
    bars.sort_by(|a, b| a.1.cmp(&b.1));
    bars.into_iter()
        .map(|(title, d)| {
            let exes = pins.iter().filter(|p| p.0 == d).map(|p| p.1.clone()).collect();
            (d, title, exes)
        })
        .collect()
}

pub(super) fn populate_lists(ui: &SettingsWindow) {
    LAST_LISTS.with(|l| *l.borrow_mut() = lists_snapshot());
    let apps: Vec<GridItem> =
        sorted_icon_only().into_iter().map(|e| GridItem { icon: exe_image(&e), tip: e.into() }).collect();
    ui.set_icon_only(ModelRc::new(VecModel::from(apps)));
    let rules: Vec<RuleRow> = load_icon_rules_raw()
        .into_iter()
        .map(|(e, t, i)| RuleRow {
            app_icon: if e == "*" { slint::Image::default() } else { exe_image(&e.to_lowercase()) },
            icon: file_image(&i),
            exe: e.into(),
            title: t.into(),
            icon_name: i.into(),
        })
        .collect();
    ui.set_rules(ModelRc::new(VecModel::from(rules)));
    let monitors = monitor_pins();
    ui.set_mon_titles(strings(monitors.iter().map(|m| m.1.clone()).collect()));
    if ui.get_mon_sel() as usize >= monitors.len() {
        ui.set_mon_sel(0);
    }
    let primary_dev = primary_device();
    let mons: Vec<MonRow> = monitors
        .into_iter()
        .map(|(d, title, pins)| {
            let c = monitor_cfg(&d);
            let pins: Vec<GridItem> = pins
                .into_iter()
                .map(|p| GridItem { icon: unsafe { icon_image(exe_icon(&p), 48) }.unwrap_or_default(), tip: p.into() })
                .collect();
            MonRow {
                title: title.into(),
                show_labels: c.show_labels,
                show_clock: c.show_clock,
                show_lang: c.lang_on(d == primary_dev),
                show_tray: c.tray_on(d == primary_dev),
                tray_left: c.tray_left,
                show_start: c.show_start,
                pins: ModelRc::new(VecModel::from(pins)),
            }
        })
        .collect();
    ui.set_monitors(ModelRc::new(VecModel::from(mons)));
    let apps: Vec<TrayApp> =
        crate::tray::known_apps().into_iter().map(|(exe, icon, overflow)| TrayApp { exe: exe.into(), icon, overflow }).collect();
    ui.set_tray_apps(ModelRc::new(VecModel::from(apps)));
    ui.set_mixer_hidden(strings(load_ui().mixer_hidden.clone()));
}

pub(super) fn wire_rules(ui: &SettingsWindow) {
    let w = ui.as_weak();
    ui.on_add_icon_only(move |exe| {
        let exe = exe.trim().to_lowercase();
        if exe.is_empty() {
            return;
        }
        let mut set = load_icon_only();
        set.insert(exe);
        save_icon_only(&set);
        changed();
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });

    let w = ui.as_weak();
    ui.on_remove_icon_only(move |i| {
        let list = sorted_icon_only();
        let Some(exe) = list.get(i as usize) else { return };
        let mut set = load_icon_only();
        set.remove(exe);
        save_icon_only(&set);
        changed();
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });

    let w = ui.as_weak();
    ui.on_add_rule(move |exe, title, icon| {
        if exe.trim().is_empty() || icon.trim().is_empty() {
            return;
        }
        let mut rules = load_icon_rules_raw();
        rules.push((exe.trim().into(), title.trim().into(), icon.trim().into()));
        save_icon_rules_raw(rules);
        changed();
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });

    let w = ui.as_weak();
    ui.on_remove_rule(move |i| {
        let mut rules = load_icon_rules_raw();
        if (i as usize) < rules.len() {
            rules.remove(i as usize);
            save_icon_rules_raw(rules);
            changed();
        }
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });
}

pub(super) fn wire_monitors(ui: &SettingsWindow) {
    let w = ui.as_weak();
    ui.on_monitor_labels({
        let w = w.clone();
        move |i, on| edit_monitor(&w, i, |c| c.show_labels = on)
    });
    ui.on_monitor_clock({
        let w = w.clone();
        move |i, on| edit_monitor(&w, i, |c| c.show_clock = on)
    });
    ui.on_monitor_part({
        let w = w.clone();
        move |i, part, on| {
            edit_monitor(&w, i, |c| match part.as_str() {
                "lang" => c.show_lang = Some(on),
                "tray" => c.show_tray = Some(on),
                _ => {}
            })
        }
    });
    ui.on_monitor_side({
        let w = w.clone();
        move |i, left| edit_monitor(&w, i, |c| c.tray_left = left)
    });
    ui.on_monitor_start({
        let w = w.clone();
        move |i, on| edit_monitor(&w, i, |c| c.show_start = on)
    });

    let w = ui.as_weak();
    ui.on_remove_pin(move |mi, pi| {
        let Some((device, _, exes)) = monitor_pins().into_iter().nth(mi as usize) else { return };
        let Some(exe) = exes.get(pi as usize) else { return };
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.pinned.retain(|p| !(p.0 == device && p.1 == *exe));
            save_pins(&s.pinned);
        });
        crate::refresh_all();
        relayout();
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });
}

