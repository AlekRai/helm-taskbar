//! Built-in web services (mail, chat) shown as two bar groups; the windows live in the `webservices` crate.
use crate::prelude::*;

// Width of one group button on the bar.
pub(crate) const SVC_BTN: f32 = 34.0;

const GROUPS: [(&str, &str); 2] = [("personal", "Personal"), ("work", "Work")];

// All services share one browser profile ("main"): the old Google profile folder becomes it, so only the
// other profiles' services (Messenger, Outlook, Teams) need a new sign-in.
fn migrate_profiles(base: &std::path::Path) {
    let (old, new) = (base.join("web").join("google"), base.join("web").join("main"));
    if old.exists() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    }
    let list = load_services();
    if list.iter().any(|c| c.profile != "main") {
        save_services(list.into_iter().map(|c| webservices::ServiceCfg { profile: "main".into(), ..c }).collect());
    }
}

// Starts every enabled service with a hidden window.
pub(crate) fn start() {
    let Some(base) = config_dir() else { return };
    migrate_profiles(&base);
    webservices::init(base, load_svc().dev, load_svc().low_memory, log);
    push_opts();
    ACTIVE.with(|a| a.set(GROUPS.map(|(k, _)| group_visible(k))));
    start_active();
}

thread_local! {
    // Whether each group was in its active hours at the last tick.
    static ACTIVE: std::cell::Cell<[bool; 2]> = const { std::cell::Cell::new([true; 2]) };
}

// Hands the adapter-facing options to the web pages.
fn push_opts() {
    let o = load_svc();
    webservices::set_opts(webservices::Opts {
        stale_hours: o.stale_hours,
        teams_activity: o.teams_activity,
        teams_threads: o.teams_threads,
        poll_ms: o.poll_s.clamp(1, 120) * 1000,
        panel_widen: o.panel_widen,
    });
}

// Launches the services whose group is currently active (a scheduled group only runs in its hours).
pub(crate) fn start_active() {
    let cfgs: Vec<_> = load_services().into_iter().filter(|c| group_visible(&c.group)).collect();
    webservices::start(&cfgs, false);
}

// Called every tick: a scheduled group is closed completely (no processes) outside its hours and reopened in them.
pub(crate) fn schedule() {
    push_opts();
    let now = GROUPS.map(|(k, _)| group_visible(k));
    let before = ACTIVE.with(|a| a.replace(now));
    for (i, (key, _)) in GROUPS.iter().enumerate() {
        if before[i] == now[i] {
            continue;
        }
        if now[i] {
            start_active();
        } else {
            webservices::stop_group(key);
        }
    }
}

// Services with a mail list flash while any of their mails does; others use their attention flag.
fn flash_of(s: &webservices::Status) -> bool {
    if !s.flash || !load_svc().flash {
        false
    } else if s.mail.is_empty() {
        s.attention
    } else {
        FLASH.with(|f| s.mail.iter().any(|m| f.borrow().contains(&m.url)))
    }
}

fn item(name: &str, kind: usize, rows: &[&webservices::Status]) -> SvcItem {
    SvcItem {
        name: name.into(),
        kind: kind as i32,
        count: rows.iter().map(|s| s.unread as i32).sum(),
        meeting: rows.iter().any(|s| s.meeting),
        flashing: rows.iter().any(|s| flash_of(s)),
        icon: slint::Image::default(),
        has_icon: false,
    }
}

fn row_item(kind: usize, s: &webservices::Status, scale: f32) -> SvcItem {
    let mut it = item(&s.name, kind, &[s]);
    if s.icon != 0 {
        it.icon = cached_icon_image(HICON(s.icon as *mut c_void), scale);
        it.has_icon = true;
    }
    it
}

fn group_rows<'a>(all: &'a [webservices::Status], key: &str) -> Vec<&'a webservices::Status> {
    all.iter().filter(|s| s.group == key).collect()
}

// The scheduled group (the Work group by default) only shows on its days and hours.
pub(crate) fn group_visible(key: &str) -> bool {
    let o = load_svc();
    if key != o.sched_group {
        return true;
    }
    let t = unsafe { GetLocalTime() };
    let (from, to) = (o.sched_start, o.sched_end);
    let hour = t.wHour as u32;
    let in_hours = if from <= to { (from..to).contains(&hour) } else { hour >= from || hour < to };
    o.sched_days >> t.wDayOfWeek & 1 == 1 && in_hours
}

