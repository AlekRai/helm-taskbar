use super::*;

// ---------- Win key ----------
// The keyboard hook swallows Win so the shell never sees it (no native Start, no SearchHost
// activation). A lone Win press toggles our launcher. If another key arrives while Win is held,
// Win is replayed (marked with MARK) followed by that key, so every Win combo still works.
// winkey_hook.dll additionally blocks the native Start host's show message as a safety net.

// Tags our own injected key events so the hook ignores them.
pub(super) const MARK: usize = 0x5441;
pub(super) static WIN_DOWN: AtomicBool = AtomicBool::new(false);
pub(super) static COMBO: AtomicBool = AtomicBool::new(false);
pub(super) static WIN_VK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// Replays keys from another thread: injecting inside a low-level hook would deadlock it.
pub(super) fn replay(keys: Vec<(u32, bool, bool)>) {
    std::thread::spawn(move || unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        };
        for (vk, up, ext) in keys {
            let mut fl = KEYBD_EVENT_FLAGS(0);
            if up {
                fl |= KEYEVENTF_KEYUP;
            }
            if ext {
                fl |= KEYEVENTF_EXTENDEDKEY;
            }
            keybd_event(vk as u8, 0, fl, MARK);
        }
    });
}

pub(super) unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if k.dwExtraInfo == MARK {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let is_win = k.vkCode == VK_LWIN.0 as u32 || k.vkCode == VK_RWIN.0 as u32;
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let ext = k.flags.0 & 1 != 0;
            if is_win {
                HOOK_WIN_EVENTS.fetch_add(1, Ordering::SeqCst);
                if down {
                    if !WIN_DOWN.swap(true, Ordering::SeqCst) {
                        COMBO.store(false, Ordering::SeqCst);
                        WIN_VK.store(k.vkCode, Ordering::SeqCst);
                    }
                } else if WIN_DOWN.swap(false, Ordering::SeqCst) {
                    if COMBO.load(Ordering::SeqCst) {
                        replay(vec![(WIN_VK.load(Ordering::SeqCst), true, true)]);
                    } else {
                        let _ = slint::invoke_from_event_loop(toggle_start_at_cursor);
                    }
                }
                return LRESULT(1);
            } else if down && WIN_DOWN.load(Ordering::SeqCst) && k.flags.0 & 0x10 == 0
                && !COMBO.swap(true, Ordering::SeqCst) {
                    replay(vec![(WIN_VK.load(Ordering::SeqCst), false, true), (k.vkCode, false, ext)]);
                    return LRESULT(1);
                }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}
pub(super) fn toggle_start_at_cursor() {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    let under = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) }.0 as isize;
    let monitor = if bar_keys().contains(&under) { under } else { primary_monitor() };
    toggle_start(monitor);
}

pub(super) static HOOKS: std::sync::Mutex<Vec<(u32, isize)>> = std::sync::Mutex::new(Vec::new());

