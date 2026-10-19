use super::*;

fn group_index(group: &str) -> i32 {
    (group == "work") as i32
}

fn sched_index(group: &str) -> i32 {
    match group {
        "work" => 1,
        "personal" => 2,
        _ => 0,
    }
}

pub(super) fn populate_svc(ui: &SettingsWindow) {
    let icons: std::collections::HashMap<String, isize> = webservices::status().into_iter().map(|s| (s.id, s.icon)).collect();
    let rows: Vec<SvcRow> = load_services()
        .into_iter()
        .map(|c| {
            let h = icons.get(&c.id).copied().unwrap_or(0);
            let cached = (h == 0).then(|| slint::Image::load_from_path(&webservices::cached_icon_path(&c.id)).ok()).flatten();
            SvcRow {
                icon: if h != 0 { cached_icon_image(HICON(h as *mut c_void), 1.0) } else { cached.clone().unwrap_or_default() },
                has_icon: h != 0 || cached.is_some(),
                letter: c.name.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_default().into(),
            id: c.id.into(),
            name: c.name.into(),
            url: c.url.into(),
            group: group_index(&c.group),
            enabled: c.enabled,
            flash: c.flash,
                adapter: c.adapter.into(),
            }
        })
        .collect();
    let mut rows = rows;
    let o = load_svc();
    let icon = unsafe { exe_icon(r"C:\Windows\System32\mstsc.exe") };
    rows.push(SvcRow {
        icon: if icon.0.is_null() { Default::default() } else { cached_icon_image(icon, 1.0) },
        has_icon: !icon.0.is_null(),
        letter: "R".into(),
        id: "rdp".into(),
        name: "Remote Desktop".into(),
        url: "".into(),
        group: group_index(&o.rdp_group),
        enabled: o.rdp_enabled,
        flash: false,
        adapter: "rdp".into(),
    });
    ui.set_svc_rows(ModelRc::new(VecModel::from(rows)));
    populate_svc_opts(ui);
}

fn populate_svc_opts(ui: &SettingsWindow) {
    let o = load_svc();
    ui.set_svc_opts(SvcOptsUi {
        flash: o.flash,
        sound: o.sound,
        sound_name: o.sound_file.as_str().into(),
        settle: o.flash_settle_s as i32,
        guard: o.hover_guard_ms as i32,
        list: o.mail_list,
        preview: o.mail_preview,
        max: o.mail_max as i32,
        panel: o.open_in_panel,
        widen: o.panel_widen,
        stale: o.stale_hours as i32,
        activity: o.teams_activity,
        threads: o.teams_threads,
        poll: o.poll_s as i32,
        badge: o.badge_max as i32,
        sched_group: sched_index(&o.sched_group),
        days: o.sched_days as i32,
        start: o.sched_start as i32,
        end: o.sched_end as i32,
        low_memory: o.low_memory,
        dev: o.dev,
        signin: o.signin_alert,
        signin_sound_name: o.signin_sound_file.as_str().into(),
        avail: o.teams_available,
        avail_start: o.avail_start as i32,
        avail_end: o.avail_end as i32,
    });
}

// Applies an options change to the bars, badges and flashing.
fn edit_svc(f: impl FnOnce(&mut SvcOpts)) {
    update_svc(f);
    crate::theme::apply_theme_all();
    crate::refresh_all();
    relayout();
}

fn edit_services(f: impl FnOnce(&mut Vec<webservices::ServiceCfg>)) {
    let mut list = load_services();
    f(&mut list);
    save_services(list);
}

fn slug(name: &str, taken: &[String]) -> String {
    let base: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let base = if base.is_empty() { "service".to_string() } else { base };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}{n}");
        n += 1;
    }
    id
}

pub(super) fn wire_svc(ui: &SettingsWindow) {
    wire_svc_rows(ui);
    wire_svc_opts(ui);
}

