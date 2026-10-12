use super::*;

// Windows asking for attention. Cleared once they become the foreground window.
pub(crate) fn is_flashing(hwnd: HWND, foreground: HWND) -> bool {
    FLASHING.with(|f| {
        let mut f = f.borrow_mut();
        if hwnd == foreground {
            f.remove(&(hwnd.0 as isize));
            return false;
        }
        f.contains(&(hwnd.0 as isize))
    })
}

pub(crate) fn forget_flashing_dead() {
    FLASHING.with(|f| f.borrow_mut().retain(|h| unsafe { IsWindow(Some(HWND(*h as *mut c_void))).as_bool() }));
    TASKINFO.with(|t| t.borrow_mut().retain(|h, _| unsafe { IsWindow(Some(HWND(*h as *mut c_void))).as_bool() }));
}

// Overlay icon and progress that apps set through ITaskbarList3.
#[derive(Default)]
pub(super) struct TaskInfo {
    overlay: Option<slint::Image>,
    state: u32,
    value: i32,
}

thread_local! {
    pub(super) static TASKINFO: RefCell<HashMap<isize, TaskInfo>> = RefCell::new(HashMap::new());
}

// (overlay, has overlay, progress state, progress 0..1) for a window.
pub(crate) fn task_info(hwnd: HWND) -> (slint::Image, bool, i32, f32) {
    TASKINFO.with(|t| match t.borrow().get(&(hwnd.0 as isize)) {
        Some(i) => (i.overlay.clone().unwrap_or_default(), i.overlay.is_some(), i.state as i32, i.value as f32 / 65535.0),
        None => (slint::Image::default(), false, 0, 0.0),
    })
}

pub(super) const TB_PROGRESS_VALUE: u32 = WM_USER + 64;
pub(super) const TB_PROGRESS_STATE: u32 = WM_USER + 65;
pub(super) const TB_OVERLAY_ICON: u32 = WM_USER + 79;

// Receives the ITaskbarList3 calls; the tray window points apps here via its "TaskbandHWND" property.
pub(super) unsafe extern "system" fn hook_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let target = wp.0 as isize;
    let changed = match msg {
        TB_PROGRESS_VALUE => {
            TASKINFO.with(|t| t.borrow_mut().entry(target).or_default().value = lp.0 as i32);
            true
        }
        TB_PROGRESS_STATE => {
            TASKINFO.with(|t| t.borrow_mut().entry(target).or_default().state = lp.0 as u32);
            true
        }
        TB_OVERLAY_ICON => {
            let image = if lp.0 != 0 {
                unsafe { icon_image(HICON(lp.0 as *mut c_void), icon_px_logical(12.0)) }
            } else {
                None
            };
            TASKINFO.with(|t| t.borrow_mut().entry(target).or_default().overlay = image);
            true
        }
        _ => false,
    };
    if changed {
        let _ = slint::invoke_from_event_loop(crate::refresh_all);
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub(super) fn icon_px_logical(logical: f32) -> u32 {
    unsafe { (logical * GetDpiForSystem() as f32 / 96.0).round() as u32 }
}

pub(super) fn init_task_hook(tray: HWND, module: HMODULE) {
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(hook_proc),
            hInstance: module.into(),
            lpszClassName: w!("RustTaskbarTaskHook"),
            ..Default::default()
        };
        RegisterClassW(&class);
        if let Ok(h) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RustTaskbarTaskHook"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        ) {
            let _ = SetPropW(tray, w!("TaskbandHWND"), Some(HANDLE(h.0)));
        }
    }
}

