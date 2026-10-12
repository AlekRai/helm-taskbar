use super::*;

pub(super) type Anim = (HWND, i32, i32, i32, Instant);

thread_local! {
    pub(super) static ENTER: slint::Timer = slint::Timer::default();
    pub(super) static ANIMS: RefCell<Vec<Anim>> = const { RefCell::new(Vec::new()) };
}

// Fades the popup in while sliding it up into place.
pub(super) fn animate_in(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, slide: i32) {
    unsafe {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as i32);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y + slide, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        // Parked popups don't repaint reliably; mark everything dirty so the first visible frame is complete.
        crate::theme::force_repaint();
    }
    let start = Instant::now();
    ANIMS.with(|a| {
        let mut a = a.borrow_mut();
        a.retain(|e| e.0 != hwnd);
        a.push((hwnd, x, y, slide, start));
    });
    ENTER.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(16), || {
            let done = ANIMS.with(|a| {
                let mut a = a.borrow_mut();
                a.retain(|&(hwnd, x, y, slide, start)| {
                    let dur = 0.14 * load_ui().anim_scale();
                    let p = if dur <= 0.0 { 1.0 } else { (start.elapsed().as_secs_f32() / dur).min(1.0) };
                    let e = 1.0 - (1.0 - p).powi(3);
                    unsafe {
                        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), (e * 255.0) as u8, LWA_ALPHA);
                        let _ = SetWindowPos(
                            hwnd,
                            None,
                            x,
                            y + ((1.0 - e) * slide as f32) as i32,
                            0,
                            0,
                            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                        );
                        if p >= 1.0 {
                            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                            SetWindowLongW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as i32));
                        }
                    }
                    p < 1.0
                });
                a.is_empty()
            });
            if done {
                ENTER.with(|t| t.stop());
            }
        });
    });
}

pub(super) fn place(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, scale: f32) {
    unsafe {
        style_overlay(hwnd);
    }
    animate_in(hwnd, x, y, w, h, (12.0 * scale).round() as i32);
}

// The launcher needs keyboard input, so unlike the other popups it may take focus.
pub(super) fn place_focused(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, scale: f32) {
    unsafe {
        style_overlay(hwnd);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
    }
    animate_in(hwnd, x, y, w, h, (12.0 * scale).round() as i32);
    unsafe { crate::start::force_foreground(hwnd) };
}

