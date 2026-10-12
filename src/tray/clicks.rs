use super::*;

// Moves an app's tray icons between the bar and the flyout.
pub(crate) fn set_overflow(exe: &str, overflow: bool) {
    let mut set = load_tray_overflow();
    if overflow {
        set.insert(exe.to_string());
    } else {
        set.remove(exe);
    }
    save_tray_overflow(&set);
    push_tray();
    for m in bar_keys() {
        push_ui(m);
    }
}

pub(crate) fn on_tray_clicked(monitor: isize, index: usize, button: i32) {
    crate::popup::hide_tip();
    let Some(target) = nth_target(index, false) else { return };
    dispatch_click(monitor, target, button);
}

pub(crate) fn on_flyout_clicked(monitor: isize, index: usize, button: i32) {
    let Some(target) = nth_target(index, true) else { return };
    if button == 0 {
        crate::popup::close_popups();
    }
    dispatch_click(monitor, target, button);
}

pub(super) fn shift_down() -> bool {
    unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000 != 0 }
}

// Shift+right click opens our own menu to move the icon between bar and flyout.
pub(super) fn dispatch_click(monitor: isize, target: Target, button: i32) {
    let (hwnd, uid, callback, version, exe, overflow) = target;
    if button == 1 && shift_down() {
        if !exe.is_empty() {
            let info = AppInfo { exe, pinned: overflow, ..AppInfo::default() };
            crate::popup::open_menu(monitor, vec![Box::new(TrayProvider)], info);
        }
        return;
    }
    send_click(hwnd, uid, callback, version, button);
}

pub(crate) struct TrayProvider;

impl MenuProvider for TrayProvider {
    fn applies_to(&self, _: &AppInfo) -> bool {
        true
    }
    fn items(&self, info: &AppInfo) -> Vec<MenuItem> {
        let label = if info.pinned { "Show in Taskbar" } else { "Move to Overflow" };
        vec![MenuItem { id: 1, label: label.into(), enabled: true }]
    }
    fn run(&self, _: usize, info: &AppInfo) {
        set_overflow(&info.exe, !info.pinned);
    }
}

// Sends a click to the icon's owner the way Explorer does (right = 1).
pub(super) fn send_click(hwnd: isize, uid: u32, callback: u32, version: u32, button: i32) {
    let hwnd = HWND(hwnd as *mut c_void);
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        CLICKED.with(|c| c.set((pid, std::time::Instant::now())));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        // Taking focus dismisses any menu another tray app left open.
        let _ = SetForegroundWindow(tray_hwnd());
        let post = |event: u32| {
            let (w, l) = if version >= 4 {
                let anchor = ((pt.y as u32 & 0xFFFF) << 16) | (pt.x as u32 & 0xFFFF);
                (anchor as usize, ((uid & 0xFFFF) << 16 | (event & 0xFFFF)) as isize)
            } else {
                (uid as usize, event as isize)
            };
            let _ = PostMessageW(Some(hwnd), callback, WPARAM(w), LPARAM(l));
        };
        if button == 0 {
            post(WM_LBUTTONDOWN);
            post(WM_LBUTTONUP);
            if version >= 4 {
                post(NIN_SELECT);
            }
        } else {
            post(WM_RBUTTONDOWN);
            post(WM_RBUTTONUP);
            if version >= 4 {
                post(WM_CONTEXTMENU);
            }
        }
    }
}
