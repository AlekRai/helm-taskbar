use crate::prelude::*;
use crate::ui::Theme;
use slint::Global;

pub(crate) fn parse_color(text: &str) -> Option<slint::Color> {
    let hex = text.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(slint::Color::from_rgb_u8(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(slint::Color::from_argb_u8(byte(6)?, byte(0)?, byte(2)?, byte(4)?)),
        _ => None,
    }
}

fn color(text: &str, fallback: slint::Color) -> slint::Color {
    parse_color(text).unwrap_or(fallback)
}

fn apply_values(th: &Theme<'_>, t: &ThemeCfg) {
    th.set_accent(color(&t.accent, th.get_accent()));
    th.set_bg(color(&t.bar, th.get_bg()));
    th.set_hover(color(&t.hover, th.get_hover()));
    th.set_pressed(color(&t.pressed, th.get_pressed()));
    th.set_selected(color(&t.selected, th.get_selected()));
    th.set_selected_hover(color(&t.selected_hover, th.get_selected_hover()));
    th.set_running_dot(color(&t.running_dot, th.get_running_dot()));
    th.set_text(color(&t.text, th.get_text()));
    th.set_text_dim(color(&t.text_dim, th.get_text_dim()));
    th.set_popup_bg(color(&t.popup, th.get_popup_bg()));
    th.set_popup_border(color(&t.popup_border, th.get_popup_border()));
    th.set_button_radius(t.button_radius.clamp(0.0, 24.0));
    th.set_anim(t.animation_ms.min(2000) as i64);
    let ui_cfg = load_ui();
    th.set_popup_scale(ui_cfg.anim_scale());
    th.set_app_icon(ui_cfg.icon_size.clamp(12, 32) as f32);
}

pub(crate) fn apply_bar_theme(ui: &BarWindow) {
    apply_values(&Theme::get(ui), &load_theme());
}

pub(crate) fn apply_window_theme(ui: &crate::ui::SettingsWindow) {
    apply_values(&Theme::get(ui), &load_theme());
    ui.invoke_sync_palette();
}

// Re-applies the config theme to every bar and popup.
pub(crate) fn apply_theme_all() {
    let t = load_theme();
    crate::settings::with_window(apply_window_theme);
    for mon in bar_keys() {
        if let Some(ui) = with_bar(mon, |b| b.ui.clone_strong()) {
            apply_values(&Theme::get(&ui), &t);
        }
    }
    apply_popup_theme(&t);
}

// Flips the repaint flag that every popup background depends on, dirtying whole windows.
pub(crate) fn force_repaint() {
    crate::popup::try_with_popup_uis(|menu, start, quick, fly, tip| {
        for th in [Theme::get(menu), Theme::get(start), Theme::get(quick), Theme::get(fly), Theme::get(tip)] {
            th.set_repaint(!th.get_repaint());
        }
    });
}

pub(crate) fn apply_popup_theme(t: &ThemeCfg) {
    with_popup_uis(|menu, start, quick, fly, tip| {
        for th in [Theme::get(menu), Theme::get(start), Theme::get(quick), Theme::get(fly), Theme::get(tip)] {
            apply_values(&th, t);
        }
    });
}
