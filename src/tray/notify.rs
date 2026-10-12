use super::*;

pub(super) fn rd32(data: &[u8], off: usize) -> u32 {
    data.get(off..off + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
}

pub(super) fn rd_wide(data: &[u8], off: usize, chars: usize) -> String {
    let words: Vec<u16> = (0..chars)
        .map_while(|i| {
            let b = data.get(off + i * 2..off + i * 2 + 2)?;
            let w = u16::from_le_bytes([b[0], b[1]]);
            (w != 0).then_some(w)
        })
        .collect();
    String::from_utf16_lossy(&words)
}

// Handles are 32-bit on the wire and sign-extended.
pub(super) fn handle32(v: u32) -> isize {
    v as i32 as isize
}

pub(super) fn icon_px() -> u32 {
    unsafe { (16.0 * GetDpiForSystem() as f32 / 96.0).round() as u32 }
}

// NIM_ADD / NIM_MODIFY: creates or updates one icon from the NOTIFYICONDATA bytes.
// `id` is (hwnd, uid, guid, flags). Returns whether the tray changed.
pub(super) fn apply_icon_update(
    t: &mut Vec<TrayIcon>,
    msg: u32,
    id: (isize, u32, [u8; 16], u32),
    nid: &[u8],
    matches: &dyn Fn(&TrayIcon) -> bool,
) -> bool {
    let (hwnd, uid, guid, flags) = id;
    let existing = t.iter().position(matches);
    if existing.is_none() && msg == NIM_MODIFY {
        return false;
    }
    let idx = existing.unwrap_or_else(|| {
        t.push(TrayIcon {
            hwnd,
            uid,
            guid,
            callback: 0,
            version: 0,
            hidden: false,
            tip: String::new(),
            image: slint::Image::default(),
            exe: owner_exe(hwnd),
        });
        t.len() - 1
    });
    let icon = &mut t[idx];
    if hwnd != 0 {
        icon.hwnd = hwnd;
        icon.uid = uid;
    }
    if flags & NIF_MESSAGE != 0 {
        icon.callback = rd32(nid, 16);
    }
    if flags & NIF_TIP != 0 {
        icon.tip = rd_wide(nid, 24, 128);
    }
    if flags & NIF_STATE != 0 {
        icon.hidden = rd32(nid, 280) & rd32(nid, 284) & 1 != 0;
    }
    if flags & NIF_ICON != 0 {
        let hicon = handle32(rd32(nid, 20));
        icon.image = if hicon != 0 {
            unsafe { icon_image(HICON(hicon as *mut c_void), icon_px()) }.unwrap_or_default()
        } else {
            slint::Image::default()
        };
    }
    let version = rd32(nid, 800);
    if msg == NIM_ADD && (1..=4).contains(&version) {
        icon.version = version;
    }
    true
}

pub(super) fn handle_notify(data: &[u8]) -> bool {
    if rd32(data, 0) != SIGNATURE {
        return false;
    }
    let msg = rd32(data, 4);
    let nid = &data[8..];
    let hwnd = handle32(rd32(nid, 4));
    let uid = rd32(nid, 8);
    let flags = rd32(nid, 12);
    let mut guid = [0u8; 16];
    if flags & NIF_GUID != 0
        && let Some(g) = nid.get(936..952) {
            guid.copy_from_slice(g);
        }
    let has_guid = guid != [0u8; 16];
    if hwnd == 0 && !has_guid {
        return false;
    }
    let matches = |t: &TrayIcon| if has_guid { t.guid == guid } else { t.hwnd == hwnd && t.uid == uid };

    let changed = TRAY.with(|t| {
        let mut t = t.borrow_mut();
        match msg {
            NIM_ADD | NIM_MODIFY => apply_icon_update(&mut t, msg, (hwnd, uid, guid, flags), nid, &matches),
            NIM_DELETE => {
                let before = t.len();
                t.retain(|i| !matches(i));
                t.len() != before
            }
            NIM_SETVERSION => {
                let v = rd32(nid, 800);
                if let Some(i) = t.iter_mut().find(|i| matches(i))
                    && v <= 4 {
                        i.version = v;
                    }
                false
            }
            _ => false,
        }
    });
    if changed {
        let _ = slint::invoke_from_event_loop(push_tray);
    }
    true
}

pub(super) unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg != 0 && msg == SHELLHOOK_MSG.with(|m| m.get()) {
            // HSHELL_FLASH = 0x8006: lparam is the flashing window.
            if wp.0 as u32 == 0x8006 {
                FLASHING.with(|f| f.borrow_mut().insert(lp.0));
                let _ = slint::invoke_from_event_loop(crate::refresh_all);
            }
            return LRESULT(0);
        }
        match msg {
            WM_COPYDATA => {
                let cds = lp.0 as *const COPYDATASTRUCT;
                if !cds.is_null() && (*cds).dwData == 1 && !(*cds).lpData.is_null() {
                    let data = std::slice::from_raw_parts((*cds).lpData as *const u8, (*cds).cbData as usize);
                    if handle_notify(data) {
                        return LRESULT(1);
                    }
                }
            }
            WM_WINDOWPOSCHANGED => {
                // Never become visible; Explorer's tray window does the visible part of the shell.
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                if style & WS_VISIBLE.0 as isize != 0 {
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_VISIBLE.0 as isize));
                }
            }
            _ => {}
        }
        if (msg == WM_COPYDATA || msg == WM_ACTIVATEAPP || msg == WM_COMMAND || msg >= WM_USER)
            && let Some(target) = explorer_tray() {
                return SendMessageW(target, msg, Some(wp), Some(lp));
            }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

// Drops icons whose owner window is gone (apps that crashed without NIM_DELETE).
pub(crate) fn prune_dead() -> bool {
    TRAY.with(|t| {
        let mut t = t.borrow_mut();
        let before = t.len();
        t.retain(|i| unsafe { IsWindow(Some(HWND(i.hwnd as *mut c_void))).as_bool() });
        t.len() != before
    })
}

