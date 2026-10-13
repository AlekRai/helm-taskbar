//! Built-in web services (mail, chat) shown as two bar groups; the windows live in the `webservices` crate.
use crate::prelude::*;

// Width of one group button on the bar.
pub(crate) const SVC_BTN: f32 = 34.0;

const GROUPS: [(&str, &str); 2] = [("personal", "Personal"), ("work", "Work")];

// Starts every enabled service with a hidden window.
pub(crate) fn start() {
    let Some(base) = config_dir() else { return };
    webservices::init(base, load_svc().dev, log);
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
    ui.set_meet_color(slint::Color::from_rgb_u8(color.0, color.1, color.2).into());
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
    group_rows(&all, key).into_iter().map(|s| (row_item(kind, s, scale), s.id.clone())).collect()
}

// Fills a bar's group model (empty when this bar doesn't show them) and returns the width it takes.
pub(crate) fn sync_bar(ui: &BarWindow, on: bool) -> f32 {
    let items = if on { groups() } else { Vec::new() };
    let width = items.len() as f32 * SVC_BTN + sync_meeting(ui, on && !items.is_empty());
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

pub(crate) fn open(id: &str) {
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
            let fresh = SEEN.with(|seen| seen.borrow_mut().insert(m.url.clone()));
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
    group_rows(&all, key).get(index).map(|s| describe(s)).unwrap_or_default()
}
