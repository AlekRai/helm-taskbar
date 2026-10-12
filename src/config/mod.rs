use crate::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

mod accessors;
mod migrate;
mod profiles;

pub(crate) use accessors::*;
use migrate::*;
pub(crate) use profiles::*;


#[derive(Serialize, Deserialize, Clone, Default)]
struct Pair {
    device: String,
    exe: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct IconRule {
    exe: String,
    title: String,
    icon: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct WinRec {
    pub(crate) exe: String,
    pub(crate) title: String,
    pub(crate) hwnd: isize,
    pub(crate) pid: u32,
    pub(crate) rank: usize,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub(crate) struct ThemeCfg {
    // Colors are "#rrggbb" or "#rrggbbaa".
    pub(crate) accent: String,
    pub(crate) bar: String,
    pub(crate) hover: String,
    pub(crate) pressed: String,
    pub(crate) selected: String,
    pub(crate) selected_hover: String,
    pub(crate) running_dot: String,
    pub(crate) text: String,
    pub(crate) text_dim: String,
    pub(crate) popup: String,
    pub(crate) popup_border: String,
    pub(crate) button_radius: f32,
    pub(crate) animation_ms: u32,
    pub(crate) clock_24h: bool,
}

impl Default for ThemeCfg {
    fn default() -> Self {
        let s = |v: &str| v.to_string();
        ThemeCfg {
            accent: s("#4ca0ff"),
            bar: s("#18181c"),
            hover: s("#24242a"),
            pressed: s("#1e1e23"),
            selected: s("#2c2c34"),
            selected_hover: s("#36363f"),
            running_dot: s("#6e6e7a"),
            text: s("#f0f0f0"),
            text_dim: s("#7c7c88"),
            popup: s("#202024"),
            popup_border: s("#3a3a42"),
            button_radius: 4.0,
            animation_ms: 120,
            clock_24h: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub(crate) struct UiCfg {
    // Tray tooltip delay; 0 turns tooltips off.
    pub(crate) tip_delay_ms: u32,
    pub(crate) tray_cols: u32,
    pub(crate) show_calendar: bool,
    pub(crate) show_mixer: bool,
    pub(crate) show_bluetooth: bool,
    pub(crate) show_wifi: bool,
    pub(crate) mixer_hide_system: bool,
    // Lowercase exe names left out of the volume mixer.
    pub(crate) mixer_hidden: Vec<String>,
    // Popup animations: 0 off, 1 fast, 2 normal, 3 slow.
    pub(crate) anim_mode: u32,
    pub(crate) clock_seconds: bool,
    // 0 none, 1 short (12 Oct), 2 long (Mon 12 Oct).
    pub(crate) clock_date: u32,
    pub(crate) icon_size: u32,
    // 0 matches the Windows taskbar height.
    pub(crate) bar_height: u32,
}

impl Default for UiCfg {
    fn default() -> Self {
        UiCfg {
            tip_delay_ms: 450,
            tray_cols: 5,
            show_calendar: true,
            show_mixer: true,
            show_bluetooth: true,
            show_wifi: true,
            mixer_hide_system: false,
            mixer_hidden: Vec::new(),
            anim_mode: 2,
            clock_seconds: false,
            clock_date: 0,
            icon_size: 22,
            bar_height: 0,
        }
    }
}

impl UiCfg {
    // Multiplier for popup animation durations; 0 means no animation.
    pub(crate) fn anim_scale(&self) -> f32 {
        match self.anim_mode {
            0 => 0.0,
            1 => 0.5,
            3 => 2.0,
            _ => 1.0,
        }
    }
}

// Options for the built-in web services (see the Services settings tab).
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub(crate) struct SvcOpts {
    // Flash a service (and its new mails) when mail arrives.
    pub(crate) flash: bool,
    // Play the Windows notification sound when new mail arrives.
    pub(crate) sound: bool,
    // Seconds after a service starts during which already-present mails don't count as new.
    pub(crate) flash_settle_s: u32,
    // A mail list ignores hover for this long after opening.
    pub(crate) hover_guard_ms: u32,
    // Clicking a service lists its unread mails instead of opening it.
    pub(crate) mail_list: bool,
    pub(crate) mail_preview: bool,
    pub(crate) mail_max: u32,
    // Clicking a mail shows it in the panel next to the list instead of the full window.
    pub(crate) open_in_panel: bool,
    pub(crate) panel_widen: bool,
    // Messenger: ignore reaction-only unreads older than this many hours (0 = never ignore).
    pub(crate) stale_hours: u32,
    // Teams: count the Activity badge as well as Chat.
    pub(crate) teams_activity: bool,
    // Teams: keep the hidden page on Followed threads and list its threads.
    pub(crate) teams_threads: bool,
    // Seconds between mail list reads (Outlook, Messenger, Teams).
    pub(crate) poll_s: u32,
    // Largest count a badge shows before turning into "N+".
    pub(crate) badge_max: u32,
    // Group that only runs in the schedule: "work", "personal" or "".
    pub(crate) sched_group: String,
    // Bit n = weekday n (0 = Sunday).
    pub(crate) sched_days: u32,
    pub(crate) sched_start: u32,
    pub(crate) sched_end: u32,
    // DevTools port and adapter logging; applies at next start.
    pub(crate) dev: bool,
}

impl Default for SvcOpts {
    fn default() -> Self {
        SvcOpts {
            flash: true,
            sound: true,
            flash_settle_s: 20,
            hover_guard_ms: 700,
            mail_list: true,
            mail_preview: true,
            mail_max: 20,
            open_in_panel: true,
            panel_widen: true,
            stale_hours: 24,
            teams_activity: true,
            teams_threads: true,
            poll_s: 3,
            badge_max: 9,
            sched_group: "work".into(),
            sched_days: 0b0111110,
            sched_start: 6,
            sched_end: 17,
            dev: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub(crate) struct MonitorCfg {
    pub(crate) device: String,
    pub(crate) show_labels: bool,
    pub(crate) show_clock: bool,
    pub(crate) show_start: bool,
    // Tray, language and clock on the left edge instead of the right.
    pub(crate) tray_left: bool,
    // Language button and tray icons; unset means "only on the primary monitor".
    pub(crate) show_lang: Option<bool>,
    pub(crate) show_tray: Option<bool>,
    // Service group buttons; unset means like the tray.
    pub(crate) show_services: Option<bool>,
}

impl MonitorCfg {
    pub(crate) fn lang_on(&self, primary: bool) -> bool {
        self.show_lang.unwrap_or(primary)
    }
    pub(crate) fn tray_on(&self, primary: bool) -> bool {
        self.show_tray.unwrap_or(primary)
    }
    pub(crate) fn services_on(&self, primary: bool) -> bool {
        self.show_services.unwrap_or_else(|| self.tray_on(primary))
    }
}

impl Default for MonitorCfg {
    fn default() -> Self {
        MonitorCfg {
            device: String::new(),
            show_labels: true,
            show_clock: true,
            show_start: true,
            tray_left: false,
            show_lang: None,
            show_tray: None,
            show_services: None,
        }
    }
}

// Everything user-configurable lives in %APPDATA%\Helm\config.toml.
#[derive(Serialize, Deserialize, Clone, Default)]
struct Config {
    // Start menu pinned apps (shell parsing names), in display order.
    #[serde(default)]
    start_pins: Vec<String>,
    #[serde(default)]
    theme: ThemeCfg,
    #[serde(default)]
    ui: UiCfg,
    #[serde(default)]
    svc: SvcOpts,
    // Per-monitor options, keyed by device name.
    #[serde(default)]
    monitors: Vec<MonitorCfg>,
    // Order of windows of the same app, so it survives taskbar (and app) restarts.
    #[serde(default)]
    windows: Vec<WinRec>,
    // Apps pinned per monitor device.
    #[serde(default)]
    pins: Vec<Pair>,
    // Remembered button order per monitor; an exe appears once per window of that app.
    #[serde(default)]
    order: Vec<Pair>,
    // Lowercase exe names shown without a label.
    #[serde(default)]
    icon_only: Vec<String>,
    // Lowercase exe names of tray icons kept in the overflow flyout instead of the bar.
    #[serde(default)]
    tray_overflow: Vec<String>,
    // Icon override: exe (or "*") + case-insensitive title substring -> .ico path (relative to the icons folder).
    #[serde(default)]
    icon_rules: Vec<IconRule>,
    // Built-in web services; unset means the defaults (see webservices::default_services).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    services: Option<Vec<webservices::ServiceCfg>>,
}

struct Store {
    cfg: Config,
    // Shared snapshots handed out by load_ui/load_theme; rebuilt whenever cfg changes.
    ui: std::rc::Rc<UiCfg>,
    theme: std::rc::Rc<ThemeCfg>,
    svc: std::rc::Rc<SvcOpts>,
    // Modification time of the file as we last read or wrote it, to spot outside edits.
    stamp: Option<SystemTime>,
}

impl Store {
    fn new(cfg: Config, stamp: Option<SystemTime>) -> Self {
        let mut s = Store { ui: Default::default(), theme: Default::default(), svc: Default::default(), cfg, stamp };
        s.refresh();
        s
    }

    fn refresh(&mut self) {
        self.ui = std::rc::Rc::new(self.cfg.ui.clone());
        self.theme = std::rc::Rc::new(self.cfg.theme.clone());
        self.svc = std::rc::Rc::new(self.cfg.svc.clone());
    }
}

thread_local! {
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
}

pub(crate) fn config_dir() -> Option<PathBuf> {
    let appdata = PathBuf::from(std::env::var_os("APPDATA")?);
    let (new, old) = (appdata.join("Helm"), appdata.join("rust_taskbar"));
    // The app used to be called rust_taskbar: carry its settings and logins over once.
    if !new.exists() && old.exists() && std::fs::rename(&old, &new).is_err() {
        return Some(old);
    }
    Some(new)
}

fn config_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

fn stamp_of(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn write_cfg(cfg: &Config) -> Option<SystemTime> {
    let path = config_file()?;
    let _ = std::fs::create_dir_all(path.parent()?);
    let text = toml::to_string_pretty(cfg).ok()?;
    if let Err(e) = std::fs::write(&path, text) {
        log(&format!("config: could not write {}: {e}", path.display()));
        return None;
    }
    stamp_of(&path)
}

fn load_cfg(primary: &str) -> Store {
    let path = config_file();
    if let Some(p) = &path
        && let Ok(text) = std::fs::read_to_string(p) {
            match toml::from_str::<Config>(&text) {
                Ok(cfg) => return Store::new(cfg, stamp_of(p)),
                // Keep running with the in-memory defaults if the user's file has a syntax error.
                Err(e) => {
                    log(&format!("config: {} is invalid, using defaults: {e}", p.display()));
                    return Store::new(Config::default(), stamp_of(p));
                }
            }
        }
    let cfg = migrate(primary);
    let stamp = write_cfg(&cfg);
    Store::new(cfg, stamp)
}

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    STORE.with(|s| {
        let mut s = s.borrow_mut();
        let store = s.get_or_insert_with(|| load_cfg(&primary_device_name()));
        f(store)
    })
}

fn primary_device_name() -> String {
    unsafe { monitor_device(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)) }
}

fn mutate(f: impl FnOnce(&mut Config)) {
    with_store(|st| {
        f(&mut st.cfg);
        st.stamp = write_cfg(&st.cfg);
        st.refresh();
    });
}

// True when config.toml was edited by hand since we last touched it; the new values are then reloaded.
pub(crate) fn reload_if_changed() -> bool {
    let Some(path) = config_file() else { return false };
    let now = stamp_of(&path);
    let changed = with_store(|st| now.is_some() && now != st.stamp);
    if !changed {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&path) else { return false };
    let Ok(cfg) = toml::from_str::<Config>(&text) else { return false };
    with_store(|st| {
        st.cfg = cfg;
        st.stamp = now;
        st.refresh();
    });
    true
}

// Pushes the current config into State. Layout is rebuilt by the next refresh.
pub(crate) fn apply_to_state() {
    let (pins, layout, icon_only, rules) = (load_pins(), load_order(), load_icon_only(), load_icon_rules());
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.pinned = pins;
        s.layout = layout;
        s.icon_only = icon_only;
        s.icon_rules = rules;
        s.saved_windows = load_windows();
    });
}
