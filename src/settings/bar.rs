use super::*;

pub(super) fn populate_ui(ui: &SettingsWindow) {
    let u = load_ui();
    ui.set_icon_size(u.icon_size as i32);
    ui.set_bar_height_default(crate::shell::native_bar_height() as i32);
    ui.set_bar_height(if u.bar_height == 0 { crate::shell::native_bar_height() } else { u.bar_height } as i32);
    let t = unsafe { GetLocalTime() };
    let mut opts = vec!["None".to_string()];
    opts.extend((1..=5).map(|k| crate::tasks::date_text(k, &t)));
    ui.set_date_options(strings(opts));
    ui.set_clock_seconds(u.clock_seconds);
    ui.set_clock_date(u.clock_date as i32);
    ui.set_anim_mode(u.anim_mode as i32);
    ui.set_tip_delay(u.tip_delay_ms as i32);
    ui.set_preview_delay(u.preview_delay_ms as i32);
    ui.set_tray_cols(u.tray_cols as i32);
    ui.set_show_cal(u.show_calendar);
    ui.set_show_wifi(u.show_wifi);
    ui.set_show_bt(u.show_bluetooth);
    ui.set_show_mixer(u.show_mixer);
    ui.set_mixer_hide_system(u.mixer_hide_system);
}

pub(super) fn edit_ui(f: impl FnOnce(&mut UiCfg)) {
    update_ui(f);
    crate::theme::apply_theme_all();
    crate::refresh_all();
    relayout();
}

// Bar, tray and quick-settings options.
pub(super) fn wire_ui(ui: &SettingsWindow) {
    wire_fields(ui);
    wire_app_lists(ui);
    wire_resets(ui);
    ui.on_open_config(|| {
        if let Some(path) = config_dir().map(|d| d.join("config.toml")) {
            let wide = to_wide(&path.to_string_lossy());
            unsafe {
                ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
            }
        }
    });
}

fn wire_fields(ui: &SettingsWindow) {
    ui.on_ui_int(|key, v| {
        let v = v.max(0) as u32;
        edit_ui(|u| match key.as_str() {
            "icon_size" => u.icon_size = v.clamp(12, 32),
            "bar_height" => u.bar_height = v,
            "clock_date" => u.clock_date = v.min(5),
            "anim_mode" => u.anim_mode = v.min(3),
            "tip_delay" => u.tip_delay_ms = v,
            "preview_delay" => u.preview_delay_ms = v.min(2000),
            "tray_cols" => u.tray_cols = v.clamp(2, 10),
            _ => {}
        });
        if key == "tray_cols" {
            crate::popup::refresh_tray_flyout();
        }
    });
    ui.on_ui_bool(|key, on| {
        edit_ui(|u| match key.as_str() {
            "clock_seconds" => u.clock_seconds = on,
            "show_calendar" => u.show_calendar = on,
            "show_wifi" => u.show_wifi = on,
            "show_bluetooth" => u.show_bluetooth = on,
            "show_mixer" => u.show_mixer = on,
            "mixer_hide_system" => u.mixer_hide_system = on,
            _ => {}
        });
    });
}

// Tray overflow apps and hidden mixer apps.
fn wire_app_lists(ui: &SettingsWindow) {
    let w = ui.as_weak();
    ui.on_tray_app_toggled(move |i, on| {
        if let Some((exe, _, _)) = crate::tray::known_apps().into_iter().nth(i as usize) {
            crate::tray::set_overflow(&exe, on);
        }
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });
    let w = ui.as_weak();
    ui.on_tray_app_add(move |exe| {
        let exe = exe.trim().to_lowercase();
        if !exe.is_empty() {
            crate::tray::set_overflow(&exe, true);
        }
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });
    let w = ui.as_weak();
    ui.on_mixer_unhide(move |i| {
        edit_ui(|u| {
            if (i as usize) < u.mixer_hidden.len() {
                u.mixer_hidden.remove(i as usize);
            }
        });
        if let Some(ui) = w.upgrade() {
            populate_lists(&ui);
        }
    });
}

fn wire_resets(ui: &SettingsWindow) {
    let w2 = ui.as_weak();
    ui.on_reset_field(move |key| {
        let d = UiCfg::default();
        edit_ui(|u| match key.as_str() {
            "icon_size" => u.icon_size = d.icon_size,
            "bar_height" => u.bar_height = 0,
            "clock_date" => u.clock_date = d.clock_date,
            "clock_seconds" => u.clock_seconds = d.clock_seconds,
            "anim_mode" => u.anim_mode = d.anim_mode,
            "tip_delay" => u.tip_delay_ms = d.tip_delay_ms,
            "preview_delay" => u.preview_delay_ms = d.preview_delay_ms,
            "tray_cols" => u.tray_cols = d.tray_cols,
            _ => {}
        });
        if key == "tray_cols" {
            crate::popup::refresh_tray_flyout();
        }
        if let Some(ui) = w2.upgrade() {
            populate_ui(&ui);
        }
    });
    let w = ui.as_weak();
    ui.on_reset_section(move |tab| {
        let d = UiCfg::default();
        edit_ui(|u| match tab {
            4 => {
                u.icon_size = d.icon_size;
                u.bar_height = d.bar_height;
                u.clock_seconds = d.clock_seconds;
                u.clock_date = d.clock_date;
                u.anim_mode = d.anim_mode;
                u.preview_delay_ms = d.preview_delay_ms;
            }
            5 => {
                u.tip_delay_ms = d.tip_delay_ms;
                u.tray_cols = d.tray_cols;
            }
            _ => {
                u.show_calendar = d.show_calendar;
                u.show_wifi = d.show_wifi;
                u.show_bluetooth = d.show_bluetooth;
                u.show_mixer = d.show_mixer;
                u.mixer_hide_system = d.mixer_hide_system;
                u.mixer_hidden.clear();
            }
        });
        if tab == 5 {
            for (exe, _, on) in crate::tray::known_apps() {
                if on {
                    crate::tray::set_overflow(&exe, false);
                }
            }
        }
        if let Some(ui) = w.upgrade() {
            populate_ui(&ui);
            populate_lists(&ui);
        }
    });
}
