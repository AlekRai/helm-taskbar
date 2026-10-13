use super::*;

// ---------- keyboard language ----------

pub(super) fn layouts() -> Vec<HKL> {
    unsafe {
        let n = GetKeyboardLayoutList(None);
        if n <= 0 {
            return Vec::new();
        }
        let mut v = vec![HKL::default(); n as usize];
        let got = GetKeyboardLayoutList(Some(&mut v));
        v.truncate(got.max(0) as usize);
        v
    }
}

pub(super) fn locale_text(hkl: HKL, kind: u32) -> String {
    let lang = (hkl.0 as usize & 0xFFFF) as u32;
    let mut buf = [0u16; 96];
    let n = unsafe { GetLocaleInfoW(lang, kind, Some(&mut buf)) };
    if n <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize - 1])
}

thread_local! {
    // The one layout all apps share, the foreground window last seen, and when a switch we asked for settles.
    static GLOBAL_LAYOUT: Cell<isize> = const { Cell::new(0) };
    static LAST_FG: Cell<isize> = const { Cell::new(0) };
    static SETTLE: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
}

// While the foreground is the desktop, the shell or Helm itself (e.g. while a window is being minimized)
// its layout is meaningless and is ignored.
fn is_neutral(fg: HWND) -> bool {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(fg, Some(&mut pid)) };
    if pid == unsafe { GetCurrentProcessId() } {
        return true;
    }
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(fg, &mut buf) } as usize;
    matches!(
        String::from_utf16_lossy(&buf[..n]).as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW" | "XamlExplorerHostIslandWindow"
    )
}

// The keyboard language is system-wide: a language picked in any app (or here) is applied to every app
// as it comes to the front. Returns that shared layout (the popups never take focus).
pub(super) fn current_layout() -> HKL {
    unsafe {
        let global = GLOBAL_LAYOUT.with(Cell::get);
        let fg = GetForegroundWindow();
        if fg.0.is_null() || is_neutral(fg) {
            if global != 0 {
                return HKL(global as *mut c_void);
            }
            return GetKeyboardLayout(0);
        }
        let hkl = GetKeyboardLayout(GetWindowThreadProcessId(fg, None));
        let settling = SETTLE.with(Cell::get).is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(1500));
        let fresh = LAST_FG.with(|f| f.replace(fg.0 as isize)) != fg.0 as isize;
        if global == 0 {
            GLOBAL_LAYOUT.with(|g| g.set(hkl.0 as isize));
        } else if fresh && hkl.0 as isize != global {
            SETTLE.with(|s| s.set(Some(std::time::Instant::now())));
            post_layout(fg, HKL(global as *mut c_void));
            return HKL(global as *mut c_void);
        } else if settling {
            return HKL(global as *mut c_void);
        } else {
            GLOBAL_LAYOUT.with(|g| g.set(hkl.0 as isize));
        }
        hkl
    }
}

// Asks a window (and its focused child, which is where some apps listen) to change layout.
fn post_layout(fg: HWND, hkl: HKL) {
    use windows::Win32::UI::WindowsAndMessaging::{GUITHREADINFO, GetGUIThreadInfo};
    unsafe {
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gti = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let _ = GetGUIThreadInfo(tid, &mut gti);
        for h in [fg, gti.hwndFocus, gti.hwndActive] {
            if !h.0.is_null() {
                let _ = PostMessageW(Some(h), 0x0050, WPARAM(0), LPARAM(hkl.0 as isize));
            }
        }
    }
}

// Pushes the language code to every bar (called on a fast timer so Win+Space and Alt+Shift show at once).
pub(crate) fn sync_lang() {
    let code = SharedString::from(bar_lang());
    for m in bar_keys() {
        with_bar(m, |b| {
            if b.ui.get_lang() != code {
                b.ui.set_lang(code.clone());
            }
        });
    }
}

// Code of the active keyboard language, empty when only one is installed.
pub(crate) fn bar_lang() -> String {
    if layouts().len() < 2 { String::new() } else { lang_code(current_layout()) }
}

pub(super) fn lang_code(hkl: HKL) -> String {
    // LOCALE_SISO639LANGNAME
    locale_text(hkl, 0x59).to_uppercase()
}

// True when a wired adapter with a gateway is up (ignores virtual adapters without one).
pub(crate) fn lang_clicked(monitor: isize) {
    if layouts().len() > 2 {
        open_language_menu(monitor);
    } else {
        cycle_language();
    }
}

pub(super) struct LangProvider(Vec<(isize, String)>, isize);

impl crate::menu::MenuProvider for LangProvider {
    fn applies_to(&self, _: &crate::menu::AppInfo) -> bool {
        true
    }
    fn items(&self, _: &crate::menu::AppInfo) -> Vec<crate::menu::MenuItem> {
        self.0
            .iter()
            .enumerate()
            .map(|(i, (h, name))| crate::menu::MenuItem {
                id: i + 1,
                label: if *h == self.1 { format!("✓  {name}") } else { format!("     {name}") },
                enabled: true,
            })
            .collect()
    }
    fn run(&self, id: usize, _: &crate::menu::AppInfo) {
        if let Some((h, _)) = self.0.get(id.wrapping_sub(1)) {
            switch_language(HKL(*h as *mut c_void));
        }
    }
}

pub(crate) fn open_language_menu(monitor: isize) {
    let ls = layouts();
    if ls.is_empty() {
        return;
    }
    let cur = current_layout().0 as isize;
    let entries = ls
        .iter()
        .map(|h| (h.0 as isize, format!("{} ({})", locale_text(*h, 0x1001), lang_code(*h))))
        .collect();
    crate::popup::open_menu(monitor, vec![Box::new(LangProvider(entries, cur))], Default::default());
}

pub(crate) fn cycle_language() {
    let ls = layouts();
    if ls.len() < 2 {
        return;
    }
    let cur = current_layout();
    let i = ls.iter().position(|h| *h == cur).map_or(0, |i| (i + 1) % ls.len());
    switch_language(ls[i]);
}

pub(super) fn switch_language(hkl: HKL) {
    let fg = unsafe { GetForegroundWindow() };
    if fg.0.is_null() {
        return;
    }
    GLOBAL_LAYOUT.with(|g| g.set(hkl.0 as isize));
    SETTLE.with(|s| s.set(Some(std::time::Instant::now())));
    post_layout(fg, hkl);
}

