use crate::prelude::*;
use crate::theme::parse_color;
use crate::ui::{ColorRow, GridItem, MonRow, PresetItem, RdpRow, RuleRow, SettingsWindow, SvcOptsUi, SvcRow, TrayApp};

mod bar;
mod colors;
mod lists;
mod profiles;
mod svc;

use bar::*;
use colors::*;
use lists::*;
use profiles::*;
use svc::*;


thread_local! {
    static WIN: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
    static LAST_LISTS: RefCell<String> = const { RefCell::new(String::new()) };
}

fn lists_snapshot() -> String {
    let apps: Vec<(String, bool)> = crate::tray::known_apps().into_iter().map(|a| (a.0, a.2)).collect();
    format!("{:?}|{:?}|{:?}|{:?}|{:?}", load_pins(), sorted_icon_only(), load_icon_rules_raw(), apps, load_ui().mixer_hidden)
}

// Called from the main tick: refreshes the lists when pins/icon-only/rules were changed elsewhere (e.g. bar menu).
pub(crate) fn sync_open_window() {
    let Some(ui) = WIN.with(|w| w.borrow().as_ref().map(|u| u.clone_strong())) else { return };
    if !ui.window().is_visible() || lists_snapshot() == LAST_LISTS.with(|l| l.borrow().clone()) {
        return;
    }
    populate_lists(&ui);
}

fn strings(v: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

fn populate(ui: &SettingsWindow) {
    let t = load_theme();
    ui.set_autostart(crate::autostart::is_enabled());
    ui.set_clock_24h(t.clock_24h);
    ui.set_colors(ModelRc::new(VecModel::from(color_rows(&t))));
    ui.set_radius(t.button_radius.round() as i32);
    ui.set_anim_ms(t.animation_ms as i32);
    let d = ThemeCfg::default();
    ui.set_radius_default(d.button_radius.round() as i32);
    ui.set_anim_default(d.animation_ms as i32);
    ui.set_presets(ModelRc::new(VecModel::from(preset_items())));
    ui.set_config_path(config_dir().map(|d| d.join("config.toml").to_string_lossy().into_owned()).unwrap_or_default().into());
    populate_lists(ui);
    populate_ui(ui);
    populate_profiles(ui);
    populate_svc(ui);
}

// Re-reads the config into State and redraws every bar.
fn changed() {
    apply_to_state();
    STATE.with(|s| s.borrow_mut().file_icons.clear());
    crate::theme::apply_theme_all();
    crate::refresh_all();
    relayout();
}

fn relayout() {
    for m in bar_keys() {
        push_ui(m);
    }
}

// Edits the config of the i-th monitor row, re-lays out the bars and refreshes the lists.
fn edit_monitor(w: &slint::Weak<SettingsWindow>, i: i32, f: impl FnOnce(&mut MonitorCfg)) {
    if let Some((d, _, _)) = monitor_pins().get(i as usize) {
        update_monitor(d, f);
        relayout();
    }
    if let Some(ui) = w.upgrade() {
        populate_lists(&ui);
    }
}

fn edit_theme(f: impl FnOnce(&mut ThemeCfg)) {
    update_theme(f);
    crate::theme::apply_theme_all();
    relayout();
}

pub(crate) fn with_window(f: impl FnOnce(&SettingsWindow)) {
    if let Some(ui) = WIN.with(|w| w.borrow().as_ref().map(|u| u.clone_strong())) {
        f(&ui);
    }
}

pub(crate) fn open_settings() {
    let existing = WIN.with(|w| w.borrow().as_ref().map(|u| u.clone_strong()));
    if let Some(ui) = existing {
        populate(&ui);
        crate::theme::apply_window_theme(&ui);
        let _ = ui.show();
        return;
    }
    let Ok(ui) = SettingsWindow::new() else { return };
    populate(&ui);
    crate::theme::apply_window_theme(&ui);
    wire(&ui);
    wire_profiles(&ui);
    wire_svc(&ui);
    let _ = ui.show();
    WIN.with(|w| *w.borrow_mut() = Some(ui));
}

fn wire(ui: &SettingsWindow) {
    ui.on_autostart_toggled(crate::autostart::set_enabled);
    ui.on_clock_toggled(|on| edit_theme(|t| t.clock_24h = on));
    ui.on_radius_edited(|v| edit_theme(|t| t.button_radius = v as f32));
    ui.on_anim_edited(|v| edit_theme(|t| t.animation_ms = v.max(0) as u32));

    let w = ui.as_weak();
    ui.on_color_edited(move |i, text| {
        if let Some(ui) = w.upgrade() {
            set_color(&ui, i as usize, &text);
        }
    });

    let w = ui.as_weak();
    ui.on_color_picked(move |i, c| {
        let hex = if c.alpha() == 255 {
            format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue(), c.alpha())
        };
        if let Some(ui) = w.upgrade() {
            set_color(&ui, i as usize, &hex);
        }
    });

    let w = ui.as_weak();
    ui.on_color_reset(move |i| {
        let text = color_field(&mut ThemeCfg::default(), i as usize).clone();
        if let Some(ui) = w.upgrade() {
            set_color(&ui, i as usize, &text);
        }
    });

    let w = ui.as_weak();
    ui.on_apply_preset(move |i| {
        if let Some(ui) = w.upgrade() {
            let t = preset_theme(i as usize, &load_theme());
            apply_whole_theme(&ui, t);
        }
    });

    let w = ui.as_weak();
    ui.on_theme_from_accent(move |c| {
        let hex = format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue());
        if let Some(ui) = w.upgrade() {
            let base = load_theme();
            apply_whole_theme(&ui, palette_from(&hex, is_light(&base), &base));
        }
    });

    let w = ui.as_weak();
    ui.on_theme_from_accent_text(move |text| {
        if parse_color(&text).is_none() {
            return;
        }
        if let Some(ui) = w.upgrade() {
            let base = load_theme();
            apply_whole_theme(&ui, palette_from(text.trim(), is_light(&base), &base));
        }
    });

    let w = ui.as_weak();
    ui.on_reset_theme(move || {
        save_theme(ThemeCfg::default());
        crate::theme::apply_theme_all();
        relayout();
        if let Some(ui) = w.upgrade() {
            populate(&ui);
        }
    });

    wire_rules(ui);
    wire_monitors(ui);
    wire_ui(ui);
}

