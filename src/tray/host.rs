use super::*;

// Windows of the process whose tray icon was just clicked are its menu; keep them above the bar.
pub(crate) fn is_recent_tray_popup(hwnd: HWND) -> bool {
    let (pid, at) = CLICKED.with(|c| c.get());
    if pid == 0 || at.elapsed().as_secs() > 60 {
        return false;
    }
    let mut p = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut p)) };
    p == pid
}

pub(crate) fn tray_hwnd() -> HWND {
    HWND(TRAY_HWND.with(|h| h.get()) as *mut c_void)
}

// Explorer's own tray window, i.e. the Shell_TrayWnd that isn't ours.
pub(crate) fn explorer_tray() -> Option<HWND> {
    let ours = tray_hwnd();
    let mut prev: Option<HWND> = None;
    unsafe {
        while let Ok(h) = FindWindowExW(None, prev, w!("Shell_TrayWnd"), PCWSTR::null()) {
            if h != ours {
                return Some(h);
            }
            prev = Some(h);
        }
    }
    None
}

pub(super) fn broadcast_taskbar_created() {
    unsafe {
        let msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        if msg != 0 {
            let _ = SendNotifyMessageW(HWND_BROADCAST, msg, WPARAM(0), LPARAM(0));
        }
    }
}

pub(crate) fn broadcast_taskbar_created_public() {
    broadcast_taskbar_created();
}

pub(crate) fn init_tray() {
    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else { return };
        let class = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(tray_proc),
            hInstance: module.into(),
            lpszClassName: w!("Shell_TrayWnd"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return;
        }
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Shell_TrayWnd"),
            w!(""),
            WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            GetSystemMetrics(SM_CXSCREEN),
            23,
            None,
            None,
            Some(module.into()),
            None,
        ) else {
            return;
        };
        TRAY_HWND.with(|h| h.set(hwnd.0 as isize));
        SHELLHOOK_MSG.with(|m| m.set(RegisterWindowMessageW(w!("SHELLHOOK"))));
        if !RegisterShellHookWindow(hwnd).as_bool() {
            log("tray: RegisterShellHookWindow failed");
        }
        init_task_hook(hwnd, module);
        raise_tray();
        // Apps re-add their icons when they see this, which sends them to us.
        broadcast_taskbar_created();
    }
}

pub(crate) fn shutdown_tray() {
    let hwnd = tray_hwnd();
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    TRAY_HWND.with(|h| h.set(0));
    // Let apps re-register with Explorer's tray.
    broadcast_taskbar_created();
}

// FindWindow("Shell_TrayWnd") returns the topmost match, so we have to stay above Explorer's.
pub(crate) fn raise_tray() {
    let ours = tray_hwnd();
    if ours.0.is_null() {
        return;
    }
    unsafe {
        let first = FindWindowW(w!("Shell_TrayWnd"), None).unwrap_or_default();
        // Apps position their menus from this window's rect, so it must match the primary bar.
        let target = with_bar(primary_monitor(), |b| b.rect);
        let mut cur = RECT::default();
        let _ = GetWindowRect(ours, &mut cur);
        let moved = target.is_some_and(|r| r != cur);
        if first != ours || moved {
            let (x, y, w, h, flags) = match target {
                Some(r) => (r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE),
                None => (0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE),
            };
            let _ = SetWindowPos(ours, Some(HWND_TOPMOST), x, y, w, h, flags);
        }
        if let Some(e) = explorer_tray() {
            let _ = SetWindowPos(e, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

