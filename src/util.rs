use crate::prelude::*;

pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Appends a line to %APPDATA%\rust_taskbar\log.txt; the file restarts once it passes 256 KB.
pub(crate) fn log(msg: &str) {
    use std::io::Write;
    let Some(dir) = config_dir() else { return };
    let path = dir.join("log.txt");
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

// Closes the wrapped Win32 handle when dropped.
pub(crate) struct Handle(pub(crate) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) fn open_process(access: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS, pid: u32) -> Option<Handle> {
    unsafe { OpenProcess(access, false, pid).ok().map(Handle) }
}

// Reads a bitmap as top-down 32-bit BGRA pixels (w * h * 4 bytes).
pub(crate) unsafe fn bitmap_bgra(bmp: HBITMAP, w: i32, h: i32) -> Vec<u8> {
    unsafe {
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let hdc = GetDC(None);
        GetDIBits(hdc, bmp, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, hdc);
        px
    }
}

pub(crate) fn file_stem(path: &str) -> String {
    PathBuf::from(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn file_name(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

// ---------- window / process info ----------

pub(crate) unsafe fn exe_of(pid: u32) -> String {
    unsafe {
        let Some(h) = open_process(PROCESS_QUERY_LIMITED_INFORMATION, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
            .is_ok();
        if ok { String::from_utf16_lossy(&buf[..len as usize]) } else { String::new() }
    }
}

pub(crate) unsafe fn window_icon(hwnd: HWND) -> HICON {
    unsafe {
        // ICON_BIG, ICON_SMALL2, ICON_SMALL: bigger ones stay crisp when scaled down
        for kind in [1usize, 2, 0] {
            let mut res = 0usize;
            SendMessageTimeoutW(
                hwnd,
                WM_GETICON,
                WPARAM(kind),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                50,
                Some(&mut res),
            );
            if res != 0 {
                return HICON(res as *mut c_void);
            }
        }
        for idx in [GCLP_HICONSM, GCLP_HICON] {
            let v = GetClassLongPtrW(hwnd, idx);
            if v != 0 {
                return HICON(v as *mut c_void);
            }
        }
        HICON::default()
    }
}

pub(crate) unsafe fn exe_icon(path: &str) -> HICON {
    unsafe {
        if path.is_empty() {
            return HICON::default();
        }
        if let Some(i) = STATE.with(|s| s.borrow().exe_icons.get(path).copied()) {
            return i;
        }
        let wide = to_wide(path);
        let mut info = SHFILEINFOW::default();
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        STATE.with(|s| s.borrow_mut().exe_icons.insert(path.to_string(), info.hIcon));
        info.hIcon
    }
}

// Icon of a UWP app hosted by ApplicationFrameHost, found through the window's AppUserModelID.
pub(crate) unsafe fn uwp_icon(hwnd: HWND) -> HICON {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    use windows::core::GUID;
    unsafe {
        let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd) else { return HICON::default() };
        let key = PROPERTYKEY { fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3), pid: 5 };
        let Ok(value) = store.GetValue(&key) else { return HICON::default() };
        let Ok(id) = PropVariantToStringAlloc(&value) else { return HICON::default() };
        let aumid = id.to_string().unwrap_or_default();
        windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const c_void));
        if aumid.is_empty() {
            return HICON::default();
        }
        let path = to_wide(&format!("shell:AppsFolder\\{aumid}"));
        let mut pidl = std::ptr::null_mut();
        if SHParseDisplayName(PCWSTR(path.as_ptr()), None, &mut pidl, 0, None).is_err() || pidl.is_null() {
            return HICON::default();
        }
        let mut info = SHFILEINFOW::default();
        SHGetFileInfoW(
            PCWSTR(pidl as *const u16),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON,
        );
        windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const c_void));
        info.hIcon
    }
}

// Icon from the first matching override rule (see config::load_icon_rules), if its file loads.
pub(crate) unsafe fn override_icon(exe: &str, title: &str) -> Option<HICON> {
    unsafe {
        let name = file_name(exe);
        let title = title.to_lowercase();
        let path = STATE.with(|s| {
            s.borrow()
                .icon_rules
                .iter()
                .find(|(e, t, _)| (e == "*" || *e == name) && title.contains(t.as_str()))
                .map(|r| r.2.clone())
        })?;
        if let Some(i) = STATE.with(|s| s.borrow().file_icons.get(&path).copied()) {
            return (!i.0.is_null()).then_some(i);
        }
        let wide = to_wide(&path);
        let h = LoadImageW(None, PCWSTR(wide.as_ptr()), IMAGE_ICON, 256, 256, LR_LOADFROMFILE)
            .map(|h| HICON(h.0))
            .unwrap_or_default();
        STATE.with(|s| s.borrow_mut().file_icons.insert(path, h));
        (!h.0.is_null()).then_some(h)
    }
}

pub(crate) unsafe fn monitor_device(m: HMONITOR) -> String {
    unsafe {
        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        let _ = GetMonitorInfoW(m, &mut mi as *mut _ as *mut MONITORINFO);
        let len = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
        String::from_utf16_lossy(&mi.szDevice[..len])
    }
}

// Minimized windows report an off-screen rect, so use their restore position.
pub(crate) unsafe fn window_monitor(h: HWND) -> isize {
    unsafe {
        if IsIconic(h).as_bool() {
            let mut wp = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
            if GetWindowPlacement(h, &mut wp).is_ok() {
                return MonitorFromRect(&wp.rcNormalPosition, MONITOR_DEFAULTTONEAREST).0 as isize;
            }
        }
        MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST).0 as isize
    }
}