// Hooks every thread of the native Start host with winkey_hook.dll. Re-run periodically so a
// restarted host (new threads) is covered too.
pub(super) fn inject_win_key() {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, UnhookWindowsHookEx, WH_GETMESSAGE};
    use windows::core::s;
    unsafe {
        let mut pids = Vec::new();
        for title in [w!("Start"), w!("Search")] {
            if let Ok(win) = FindWindowW(w!("Windows.UI.Core.CoreWindow"), title) {
                let mut pid = 0u32;
                if GetWindowThreadProcessId(win, Some(&mut pid)) != 0 && pid != 0 {
                    pids.push(pid);
                }
            }
        }
        if pids.is_empty() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let dll = exe.with_file_name("winkey_hook.dll");
        let wide = to_wide(&dll.to_string_lossy());
        let Ok(module) = LoadLibraryW(PCWSTR(wide.as_ptr())) else {
            log("start: could not load winkey_hook.dll");
            return;
        };
        let Some(proc) = GetProcAddress(module, s!("winkey_proc")) else { return };
        let proc: HOOKPROC = Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT>(proc));
        let mut tids = Vec::new();
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) {
            let mut e = THREADENTRY32 { dwSize: size_of::<THREADENTRY32>() as u32, ..Default::default() };
            if Thread32First(snap, &mut e).is_ok() {
                loop {
                    if pids.contains(&e.th32OwnerProcessID) {
                        tids.push(e.th32ThreadID);
                    }
                    if Thread32Next(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }

        let mut hooks = HOOKS.lock().unwrap();
        hooks.retain(|(tid, h)| {
            if tids.contains(tid) {
                true
            } else {
                let _ = UnhookWindowsHookEx(HHOOK(*h as *mut _));
                false
            }
        });
        for tid in tids {
            if hooks.iter().any(|(t, _)| *t == tid) {
                continue;
            }
            if let Ok(h) = SetWindowsHookExW(WH_GETMESSAGE, proc, Some(HINSTANCE(module.0)), tid) {
                hooks.push((tid, h.0 as isize));
            }
        }
    }
}

// SearchHost's hidden Search window grabs foreground after our launcher closes, which breaks
// focus on the next open. Nothing else here needs it, so keep it suspended while we run.
pub(super) static SUSPENDED_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(super) fn search_host_pid() -> Option<u32> {
    unsafe {
        let win = FindWindowW(w!("Windows.UI.Core.CoreWindow"), w!("Search")).ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(win, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

pub(super) unsafe fn nt_process_call(pid: u32, func: windows::core::PCSTR) {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Threading::PROCESS_SUSPEND_RESUME;
    unsafe {
        let Ok(ntdll) = LoadLibraryW(w!("ntdll.dll")) else { return };
        let Some(f) = GetProcAddress(ntdll, func) else { return };
        let f: unsafe extern "system" fn(HANDLE) -> i32 = std::mem::transmute(f);
        if let Some(h) = open_process(PROCESS_SUSPEND_RESUME, pid) {
            f(h.0);
        }
    }
}

pub(super) fn suspend_search_host() {
    let Some(pid) = search_host_pid() else { return };
    if SUSPENDED_PID.swap(pid, Ordering::SeqCst) != pid {
        unsafe { nt_process_call(pid, windows::core::s!("NtSuspendProcess")) };
    }
}

pub(crate) fn resume_search_host() {
    if let Some(pid) = search_host_pid() {
        unsafe { nt_process_call(pid, windows::core::s!("NtResumeProcess")) };
    }
    SUSPENDED_PID.store(0, Ordering::SeqCst);
}

pub(crate) fn install_win_key_hook() {
    grant_app_container_access();
    inject_win_key();
    suspend_search_host();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(3), || {
        inject_win_key();
        suspend_search_host();
    });
    std::mem::forget(timer);

    // Own thread so a busy UI thread can never make the hook time out.
    std::thread::spawn(|| unsafe {
        let module = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).ok();
        let _ = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    // Windows skips the keyboard hook for some Win presses (seen while our launcher is focused),
    // so also watch the physical key state and toggle when the hook missed a press.
    let poll = slint::Timer::default();
    let state = std::cell::Cell::new((false, 0u32));
    poll.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(15), move || unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        let down = (GetAsyncKeyState(VK_LWIN.0 as i32) | GetAsyncKeyState(VK_RWIN.0 as i32)) as u16 & 0x8000 != 0;
        let (was_down, seen) = state.get();
        let now = HOOK_WIN_EVENTS.load(Ordering::SeqCst);
        if down && !was_down {
            state.set((true, now));
        } else if !down && was_down {
            state.set((false, now));
            // Hook never saw this press (Windows skips it while our window is focused): toggle ourselves.
            if seen == now {
                toggle_start_at_cursor();
            }
        }
    });
    std::mem::forget(poll);
}
pub(super) static HOOK_WIN_EVENTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// The Start host is a sandboxed app: it can only load our DLL if app packages may read it.
pub(super) fn grant_app_container_access() {
    use std::os::windows::process::CommandExt;
    if let Ok(exe) = std::env::current_exe() {
        let dll = exe.with_file_name("winkey_hook.dll");
        let _ = std::process::Command::new("icacls")
            .arg(dll)
            .args(["/grant", "*S-1-15-2-1:(RX)", "*S-1-15-2-2:(RX)"])
            .creation_flags(0x0800_0000)
            .status();
    }
}

// Gives keyboard focus to a window even though another process owns the foreground.
pub(crate) unsafe fn force_foreground(hwnd: HWND) {
    unsafe {
        let fg = GetForegroundWindow();
        if let Ok(s) = FindWindowW(w!("Windows.UI.Core.CoreWindow"), w!("Search"))
            && s == fg {
                let _ = ShowWindow(s, SW_HIDE);
            }
        let fg = GetForegroundWindow();
        let theirs = GetWindowThreadProcessId(fg, None);
        let ours = GetCurrentThreadId();
        if theirs != 0 && theirs != ours {
            let _ = AttachThreadInput(theirs, ours, true);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(theirs, ours, false);
        } else {
            let _ = SetForegroundWindow(hwnd);
        }
        if GetForegroundWindow() != hwnd {
            // A synthetic key press makes us the last-input process, which lifts the foreground lock.
            keybd_event(0x12, 0, KEYBD_EVENT_FLAGS(0), MARK);
            keybd_event(0x12, 0, KEYEVENTF_KEYUP, MARK);
            let _ = SetForegroundWindow(hwnd);
        }
        if GetForegroundWindow() != hwnd {
            SwitchToThisWindow(hwnd, true);
        }
    }
}
