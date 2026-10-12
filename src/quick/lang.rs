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

// Keyboard layout of the focused app (the popups never take focus).
pub(super) fn current_layout() -> HKL {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = if fg.0.is_null() { 0 } else { GetWindowThreadProcessId(fg, None) };
        GetKeyboardLayout(tid)
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
    use windows::Win32::UI::WindowsAndMessaging::{GUITHREADINFO, GetGUIThreadInfo};
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gti = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let _ = GetGUIThreadInfo(tid, &mut gti);
        // WM_INPUTLANGCHANGEREQUEST; some apps only honour it on the focused child.
        for h in [fg, gti.hwndFocus, gti.hwndActive] {
            if !h.0.is_null() {
                let _ = PostMessageW(Some(h), 0x0050, WPARAM(0), LPARAM(hkl.0 as isize));
            }
        }
    }
}

