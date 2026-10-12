use crate::prelude::*;
use std::os::windows::process::CommandExt;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::{CreateMutexW, INFINITE, PROCESS_SYNCHRONIZE, WaitForSingleObject};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("rust_taskbar");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn exe_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

pub(crate) fn is_enabled() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key).is_err() {
            return false;
        }
        let found = RegQueryValueExW(key, VALUE, None, None, None, None).is_ok();
        let _ = RegCloseKey(key);
        found
    }
}

pub(crate) fn set_enabled(on: bool) {
    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None).is_err() {
            return;
        }
        if on {
            if let Some(cmd) = exe_command() {
                let wide = to_wide(&cmd);
                let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                if let Err(e) = RegSetValueExW(key, VALUE, None, REG_SZ, Some(bytes)).ok() {
                    log(&format!("autostart: could not write the Run key: {e}"));
                }
            }
        } else {
            let _ = RegDeleteValueW(key, VALUE);
        }
        let _ = RegCloseKey(key);
    }
}

// First run turns autostart on; afterwards the user's choice wins. An enabled entry is re-pointed at the current exe.
pub(crate) fn init() {
    let marker = std::env::var_os("APPDATA")
        .map(|a| std::path::PathBuf::from(a).join("rust_taskbar").join("autostart_initialized"));
    if let Some(marker) = marker
        && !marker.exists() {
            if let Some(dir) = marker.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&marker, "");
            set_enabled(true);
            return;
        }
    if is_enabled() {
        set_enabled(true);
    }
}

pub(crate) fn spawn_watchdog() -> Option<std::process::Child> {
    let exe = std::env::current_exe().ok()?;
    std::process::Command::new(exe)
        .arg("--watchdog")
        .arg(std::process::id().to_string())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()
}

// Runs in the guardian process: when the taskbar dies without a clean exit, bring Explorer's taskbar back.
pub(crate) fn run_watchdog(pid: u32) {
    unsafe {
        if let Some(h) = open_process(PROCESS_SYNCHRONIZE, pid) {
            WaitForSingleObject(h.0, INFINITE);
        }
        restore_native_taskbars();
        broadcast_taskbar_created_public();
    }
}

// Returns false if another instance already owns the mutex.
pub(crate) fn acquire_single_instance() -> bool {
    unsafe {
        let Ok(h) = CreateMutexW(None, true, w!("Local\\rust_taskbar_single_instance")) else { return true };
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        if already {
            let _ = CloseHandle(h);
        }
        !already
    }
}
