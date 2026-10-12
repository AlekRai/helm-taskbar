//! Loaded into StartMenuExperienceHost.exe / SearchHost.exe by a WH_GETMESSAGE hook on each of
//! their threads. The shell asks the native Start menu to show or hide by posting message 0x270
//! to its CoreWindow. We turn that message into WM_NULL so the native menu never appears; the
//! taskbar app opens its own launcher by watching the Win key itself.

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, MSG, WM_NULL};

const WM_START_VISIBILITY: u32 = 0x270;

/// WH_GETMESSAGE hook procedure.
///
/// # Safety
/// Must only be called by the system as a hook procedure: when `code == 0` and `wparam != 0`,
/// `lparam` must point to a valid, writable `MSG`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn winkey_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code == 0 && wparam.0 != 0 {
            let msg = &mut *(lparam.0 as *mut MSG);
            if msg.message == WM_START_VISIBILITY {
                msg.message = WM_NULL;
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}
