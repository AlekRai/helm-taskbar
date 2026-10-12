use super::*;

pub(super) const COLOR_KEYS: [&str; 11] = [
    "Accent",
    "Bar background",
    "Button hover",
    "Button pressed",
    "Active window",
    "Active window hover",
    "Running indicator",
    "Text",
    "Dimmed text",
    "Popup background",
    "Popup border",
];

pub(super) fn color_field(t: &mut ThemeCfg, i: usize) -> &mut String {
    match i {
        0 => &mut t.accent,
        1 => &mut t.bar,
        2 => &mut t.hover,
        3 => &mut t.pressed,
        4 => &mut t.selected,
        5 => &mut t.selected_hover,
        6 => &mut t.running_dot,
        7 => &mut t.text,
        8 => &mut t.text_dim,
        9 => &mut t.popup,
        _ => &mut t.popup_border,
    }
}

pub(super) fn color_rows(theme: &ThemeCfg) -> Vec<ColorRow> {
    let mut t = theme.clone();
    let mut def = ThemeCfg::default();
    (0..COLOR_KEYS.len())
        .map(|i| {
            let value = color_field(&mut t, i).clone();
            let modified = !value.eq_ignore_ascii_case(color_field(&mut def, i));
            ColorRow {
                key: COLOR_KEYS[i].into(),
                label: COLOR_KEYS[i].into(),
                swatch: parse_color(&value).unwrap_or_default(),
                value: value.into(),
                modified,
            }
        })
        .collect()
}

pub(super) fn hex(h: f32, s: f32, l: f32) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let b8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", b8(r), b8(g), b8(b))
}

// Builds a full palette around one accent color; keeps radius, speed and clock format.
pub(super) fn palette_from(accent: &str, light: bool, base: &ThemeCfg) -> ThemeCfg {
    let Some(c) = parse_color(accent) else { return base.clone() };
    let hsv = c.to_hsva();
    let h = hsv.hue;
    let s = (hsv.saturation * 0.3).clamp(0.08, 0.22);
    let mut t = base.clone();
    t.accent = accent.to_string();
    let set = |l: f32, sat: f32| hex(h, sat, l);
    if light {
        t.bar = set(0.93, s);
        t.hover = set(0.88, s);
        t.pressed = set(0.84, s);
        t.selected = set(0.82, s);
        t.selected_hover = set(0.78, s);
        t.running_dot = set(0.45, s * 0.6);
        t.text = set(0.10, s * 0.5);
        t.text_dim = set(0.40, s * 0.5);
        t.popup = set(0.96, s);
        t.popup_border = set(0.78, s);
    } else {
        t.bar = set(0.10, s);
        t.hover = set(0.14, s);
        t.pressed = set(0.12, s);
        t.selected = set(0.19, s);
        t.selected_hover = set(0.23, s);
        t.running_dot = set(0.45, s * 0.6);
        t.text = set(0.95, s * 0.3);
        t.text_dim = set(0.60, s * 0.5);
        t.popup = set(0.125, s);
        t.popup_border = set(0.22, s);
    }
    t
}

// (name, accent, light); None accent = built-in default palette.
pub(super) const PRESETS: [(&str, Option<&str>, bool); 10] = [
    ("Default", None, false),
    ("Midnight", Some("#5b8cff"), false),
    ("Forest", Some("#4cc38a"), false),
    ("Rose", Some("#ff6b9a"), false),
    ("Amber", Some("#ffb347"), false),
    ("Violet", Some("#a77bff"), false),
    ("Teal", Some("#2dd4d4"), false),
    ("Crimson", Some("#ff5252"), false),
    ("Graphite", Some("#9aa4b2"), false),
    ("Light", Some("#2f7cf6"), true),
];

pub(super) fn preset_theme(i: usize, base: &ThemeCfg) -> ThemeCfg {
    let (_, accent, light) = PRESETS[i];
    match accent {
        None => ThemeCfg { button_radius: base.button_radius, animation_ms: base.animation_ms, clock_24h: base.clock_24h, ..ThemeCfg::default() },
        Some(a) => palette_from(a, light, base),
    }
}

pub(super) fn preset_items() -> Vec<PresetItem> {
    let base = ThemeCfg::default();
    (0..PRESETS.len())
        .map(|i| {
            let t = preset_theme(i, &base);
            let col = |s: &str| parse_color(s).unwrap_or_default();
            PresetItem { name: PRESETS[i].0.into(), bar: col(&t.bar), accent: col(&t.accent), text: col(&t.text) }
        })
        .collect()
}

pub(super) fn is_light(t: &ThemeCfg) -> bool {
    parse_color(&t.bar).map(|c| c.to_hsva().value > 0.5).unwrap_or(false)
}

pub(super) fn apply_whole_theme(ui: &SettingsWindow, t: ThemeCfg) {
    save_theme(t.clone());
    crate::theme::apply_theme_all();
    relayout();
    ui.set_colors(ModelRc::new(VecModel::from(color_rows(&t))));
}

// Invalid text is left alone until it parses.
pub(super) fn set_color(ui: &SettingsWindow, i: usize, text: &str) {
    let Some(c) = parse_color(text) else { return };
    edit_theme(|t| *color_field(t, i) = text.to_string());
    let rows = ui.get_colors();
    if let Some(mut row) = rows.row_data(i) {
        row.swatch = c;
        row.value = text.into();
        row.modified = !text.eq_ignore_ascii_case(color_field(&mut ThemeCfg::default(), i));
        rows.set_row_data(i, row);
    }
}

