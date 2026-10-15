use super::*;
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_VISIBLE, DwmQueryThumbnailSourceSize,
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
};

// Live window preview shown above an app button: a plain Win32 popup that hosts a DWM thumbnail.
// It never takes focus; clicking it opens the window.
const MAX_W: f32 = 260.0;
const MAX_H: f32 = 160.0;
const PAD: f32 = 10.0;
const TITLE_H: f32 = 22.0;

struct Preview {
    hwnd: Cell<HWND>,
    thumb: Cell<isize>,
    source: Cell<HWND>,
    visible: Cell<bool>,
    hidden_at: Cell<Option<Instant>>,
    // True while the pointer is over an app button.
    hovering: Cell<bool>,
    scale: Cell<f32>,
    bar: Cell<RECT>,
    title: RefCell<String>,
}

thread_local! {
    static PREVIEW: Preview = Preview {
        hwnd: Cell::new(HWND::default()),
        thumb: Cell::new(0),
        source: Cell::new(HWND::default()),
        visible: Cell::new(false),
        hidden_at: Cell::new(None),
        hovering: Cell::new(false),
        scale: Cell::new(1.0),
        bar: Cell::new(RECT::default()),
        title: RefCell::new(String::new()),
    };
    static SHOW_TIMER: slint::Timer = slint::Timer::default();
    static WATCH_TIMER: slint::Timer = slint::Timer::default();
}

pub(crate) fn preview_hwnd() -> HWND {
    PREVIEW.with(|p| p.hwnd.get())
}

pub(crate) fn preview_hovered(monitor: isize, index: usize, on: bool) {
    SHOW_TIMER.with(|t| t.stop());
    PREVIEW.with(|p| p.hovering.set(on));
    if !on {
        // Leaving the button for the preview itself keeps it open.
        SHOW_TIMER.with(|t| t.start(slint::TimerMode::SingleShot, Duration::from_millis(250), hide_unless_used));
        return;
    }
    let delay = load_ui().preview_delay_ms;
    if delay == 0 {
        return;
    }
    // Moving straight to a neighbouring button swaps the preview quickly.
    let recent = PREVIEW.with(|p| p.visible.get() || p.hidden_at.get().is_some_and(|t| t.elapsed() < Duration::from_millis(250)));
    let wait = if recent { 80 } else { delay as u64 };
    SHOW_TIMER.with(|t| t.start(slint::TimerMode::SingleShot, Duration::from_millis(wait), move || show_preview(monitor, index)));
}

pub(crate) fn hide_preview() {
    SHOW_TIMER.with(|t| t.stop());
    WATCH_TIMER.with(|t| t.stop());
    PREVIEW.with(|p| {
        let thumb = p.thumb.replace(0);
        if thumb != 0 {
            unsafe {
                let _ = DwmUnregisterThumbnail(thumb);
            }
        }
        if p.visible.replace(false) {
            p.hidden_at.set(Some(Instant::now()));
            let hwnd = p.hwnd.get();
            if !hwnd.0.is_null() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    });
}

fn show_preview(monitor: isize, index: usize) {
    if !open_rects().is_empty() {
        return;
    }
    let Some((entry, rect, scale)) = with_bar(monitor, |b| {
        b.shown.get(index).filter(|s| !s.closing).map(|s| (s.entry.clone(), b.rect, b.scale))
    })
    .flatten() else {
        return;
    };
    let src = entry.hwnd;
    if src.0.is_null() || entry.exe == crate::actions::START_EXE || !unsafe { IsWindow(Some(src)) }.as_bool() {
        return;
    }
    let Some(host) = ensure_host() else { return };
    hide_preview();
    unsafe {
        let Ok(thumb) = DwmRegisterThumbnail(host, src) else { return };
        let Ok(size) = DwmQueryThumbnailSourceSize(thumb) else {
            let _ = DwmUnregisterThumbnail(thumb);
            return;
        };
        if size.cx <= 0 || size.cy <= 0 {
            let _ = DwmUnregisterThumbnail(thumb);
            return;
        }
        let fit = (MAX_W * scale / size.cx as f32).min(MAX_H * scale / size.cy as f32).min(1.0);
        let (tw, th) = (((size.cx as f32 * fit).round() as i32).max(1), ((size.cy as f32 * fit).round() as i32).max(1));
        let (pad, title_h) = ((PAD * scale).round() as i32, (TITLE_H * scale).round() as i32);
        let (w, h) = (tw.max((120.0 * scale) as i32) + 2 * pad, 2 * pad + title_h + th);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let x = (cursor.x - w / 2).clamp(rect.left + GAP, (rect.right - w - GAP).max(rect.left + GAP));
        let y = rect.top - h - 2;
        let props = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY,
            rcDestination: RECT { left: (w - tw) / 2, top: pad + title_h, right: (w - tw) / 2 + tw, bottom: pad + title_h + th },
            opacity: 255,
            fVisible: true.into(),
            ..Default::default()
        };
        let _ = DwmUpdateThumbnailProperties(thumb, &props);
        PREVIEW.with(|p| {
            p.thumb.set(thumb);
            p.source.set(src);
            p.scale.set(scale);
            p.bar.set(rect);
            *p.title.borrow_mut() = entry.title.clone();
            p.visible.set(true);
        });
        let _ = SetWindowPos(host, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        let _ = InvalidateRect(Some(host), None, true);
    }
    WATCH_TIMER.with(|t| t.start(slint::TimerMode::Repeated, Duration::from_millis(300), watch));
}

fn pointer_on_preview() -> bool {
    let host = preview_hwnd();
    let mut cursor = POINT::default();
    let mut r = RECT::default();
    unsafe { GetCursorPos(&mut cursor).is_ok() && GetWindowRect(host, &mut r).is_ok() && PtInRect(&r, cursor).as_bool() }
}

fn hide_unless_used() {
    if !PREVIEW.with(|p| p.hovering.get()) && !pointer_on_preview() {
        hide_preview();
    }
}

// Closes the preview when its window is gone or the pointer is on neither a button nor the preview.
fn watch() {
    let src = PREVIEW.with(|p| p.source.get());
    if !unsafe { IsWindow(Some(src)) }.as_bool() {
        hide_preview();
    } else {
        hide_unless_used();
    }
}

// Clicking the preview opens its window.
fn open_source() {
    let src = PREVIEW.with(|p| p.source.get());
    hide_preview();
    unsafe {
        if IsWindow(Some(src)).as_bool() {
            if IsIconic(src).as_bool() {
                let _ = ShowWindow(src, SW_RESTORE);
            }
            let _ = SetForegroundWindow(src);
        }
    }
}

fn ensure_host() -> Option<HWND> {
    let cur = preview_hwnd();
    if !cur.0.is_null() {
        return Some(cur);
    }
    unsafe {
        let class = w!("HelmWindowPreview");
        let hinst = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(preview_proc),
            hInstance: hinst.into(),
            lpszClassName: class,
            hbrBackground: CreateSolidBrush(COLORREF(0x0020_2020)),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            w!(""),
            WS_POPUP,
            -32000,
            -32000,
            10,
            10,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .ok()?;
        let round: u32 = 2;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const u32 as *const c_void, size_of::<u32>() as u32);
        PREVIEW.with(|p| p.hwnd.set(hwnd));
        Some(hwnd)
    }
}

