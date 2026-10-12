use super::*;

pub(crate) fn toggle_quick(monitor: isize) {
    let was_open = POPUPS.with(|p| p.borrow().as_ref().is_some_and(|p| p.quick_open));
    if was_open {
        close_popups();
        return;
    }
    let recent = TRACK
        .with(|t| t.closed_at.take())
        .is_some_and(|(t, k, m)| k == 2 && m == monitor && t.elapsed() < Duration::from_millis(400));
    if recent {
        return;
    }
    TRACK.with(|t| t.open_mon.set(monitor));
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    close_popups();
    let Some((ui, hwnd)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.quick.clone_strong(), p.quick_hwnd))) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    TRACK.with(|t| t.quick_mon.set(monitor));
    crate::quick::reset(&ui);
    let (w, h) = ((QUICK_WIDTH * scale).round() as i32, ((quick_main_height() + cal_height()) * scale).round() as i32);
    // The calendar lives in the same window, above the quick settings.
    crate::calendar::reset(&ui);
    place(hwnd, quick_x(monitor, rect, w), rect.top - h - GAP, w, h, scale);
    POPUPS.with(|p| p.borrow_mut().as_mut().map(|p| p.quick_open = true));
    start_dismiss();
}

// Quick settings opens above the clock, which may be on either edge of the bar.
pub(super) fn quick_x(monitor: isize, rect: RECT, w: i32) -> i32 {
    let device = with_bar(monitor, |b| b.device.clone()).unwrap_or_default();
    if monitor_cfg(&device).tray_left { rect.left + GAP } else { rect.right - w - GAP }
}

// Resizes the open quick-settings popup, keeping its bottom edge above the bar.
pub(crate) fn resize_quick(main_h: f32) {
    let logical_h = main_h + cal_height();
    let monitor = TRACK.with(|t| t.quick_mon.get());
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    let Some((hwnd, open)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.quick_hwnd, p.quick_open))) else {
        return;
    };
    if !open || hwnd.0.is_null() {
        return;
    }
    let mut wr = RECT::default();
    let cur = if unsafe { GetWindowRect(hwnd, &mut wr) }.is_ok() { (wr.bottom - wr.top) as f32 / scale } else { logical_h };
    let (from, to) = (cur, logical_h);
    let anim = load_ui().anim_scale();
    if (from - to).abs() < 1.0 || anim <= 0.0 {
        apply_quick_height(rect, scale, to);
        return;
    }
    let start = Instant::now();
    RESIZE.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(8), move || {
            let p = (start.elapsed().as_secs_f32() / (0.2 * anim)).min(1.0);
            let e = ease_out(p);
            apply_quick_height(rect, scale, from + (to - from) * e);
            if p >= 1.0 {
                RESIZE.with(|t| t.stop());
            }
        });
    });
}

thread_local! {
    pub(super) static RESIZE: slint::Timer = slint::Timer::default();
}

// Matches Slint's `ease-out` (cubic-bezier 0, 0, 0.58, 1) so the window tracks the in-UI animation.
pub(super) fn ease_out(x: f32) -> f32 {
    let bez = |t: f32, a: f32, b: f32| 3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3);
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, 0.0, 0.58) < x { lo = mid } else { hi = mid }
    }
    bez((lo + hi) / 2.0, 0.0, 1.0)
}

pub(super) fn apply_quick_height(rect: RECT, scale: f32, logical_h: f32) {
    let Some((hwnd, open, quick)) =
        POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.quick_hwnd, p.quick_open, p.quick.clone_strong())))
    else {
        return;
    };
    if !open || hwnd.0.is_null() {
        return;
    }
    let (w, h) = ((QUICK_WIDTH * scale).round() as i32, (logical_h * scale).round() as i32);
    quick.set_expand_h((h as f32 / scale - cal_height() - quick_main_height()).max(0.0));
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            quick_x(TRACK.with(|t| t.quick_mon.get()), rect, w),
            rect.top - h - GAP,
            w,
            h,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