fn wire_svc_rows(ui: &SettingsWindow) {
    ui.on_svc_row_bool(|i, key, on| {
        if i as usize == load_services().len() {
            if key == "enabled" {
                update_svc(|o| o.rdp_enabled = on);
                crate::refresh_all();
                relayout();
            }
            return;
        }
        let mut id = String::new();
        edit_services(|l| {
            if let Some(c) = l.get_mut(i as usize) {
                id = c.id.clone();
                match key.as_str() {
                    "enabled" => c.enabled = on,
                    _ => c.flash = on,
                }
            }
        });
        if key == "enabled" {
            if on {
                crate::services::start_active();
            } else {
                webservices::stop_service(&id);
            }
            crate::refresh_all();
            relayout();
        }
    });
    ui.on_svc_row_text(|i, key, text| {
        edit_services(|l| {
            if let Some(c) = l.get_mut(i as usize) {
                match key.as_str() {
                    "name" => c.name = text.to_string(),
                    _ => c.url = text.trim().to_string(),
                }
            }
        });
    });
    let w = ui.as_weak();
    ui.on_svc_row_group(move |i, g| {
        let group = if g == 1 { "work" } else { "personal" };
        if i as usize == load_services().len() {
            update_svc(|o| o.rdp_group = group.into());
            crate::refresh_all();
            relayout();
            if let Some(ui) = w.upgrade() {
                populate_svc(&ui);
            }
            return;
        }
        let mut id = String::new();
        edit_services(|l| {
            if let Some(c) = l.get_mut(i as usize) {
                c.group = group.into();
                id = c.id.clone();
            }
        });
        webservices::set_group(&id, group);
        if crate::services::group_visible(group) {
            crate::services::start_active();
        } else {
            webservices::stop_service(&id);
        }
        crate::refresh_all();
        relayout();
        if let Some(ui) = w.upgrade() {
            populate_svc(&ui);
        }
    });
    let w = ui.as_weak();
    ui.on_svc_row_remove(move |i| {
        let mut id = String::new();
        edit_services(|l| {
            if (i as usize) < l.len() {
                id = l.remove(i as usize).id;
            }
        });
        webservices::stop_service(&id);
        crate::refresh_all();
        relayout();
        if let Some(ui) = w.upgrade() {
            populate_svc(&ui);
        }
    });
    let w = ui.as_weak();
    ui.on_svc_add(move |name, url, g| {
        let (name, mut url) = (name.trim().to_string(), url.trim().to_string());
        if name.is_empty() || url.is_empty() {
            return;
        }
        if !url.contains("://") {
            url = format!("https://{url}");
        }
        edit_services(|l| {
            let taken: Vec<String> = l.iter().map(|c| c.id.clone()).collect();
            let id = slug(&name, &taken);
            l.push(webservices::ServiceCfg {
                id,
                name,
                url,
                group: if g == 1 { "work" } else { "personal" }.into(),
                ..Default::default()
            });
        });
        crate::services::start_active();
        crate::refresh_all();
        relayout();
        if let Some(ui) = w.upgrade() {
            populate_svc(&ui);
        }
    });
}

fn wire_svc_opts(ui: &SettingsWindow) {
    ui.on_svc_opt_bool(|key, on| {
        edit_svc(|o| match key.as_str() {
            "flash" => o.flash = on,
            "sound" => o.sound = on,
            "mail_list" => o.mail_list = on,
            "mail_preview" => o.mail_preview = on,
            "open_in_panel" => o.open_in_panel = on,
            "panel_widen" => o.panel_widen = on,
            "teams_activity" => o.teams_activity = on,
            "teams_threads" => o.teams_threads = on,
            "low_memory" => o.low_memory = on,
            "dev" => o.dev = on,
            "signin_alert" => o.signin_alert = on,
            "teams_available" => o.teams_available = on,
            _ => {}
        });
    });
    ui.on_svc_opt_int(|key, v| {
        let v = v.max(0) as u32;
        edit_svc(|o| match key.as_str() {
            "flash_settle" => o.flash_settle_s = v.min(300),
            "hover_guard" => o.hover_guard_ms = v.min(3000),
            "mail_max" => o.mail_max = v.clamp(1, 20),
            "poll" => o.poll_s = v.clamp(1, 120),
            "badge_max" => o.badge_max = v.clamp(1, 99),
            "stale_hours" => o.stale_hours = v.min(720),
            "sched_group" => o.sched_group = ["", "work", "personal"][v.min(2) as usize].into(),
            "sched_start" => o.sched_start = v.min(23),
            "sched_end" => o.sched_end = v.min(24),
            "avail_start" => o.avail_start = v.min(23),
            "avail_end" => o.avail_end = v.min(24),
            _ => {}
        });
    });
    ui.on_svc_opt_day(|day, on| {
        edit_svc(|o| {
            let bit = 1u32 << (day as u32 % 7);
            if on {
                o.sched_days |= bit;
            } else {
                o.sched_days &= !bit;
            }
        });
    });
    let w = ui.as_weak();
    ui.on_svc_sound(move |action| {
        let Some(ui) = w.upgrade() else { return };
        let signin = action >= 10;
        let (stem, action) = if signin { ("signin", action - 10) } else { ("notify", action) };
        match action {
            0 => {
                let owner = crate::shell::winit_hwnd(ui.window());
                let Some(src) = super::profiles::pick_sound(owner) else { return };
                let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("wav").to_lowercase();
                let name = format!("{stem}.{ext}");
                let Some(dir) = crate::config::config_dir() else { return };
                if let Some(old) = std::fs::read_dir(&dir).ok().into_iter().flatten().flatten().find(|e| e.file_name().to_string_lossy().starts_with(&format!("{stem}."))) {
                    let _ = std::fs::remove_file(old.path());
                }
                if std::fs::copy(&src, dir.join(&name)).is_ok() {
                    edit_svc(|o| if signin { o.signin_sound_file = name } else { o.sound_file = name });
                    if signin { crate::services::signin_sound() } else { crate::services::notify_sound() }
                }
            }
            1 => if signin { crate::services::signin_sound() } else { crate::services::notify_sound() },
            _ => edit_svc(|o| if signin { o.signin_sound_file.clear() } else { o.sound_file.clear() }),
        }
        populate_svc_opts(&ui);
    });
    let w = ui.as_weak();
    ui.on_svc_reset(move || {
        edit_svc(|o| *o = SvcOpts::default());
        if let Some(ui) = w.upgrade() {
            populate_svc_opts(&ui);
        }
    });
}