// The close button in the top right corner of the title row.
fn close_rect(client: &RECT, scale: f32) -> RECT {
    let pad = (PAD * scale).round() as i32;
    let size = (TITLE_H * scale).round() as i32;
    RECT { left: client.right - pad - size, top: pad, right: client.right - pad, bottom: pad + size }
}

fn close_source() {
    let src = PREVIEW.with(|p| p.source.get());
    hide_preview();
    unsafe {
        let _ = PostMessageW(Some(src), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn preview_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEMOVE => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let scale = PREVIEW.with(|p| p.scale.get());
                let pt = POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
                if PtInRect(&close_rect(&rc, scale), pt).as_bool() {
                    close_source();
                } else {
                    open_source();
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(3),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let (scale, title) = PREVIEW.with(|p| (p.scale.get(), p.title.borrow().clone()));
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let pad = (PAD * scale).round() as i32;
                rc.left += pad;
                rc.right -= pad;
                rc.top += pad;
                rc.bottom = rc.top + (TITLE_H * scale).round() as i32;
                let mut full = RECT::default();
                let _ = GetClientRect(hwnd, &mut full);
                let cr = close_rect(&full, scale);
                rc.right = cr.left - 4;
                let mut cur = POINT::default();
                let _ = GetCursorPos(&mut cur);
                let _ = ScreenToClient(hwnd, &mut cur);
                let hot = PtInRect(&cr, cur).as_bool();
                if hot {
                    let br = CreateSolidBrush(COLORREF(0x0043_43c4));
                    FillRect(hdc, &cr, br);
                    let _ = DeleteObject(br.into());
                }
                let pen = CreatePen(PS_SOLID, ((1.5 * scale).round() as i32).max(1), COLORREF(0x00f0_f0f0));
                let old_pen = SelectObject(hdc, pen.into());
                let m = ((7.0 * scale).round()) as i32;
                let _ = MoveToEx(hdc, cr.left + m, cr.top + m, None);
                let _ = LineTo(hdc, cr.right - m, cr.bottom - m);
                let _ = MoveToEx(hdc, cr.right - m, cr.top + m, None);
                let _ = LineTo(hdc, cr.left + m, cr.bottom - m);
                SelectObject(hdc, old_pen);
                let _ = DeleteObject(pen.into());
                let font = CreateFontW(
                    -((13.0 * scale).round() as i32),
                    0,
                    0,
                    0,
                    400,
                    0,
                    0,
                    0,
                    FONT_CHARSET(1),
                    FONT_OUTPUT_PRECISION(0),
                    FONT_CLIP_PRECISION(0),
                    FONT_QUALITY(5),
                    0,
                    w!("Segoe UI"),
                );
                let old = SelectObject(hdc, font.into());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00f0_f0f0));
                let mut text: Vec<u16> = title.encode_utf16().collect();
                DrawTextW(hdc, &mut text, &mut rc, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
                SelectObject(hdc, old);
                let _ = DeleteObject(font.into());
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
