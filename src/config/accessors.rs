use super::*;

pub(super) fn to_pairs(v: &[Pair]) -> Vec<(String, String)> {
    v.iter().map(|p| (p.device.clone(), p.exe.clone())).collect()
}

pub(super) fn from_pairs(v: &[(String, String)]) -> Vec<Pair> {
    v.iter().map(|(d, e)| Pair { device: d.clone(), exe: e.clone() }).collect()
}

pub(crate) fn load_pins() -> Vec<(String, String)> {
    with_store(|st| to_pairs(&st.cfg.pins))
}

pub(crate) fn save_pins(pins: &[(String, String)]) {
    mutate(|c| c.pins = from_pairs(pins));
}

pub(crate) fn load_order() -> Vec<(String, String)> {
    with_store(|st| to_pairs(&st.cfg.order))
}

pub(crate) fn save_order(order: &[(String, String)]) {
    mutate(|c| c.order = from_pairs(order));
}

pub(crate) fn load_windows() -> Vec<WinRec> {
    with_store(|st| st.cfg.windows.clone())
}

pub(crate) fn save_windows(w: Vec<WinRec>) {
    mutate(|c| c.windows = w);
}

pub(crate) fn load_icon_only() -> HashSet<String> {
    with_store(|st| st.cfg.icon_only.iter().map(|s| s.to_lowercase()).collect())
}

pub(crate) fn save_icon_only(set: &HashSet<String>) {
    mutate(|c| {
        let mut v: Vec<String> = set.iter().cloned().collect();
        v.sort();
        c.icon_only = v;
    });
}

pub(crate) fn load_tray_overflow() -> HashSet<String> {
    with_store(|st| st.cfg.tray_overflow.iter().map(|s| s.to_lowercase()).collect())
}

pub(crate) fn save_tray_overflow(set: &HashSet<String>) {
    mutate(|c| {
        let mut v: Vec<String> = set.iter().cloned().collect();
        v.sort();
        c.tray_overflow = v;
    });
}

// (exe, title substring, absolute icon path), exe and title lowercased.
pub(crate) fn load_icon_rules() -> Vec<(String, String, String)> {
    let icons_dir = config_dir().map(|d| d.join("icons")).unwrap_or_default();
    with_store(|st| {
        st.cfg
            .icon_rules
            .iter()
            .filter(|r| !r.exe.is_empty() && !r.icon.is_empty())
            .map(|r| {
                let icon = PathBuf::from(&r.icon);
                let icon = if icon.is_absolute() { icon } else { icons_dir.join(icon) };
                (r.exe.to_lowercase(), r.title.to_lowercase(), icon.to_string_lossy().into_owned())
            })
            .collect()
    })
}

pub(crate) fn load_start_pins() -> Vec<String> {
    with_store(|st| st.cfg.start_pins.clone())
}

pub(crate) fn save_start_pins(pins: Vec<String>) {
    mutate(|c| c.start_pins = pins);
}

pub(crate) fn load_theme() -> std::rc::Rc<ThemeCfg> {
    with_store(|st| st.theme.clone())
}

pub(crate) fn save_theme(t: ThemeCfg) {
    mutate(|c| c.theme = t);
}

pub(crate) fn update_theme(f: impl FnOnce(&mut ThemeCfg)) {
    mutate(|c| f(&mut c.theme));
}

pub(crate) fn load_ui() -> std::rc::Rc<UiCfg> {
    with_store(|st| st.ui.clone())
}

pub(crate) fn update_ui(f: impl FnOnce(&mut UiCfg)) {
    mutate(|c| f(&mut c.ui));
}

pub(crate) fn monitor_cfg(device: &str) -> MonitorCfg {
    with_store(|st| {
        st.cfg.monitors.iter().find(|m| m.device == device).cloned().unwrap_or(MonitorCfg {
            device: device.to_string(),
            ..Default::default()
        })
    })
}

pub(crate) fn update_monitor(device: &str, f: impl FnOnce(&mut MonitorCfg)) {
    let mut m = monitor_cfg(device);
    f(&mut m);
    mutate(|c| match c.monitors.iter_mut().find(|x| x.device == m.device) {
        Some(x) => *x = m,
        None => c.monitors.push(m),
    });
}

// Rules exactly as written in the file (exe, title substring, icon path).
pub(crate) fn load_icon_rules_raw() -> Vec<(String, String, String)> {
    with_store(|st| st.cfg.icon_rules.iter().map(|r| (r.exe.clone(), r.title.clone(), r.icon.clone())).collect())
}

pub(crate) fn save_icon_rules_raw(rules: Vec<(String, String, String)>) {
    mutate(|c| c.icon_rules = rules.into_iter().map(|(exe, title, icon)| IconRule { exe, title, icon }).collect());
}

