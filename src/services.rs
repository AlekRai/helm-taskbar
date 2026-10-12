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

fn item(name: &str, kind: usize, rows: &[&webservices::Status]) -> SvcItem {
    SvcItem {
        name: name.into(),
        kind: kind as i32,
        count: rows.iter().map(|s| s.unread as i32).sum(),
        meeting: rows.iter().any(|s| s.meeting),
        flashing: rows.iter().any(|s| s.attention),
    }
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
pub(crate) fn members(kind: usize) -> Vec<(SvcItem, String)> {
    let all = webservices::status();
    let Some((key, _)) = GROUPS.get(kind) else { return Vec::new() };
    group_rows(&all, key).into_iter().map(|s| (item(&s.name, kind, &[s]), s.id.clone())).collect()
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
