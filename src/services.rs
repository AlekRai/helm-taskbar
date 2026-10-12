//! Built-in web services (mail, chat) shown as two bar groups; the windows live in the `webservices` crate.
use crate::prelude::*;

// Width of one group button on the bar.
pub(crate) const SVC_BTN: f32 = 34.0;

const GROUPS: [(&str, &str); 2] = [("personal", "Personal"), ("work", "Work")];

// Starts every enabled service with a hidden window.
pub(crate) fn start() {
    let Some(base) = config_dir() else { return };
    webservices::init(base, false, log);
    webservices::start(&load_services(), false);
}

// Services with a mail list flash while any of their mails does; others use their attention flag.
fn flash_of(s: &webservices::Status) -> bool {
    if s.mail.is_empty() {
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

// One button per group that has at least one running service.
pub(crate) fn groups() -> Vec<SvcItem> {
    let all = webservices::status();
    GROUPS
        .iter()
        .enumerate()
        .filter_map(|(k, (key, name))| {
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
    let width = items.len() as f32 * SVC_BTN;
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
    static FIRST: std::cell::RefCell<std::collections::HashMap<String, std::time::Instant>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

// Mails already there at start-up (first feed reads) don't count as new.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(20);

// Called every tick: marks mails that appeared since start-up as flashing.
pub(crate) fn track_mail() {
    for s in webservices::status() {
        let settled = FIRST.with(|f| f.borrow_mut().entry(s.id.clone()).or_insert_with(std::time::Instant::now).elapsed() > SETTLE);
        for m in &s.mail {
            let fresh = SEEN.with(|seen| seen.borrow_mut().insert(m.url.clone()));
            if fresh && settled {
                crate::util::log(&format!("mail flash: {} {}", s.id, m.title));
                FLASH.with(|f| f.borrow_mut().insert(m.url.clone()));
            }
        }
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
    for m in s.mail.iter().filter(|m| !MARKED.with(|h| h.borrow().contains_key(&m.url))) {
        let name = if m.from.is_empty() { m.title.clone() } else { format!("{} \u{2014} {}", m.from, m.title) };
        rows.push((SvcItem { name: name.into(), count: 0, flashing: FLASH.with(|f| f.borrow().contains(&m.url)), meeting: false, ..base.clone() }, Some(m.url.clone())));
    }
    rows
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