// The calendar event to show: the one in progress (`true`), else the next one today, with the seconds left.
fn meeting_now() -> Option<(webservices::CalEvent, i64, bool)> {
    if !group_visible("work") {
        return None;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let t = unsafe { GetLocalTime() };
    let day_end = now + 86400 - (t.wHour as i64 * 3600 + t.wMinute as i64 * 60 + t.wSecond as i64);
    let events = webservices::calendar();
    if let Some(e) = events.iter().find(|e| e.start <= now && now < e.end) {
        return Some((e.clone(), e.end - now, true));
    }
    let e = events.iter().find(|e| e.start > now && e.start < day_end)?;
    Some((e.clone(), e.start - now, false))
}

// Clicking the chip joins the meeting through its Teams link.
pub(crate) fn meeting_clicked() {
    if let Some((event, _, _)) = meeting_now() {
        webservices::meeting_action("join", &event);
    }
}

// Right click: join, or change the response. A declined or followed meeting drops off the bar.
pub(crate) fn meeting_right_clicked(monitor: isize) {
    if let Some((event, _, _)) = meeting_now() {
        crate::popup::open_menu(monitor, vec![Box::new(MeetProvider(event))], AppInfo::default());
    }
}

struct MeetProvider(webservices::CalEvent);

impl MenuProvider for MeetProvider {
    fn applies_to(&self, _: &AppInfo) -> bool {
        true
    }
    fn items(&self, _: &AppInfo) -> Vec<MenuItem> {
        let mark = |s: &str, label: &str| if self.0.status == s { format!("{label}  ✓") } else { label.to_string() };
        let item = |id, label: &str| MenuItem { id, label: label.into(), enabled: true };
        vec![
            item(1, "Join meeting"),
            item(2, &mark("accepted", "Accept")),
            item(3, &mark("tentative", "Tentative")),
            item(4, "Decline"),
            item(5, "Follow"),
        ]
    }
    fn run(&self, id: usize, _: &AppInfo) {
        let action = match id {
            1 => "join",
            2 => "accepted",
            3 => "tentative",
            5 => "follow",
            _ => "declined",
        };
        webservices::meeting_action(action, &self.0);
    }
}

// Remaining time: hours and minutes, then minutes, then minutes and seconds in the last ten.
fn remaining(secs: i64, left: bool) -> String {
    let secs = secs.max(0);
    let t = match secs {
        3600.. => format!("{}h {:02}m", secs / 3600, secs % 3600 / 60),
        600.. => format!("{}m", secs / 60),
        _ => format!("{}m {:02}s", secs / 60, secs % 60),
    };
    if left { format!("{t} left") } else { t }
}

// Shows the meeting countdown on the bar and returns the width it takes.
fn sync_meeting(ui: &BarWindow, on: bool) -> f32 {
    let Some((event, secs, current)) = on.then(meeting_now).flatten() else {
        if ui.get_meet_w() != 0.0 {
            ui.set_meet_w(0.0);
        }
        return 0.0;
    };
    let color = match (current, secs) {
        (true, _) => (0x3e, 0xcf, 0x6e),
        (_, 0..600) => (0xff, 0x45, 0x3a),
        (_, 600..3600) => (0xf5, 0xa5, 0x24),
        _ => (0x7a, 0xa2, 0xf7),
    };
    let time = remaining(secs, current);
    // Snapped so the width only changes now and then (the text itself ticks every second near the end).
    let snap = |w: f32| (w / 12.0).ceil() * 12.0;
    let time_w = snap(unsafe { crate::tasks::measure_text(&time) } * 1.25);
    let title_w = (unsafe { crate::tasks::measure_text(&event.title) } * 1.12 + 4.0).min(220.0);
    let total = time_w + 8.0 + title_w + 12.0;
    ui.set_meet_time(time.into());
    ui.set_meet_title(event.title.into());
    ui.set_meet_color(slint::Color::from_rgb_u8(color.0, color.1, color.2));
    ui.set_meet_time_w(time_w);
    ui.set_meet_title_w(title_w);
    ui.set_meet_w(total);
    total
}

// One button per group that has at least one running service.
pub(crate) fn groups() -> Vec<SvcItem> {
    let all = webservices::status();
    GROUPS
        .iter()
        .enumerate()
        .filter_map(|(k, (key, name))| {
            if !group_visible(key) {
                return None;
            }
            let rows = group_rows(&all, key);
            (!rows.is_empty()).then(|| item(name, k, &rows))
        })
        .collect()
}

// (row for the flyout, service id) for each service in a group.
pub(crate) fn members(kind: usize, scale: f32) -> Vec<(SvcItem, String)> {
    let all = webservices::status();
    let Some((key, _)) = GROUPS.get(kind) else { return Vec::new() };
    let mut rows: Vec<_> = group_rows(&all, key).into_iter().map(|s| (row_item(kind, s, scale), s.id.clone())).collect();
    if *key == "work" && !rows.is_empty() {
        rows.extend(rdp_rows(kind, scale));
    }
    rows
}

// Remote Desktop launchers: connect scripts run directly, listed under the Work group when present.
const RDP: [(&str, &str, &str); 2] = [
    ("rdp:all", "All monitors", "Connect-WorkPC.ps1"),
    ("rdp:one", "One monitor", "Connect-WorkPC-SingleMonitor.ps1"),
];

fn rdp_path(id: &str) -> Option<std::path::PathBuf> {
    let file = RDP.iter().find(|r| r.0 == id)?.2;
    let dir = load_svc().rdp_dir.clone();
    let dir = if dir.is_empty() {
        std::path::PathBuf::from(std::env::var_os("USERPROFILE")?).join("Desktop").join("Bentley").join("Remote Desktop")
    } else {
        std::path::PathBuf::from(dir)
    };
    let path = dir.join(file);
    path.exists().then_some(path)
}

fn rdp_item(name: &str, kind: usize, scale: f32) -> SvcItem {
    let icon = unsafe { exe_icon(r"C:\Windows\System32\mstsc.exe") };
    SvcItem {
        name: name.into(),
        kind: if kind == usize::MAX { -1 } else { kind as i32 },
        count: 0,
        meeting: false,
        flashing: false,
        icon: cached_icon_image(icon, scale),
        has_icon: !icon.0.is_null(),
    }
}

// A single "RDP" row; its options open in the side list like a service's unread mails.
fn rdp_rows(kind: usize, scale: f32) -> Vec<(SvcItem, String)> {
    if RDP.iter().all(|r| rdp_path(r.0).is_none()) {
        return Vec::new();
    }
    vec![(rdp_item("Remote Desktop", kind, scale), "rdp".to_string())]
}

fn rdp_options(scale: f32) -> Vec<(SvcItem, Option<String>)> {
    RDP.iter().filter(|r| rdp_path(r.0).is_some()).map(|r| (rdp_item(r.1, usize::MAX, scale), Some(r.0.to_string()))).collect()
}

// Fills a bar's group model (empty when this bar doesn't show them) and returns the width it takes.
pub(crate) fn sync_bar(ui: &BarWindow, on: bool) -> f32 {
    let items = if on { groups() } else { Vec::new() };
    let meet_w = sync_meeting(ui, on && !items.is_empty());
    let width = items.len() as f32 * SVC_BTN + meet_w;
    sync_strip(ui, &items, meet_w);
    let current = ui.get_svc();
    if current.row_count() == items.len() {
        for (i, it) in items.into_iter().enumerate() {
            if current.row_data(i).is_none_or(|r| r != it) {
                current.set_row_data(i, it);
            }
        }
    } else {
        ui.set_svc(ModelRc::new(VecModel::from(items)));
    }
    width
}

thread_local! {
    // The strip as last laid out, left to right: (key, width). Key -1 is the meeting chip, else the group kind.
    static STRIP: RefCell<Vec<(i32, f32)>> = const { RefCell::new(Vec::new()) };
    // Order being previewed while a button is dragged.
    static STRIP_DRAG: RefCell<Option<Vec<i32>>> = const { RefCell::new(None) };
}

// Orders the present buttons: the dragged or saved order first, anything unlisted after in default order.
fn strip_order(present: &[(i32, f32)]) -> Vec<(i32, f32)> {
    let saved = STRIP_DRAG.with(|d| d.borrow().clone()).unwrap_or_else(|| load_svc().bar_order.clone());
    let mut out: Vec<(i32, f32)> = saved.iter().filter_map(|k| present.iter().find(|p| p.0 == *k).copied()).collect();
    for p in present {
        if !out.iter().any(|o| o.0 == p.0) {
            out.push(*p);
        }
    }
    out
}

fn set_floats(model: ModelRc<f32>, values: Vec<f32>) -> ModelRc<f32> {
    if model.row_count() == values.len() {
        for (i, v) in values.into_iter().enumerate() {
            if model.row_data(i) != Some(v) {
                model.set_row_data(i, v);
            }
        }
        model
    } else {
        ModelRc::new(VecModel::from(values))
    }
}

// Positions the meeting chip and group buttons in the user's order, measured from the strip's right end.
fn sync_strip(ui: &BarWindow, items: &[SvcItem], meet_w: f32) {
    let mut present: Vec<(i32, f32)> = Vec::new();
    if meet_w > 0.0 {
        present.push((-1, meet_w));
    }
    present.extend(items.iter().map(|i| (i.kind, SVC_BTN)));
    let order = strip_order(&present);
    let mut from_right = std::collections::HashMap::new();
    let mut acc = 0.0;
    for (k, w) in order.iter().rev() {
        from_right.insert(*k, acc);
        acc += w;
    }
    let pos: Vec<f32> = items.iter().map(|i| from_right.get(&i.kind).copied().unwrap_or(0.0)).collect();
    ui.set_svc_pos(set_floats(ui.get_svc_pos(), pos));
    ui.set_meet_pos(from_right.get(&-1).copied().unwrap_or(0.0));
    ui.set_strip_w(acc);
    STRIP.with(|s| *s.borrow_mut() = order);
}

// Live preview while a strip button is dragged; `center` is its distance from the strip's right end.
pub(crate) fn strip_drag_moved(monitor: isize, key: i32, center: f32) {
    let layout = STRIP.with(|s| s.borrow().clone());
    let mut acc = 0.0;
    let mut centers: Vec<(i32, f32)> = Vec::new();
    for (k, w) in layout.iter().rev() {
        centers.push((*k, acc + w / 2.0));
        acc += w;
    }
    let Some(&(target, _)) = centers.iter().min_by(|a, b| (a.1 - center).abs().total_cmp(&(b.1 - center).abs())) else { return };
    let mut list: Vec<i32> = layout.iter().map(|p| p.0).collect();
    let (Some(f), Some(t)) = (list.iter().position(|k| *k == key), list.iter().position(|k| *k == target)) else { return };
    if f == t {
        return;
    }
    let k = list.remove(f);
    list.insert(t, k);
    STRIP_DRAG.with(|d| *d.borrow_mut() = Some(list));
    crate::tasks::push_ui(monitor);
}

// Drop: saves the previewed order.
pub(crate) fn strip_dropped(monitor: isize, key: i32, center: f32) {
    strip_drag_moved(monitor, key, center);
    if let Some(list) = STRIP_DRAG.with(|d| d.borrow_mut().take()) {
        update_svc(|s| s.bar_order = list);
    }
    crate::tasks::push_ui(monitor);
}

pub(crate) fn open(id: &str) {
    if id.starts_with("rdp:") {
        if let Some(path) = rdp_path(id) {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(path)
                .creation_flags(0x0800_0000)
                .spawn();
        }
        return;
    }
    webservices::open(id);
}

pub(crate) fn open_url(id: &str, url: &str) {
    webservices::open_url(id, url);
}

pub(crate) fn open_panel(id: &str, url: &str, rect: (i32, i32, i32, i32)) {
    webservices::open_panel(id, url, rect);
}

thread_local! {
    // Mails just marked read: hidden from the list at once, shown again if still unread after a minute.
    static MARKED: std::cell::RefCell<std::collections::HashMap<String, std::time::Instant>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

thread_local! {
    // Mail urls ever listed, those new since start-up that still flash, and when each service was first seen.
    static SEEN: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::new(std::collections::HashSet::new());
    static FLASH: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::new(std::collections::HashSet::new());
    static COUNTS: std::cell::RefCell<std::collections::HashMap<String, i64>> = std::cell::RefCell::new(std::collections::HashMap::new());
    static FIRST: std::cell::RefCell<std::collections::HashMap<String, std::time::Instant>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

// Mails already there at start-up (first feed reads) don't count as new.

// Called every tick: marks mails that appeared since start-up as flashing.
pub(crate) fn track_mail() {
    let settle = std::time::Duration::from_secs(load_svc().flash_settle_s as u64);
    let mut chime = false;
    let all = webservices::status();
    // Mails that are no longer unread stop flashing.
    FLASH.with(|f| f.borrow_mut().retain(|u| all.iter().any(|s| s.mail.iter().any(|m| m.url == *u))));
    for s in all {
        let settled = FIRST.with(|f| f.borrow_mut().entry(s.id.clone()).or_insert_with(std::time::Instant::now).elapsed() > settle);
        for m in &s.mail {
            let mut fresh = SEEN.with(|seen| seen.borrow_mut().insert(m.url.clone()));
            // A chat that already has unread messages sounds again for each newer last message.
            if s.id.contains("messenger") {
                let key = format!("{}\n{}", m.url, m.preview);
                fresh |= SEEN.with(|seen| seen.borrow_mut().insert(key));
            }
            if fresh && settled {
                FLASH.with(|f| f.borrow_mut().insert(m.url.clone()));
                chime = true;
            }
        }
        // Any rise in a service's unread count is a notification too (covers services without a mail list and re-unread mail).
        let prev = COUNTS.with(|c| c.borrow_mut().insert(s.id.clone(), s.unread as i64));
        if settled && prev.is_some_and(|p| (s.unread as i64) > p) {
            chime = true;
        }
    }
    if chime && load_svc().sound {
        notify_sound();
    }
}

// Plays the custom sound if one is set, else the Windows notification sound.
pub(crate) fn notify_sound() {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
    use windows::Win32::Media::Multimedia::mciSendStringW;
    let file = load_svc().sound_file.clone();
    let path = crate::config::config_dir().unwrap_or_default().join(&file);
    if !file.is_empty() && path.is_file() {
        let cmd = |s: String| {
            let w = to_wide(&s);
            unsafe { mciSendStringW(PCWSTR(w.as_ptr()), None, None) }
        };
        cmd("close helmsound".into());
        if cmd(format!("open \"{}\" alias helmsound", path.display())) == 0 {
            cmd("play helmsound from 0".into());
            return;
        }
    }
    unsafe {
        let _ = PlaySoundW(w!("Notification.Default"), None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
    }
}

// Mouse over a flashing mail stops its flash.
pub(crate) fn mail_seen(_id: &str, url: &str) {
    FLASH.with(|f| f.borrow_mut().remove(url));
}

const MARKED_HIDE: std::time::Duration = std::time::Duration::from_secs(60);

pub(crate) fn mark_read_pending(url: &str) {
    MARKED.with(|m| m.borrow_mut().insert(url.to_owned(), std::time::Instant::now()));
}

// Unread mails of a service for the mail list, each with the url that opens it.
pub(crate) fn mail_rows(id: &str, scale: f32) -> Vec<(SvcItem, Option<String>)> {
    if id == "rdp" {
        return rdp_options(scale);
    }
    let all = webservices::status();
    let Some(s) = all.iter().find(|s| s.id == id) else { return Vec::new() };
    if s.mail.is_empty() {
        return Vec::new();
    }
    let base = row_item(0, s, scale);
    let mut rows = Vec::new();
    MARKED.with(|m| m.borrow_mut().retain(|_, t| t.elapsed() < MARKED_HIDE));
    let max = load_svc().mail_max.max(1) as usize;
    for m in s.mail.iter().filter(|m| !MARKED.with(|h| h.borrow().contains_key(&m.url))).take(max) {
        let name = if m.from.is_empty() || m.title.is_empty() { if m.title.is_empty() { m.from.clone() } else { m.title.clone() } } else { format!("{} \u{2014} {}", m.from, m.title) };
        rows.push((SvcItem { name: name.into(), count: 0, flashing: FLASH.with(|f| f.borrow().contains(&m.url)), meeting: false, ..base.clone() }, Some(m.url.clone())));
    }
    rows
}

// Tooltip for the n-th listed mail: sender, subject and a preview of the body.
pub(crate) fn mail_tip(id: &str, index: usize) -> String {
    if !load_svc().mail_preview {
        return String::new();
    }
    let Some(url) = mail_rows(id, 1.0).get(index).and_then(|r| r.1.clone()) else { return String::new() };
    let all = webservices::status();
    let Some(m) = all.iter().find(|s| s.id == id).and_then(|s| s.mail.iter().find(|m| m.url == url)) else { return String::new() };
    let head = if m.from.is_empty() || m.title.is_empty() { format!("{}{}", m.from, m.title) } else { format!("{}\n{}", m.from, m.title) };
    if m.preview.is_empty() { head } else { format!("{head}\n\n{}", m.preview) }
}

fn describe(s: &webservices::Status) -> String {
    let meeting = if s.meeting { " (in a meeting)" } else { "" };
    match s.unread {
        0 => format!("{}: no unread{meeting}", s.name),
        n => format!("{}: {n} unread{meeting}", s.name),
    }
}

// Tooltip for a bar group button: one line per service.
pub(crate) fn group_tip(kind: usize) -> String {
    let Some((key, name)) = GROUPS.get(kind) else { return String::new() };
    let all = webservices::status();
    let lines: Vec<String> = group_rows(&all, key).into_iter().map(describe).collect();
    format!("{name}\n{}", lines.join("\n"))
}

pub(crate) fn row_tip(kind: usize, index: usize) -> String {
    let Some((key, _)) = GROUPS.get(kind) else { return String::new() };
    let all = webservices::status();
    let rows = group_rows(&all, key);
    match rows.get(index) {
        Some(s) => describe(s),
        None => "Remote Desktop".to_string(),
    }
}
