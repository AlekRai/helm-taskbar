use super::*;

pub(crate) static START_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) static PREV_FG: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub(super) fn search_window() -> HWND {
    unsafe { FindWindowW(w!("Windows.UI.Core.CoreWindow"), w!("Search")) }.unwrap_or_default()
}


pub(crate) fn toggle_start(monitor: isize) {
    let was_open = POPUPS.with(|p| p.borrow().as_ref().is_some_and(|p| p.start_open));
    if was_open {
        close_popups();
        return;
    }
    let recent = TRACK
        .with(|t| t.closed_at.take())
        .is_some_and(|(t, k, m)| k == 1 && m == monitor && t.elapsed() < Duration::from_millis(400));
    if recent {
        return;
    }
    TRACK.with(|t| t.open_mon.set(monitor));
    let Some((rect, scale)) = with_bar(monitor, |b| (b.rect, b.scale)) else { return };
    close_popups();
    let Some((ui, hwnd)) = POPUPS.with(|p| p.borrow().as_ref().map(|p| (p.start.clone_strong(), p.start_hwnd))) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    crate::start::reset(&ui);
    {
        let fg = unsafe { GetForegroundWindow() };
        if fg != hwnd && fg != search_window() {
            PREV_FG.store(fg.0 as isize, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let (w, h) = ((START_WIDTH * scale).round() as i32, (START_HEIGHT * scale).round() as i32);
    let x = rect.left + ((rect.right - rect.left) - w) / 2;
    place_focused(hwnd, x, rect.top - h - GAP, w, h, scale);
    POPUPS.with(|p| p.borrow_mut().as_mut().map(|p| p.start_open = true));
    START_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
    start_dismiss();
    slint::Timer::single_shot(Duration::from_millis(60), move || ui.invoke_focus_search());
}
