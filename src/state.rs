use crate::prelude::*;

#[derive(Clone, PartialEq)]
pub(crate) struct Entry {
    // Null for a pinned app that isn't running.
    pub(crate) hwnd: HWND,
    pub(crate) title: String,
    pub(crate) exe: String,
    pub(crate) pid: u32,
    pub(crate) icon: HICON,
    pub(crate) pinned: bool,
}

pub(crate) struct Shown {
    pub(crate) key: String,
    pub(crate) entry: Entry,
    pub(crate) closing: bool,
    pub(crate) entered: bool,
    pub(crate) since: std::time::Instant,
}

pub(crate) struct Bar {
    pub(crate) monitor: isize,
    pub(crate) device: String,
    // Null until the Slint window has a native handle.
    pub(crate) hwnd: HWND,
    pub(crate) ui: BarWindow,
    pub(crate) entries: Vec<Entry>,
    // Rows of the Slint model, in order; includes buttons that are animating out.
    pub(crate) shown: Vec<Shown>,
    // Button centers (in layout order) from the last layout; used to find the drop slot.
    pub(crate) slots: Vec<(String, f32)>,
    // Active drag: dragged row key and the x it stays pinned to, plus the previewed order.
    pub(crate) drag: Option<(String, f32)>,
    pub(crate) drag_order: Option<Vec<String>>,
    pub(crate) model: Rc<VecModel<AppItem>>,
    pub(crate) foreground: HWND,
    pub(crate) scale: f32,
    pub(crate) rect: RECT,
    pub(crate) minute: u16,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) bars: HashMap<isize, Bar>,
    pub(crate) pinned: Vec<(String, String)>,
    pub(crate) layout: Vec<(String, String)>,
    pub(crate) icon_only: HashSet<String>,
    pub(crate) icon_rules: Vec<(String, String, String)>,
    pub(crate) file_icons: HashMap<String, HICON>,
    // Session order of windows of the same app (hwnd -> rank); also persisted in config.toml.
    pub(crate) win_rank: HashMap<isize, usize>,
    pub(crate) saved_windows: Vec<WinRec>,
    pub(crate) order: Vec<isize>,
    pub(crate) info_cache: HashMap<isize, (u32, String)>,
    pub(crate) icon_cache: HashMap<isize, HICON>,
    pub(crate) exe_icons: HashMap<String, HICON>,
    pub(crate) icon_images: HashMap<(isize, u32), Option<slint::Image>>,
}

thread_local! {
    pub(crate) static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(crate) fn with_bar<R>(monitor: isize, f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().bars.get_mut(&monitor).map(f))
}

pub(crate) fn bar_keys() -> Vec<isize> {
    STATE.with(|s| s.borrow().bars.keys().copied().collect())
}

// True for any window we own: bars and popups.
pub(crate) fn is_ours(h: HWND) -> bool {
    let bar = STATE.with(|s| s.borrow().bars.values().any(|b| b.hwnd == h));
    bar || popup_owns(h)
}
