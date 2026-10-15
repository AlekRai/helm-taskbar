use super::*;

// Date styles offered in settings, in `clock_date` order (0 = none).
pub(crate) fn date_text(kind: u32, t: &SYSTEMTIME) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const FULL: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
        "December",
    ];
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let m = (t.wMonth as usize).clamp(1, 12) - 1;
    match kind {
        1 => format!("{} {}", t.wDay, MONTHS[m]),
        2 => format!("{} {} {}", DAYS[t.wDayOfWeek as usize % 7], t.wDay, MONTHS[m]),
        3 => format!("{:02}/{:02}/{}", t.wDay, t.wMonth, t.wYear),
        4 => format!("{}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay),
        5 => format!("{} {} {}", t.wDay, FULL[m], t.wYear),
        _ => String::new(),
    }
}

// Estimated extra bar width for each date style.
pub(crate) fn date_width(kind: u32) -> f32 {
    [0.0, 52.0, 86.0, 80.0, 80.0, 124.0].get(kind as usize).copied().unwrap_or(0.0)
}

// (time, date) strings; the date is empty when no date style is chosen.
pub(crate) fn clock_parts() -> (String, String) {
    let t = unsafe { GetLocalTime() };
    let u = load_ui();
    let sec = if u.clock_seconds { format!(":{:02}", t.wSecond) } else { String::new() };
    let clock = if load_theme().clock_24h {
        format!("{:02}:{:02}{}", t.wHour, t.wMinute, sec)
    } else {
        format!("{}:{:02}{} {}", (t.wHour + 11) % 12 + 1, t.wMinute, sec, if t.wHour < 12 { "AM" } else { "PM" })
    };
    (clock, date_text(u.clock_date, &t))
}

pub(super) fn set_clock(ui: &BarWindow, parts: &(String, String)) {
    ui.set_clock(parts.0.clone().into());
    ui.set_clock_date(parts.1.clone().into());
}

// Refreshes just the clock text on every bar.
pub(crate) fn update_clocks() {
    let parts = clock_parts();
    log_bar_health(&parts.0);
    // A changed text also forces a repaint, in case a bar's window stopped redrawing (e.g. after sleep or a display change).
    let changed = LAST_CLOCK.with(|l| l.replace(parts.clone()) != parts) | MEET_CHANGED.with(|c| c.replace(false));
    for m in bar_keys() {
        if let Some((ui, hwnd)) = with_bar(m, |b| (b.ui.clone_strong(), b.hwnd)) {
            set_clock(&ui, &parts);
            if changed {
                ui.set_clock_flip(!ui.get_clock_flip());
                ui.window().request_redraw();
                unsafe {
                    use windows::Win32::Graphics::Gdi::{RDW_ALLCHILDREN, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow};
                    let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW);
                    // Hovering is what revives a stalled bar, so nudge it the same way while the cursor is elsewhere.
                    let mut pt = POINT::default();
                    let mut r = RECT::default();
                    let _ = GetCursorPos(&mut pt);
                    let _ = GetWindowRect(hwnd, &mut r);
                    if !PtInRect(&r, pt).as_bool() {
                        use slint::platform::WindowEvent;
                        ui.window().dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(0.0, 0.0) });
                        ui.window().dispatch_event(WindowEvent::PointerExited);
                    }
                }
            }
        }
    }
}

// Every 30 s notes each bar's window state, but writes only when it differs from the last note, so the log stays small.
fn log_bar_health(clock: &str) {
    let due = HEALTH_AT.with(|t| {
        let due = t.get().is_none_or(|at| at.elapsed() >= Duration::from_secs(30));
        if due {
            t.set(Some(std::time::Instant::now()));
        }
        due
    });
    if !due {
        return;
    }
    for m in bar_keys() {
        let Some(hwnd) = with_bar(m, |b| b.hwnd) else { continue };
        let mut r = RECT::default();
        let (visible, iconic, topmost) = unsafe {
            let _ = GetWindowRect(hwnd, &mut r);
            (IsWindowVisible(hwnd).as_bool(), IsIconic(hwnd).as_bool(), GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0)
        };
        let note = format!("visible={visible} iconic={iconic} topmost={topmost} rect={},{},{},{}", r.left, r.top, r.right, r.bottom);
        let changed = HEALTH.with(|h| h.borrow_mut().insert(m, note.clone()).as_ref() != Some(&note));
        if changed {
            log(&format!("bar {m:?} health: {note} (clock {clock})"));
        }
    }
}

thread_local! {
    static HEALTH_AT: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    static HEALTH: std::cell::RefCell<std::collections::HashMap<isize, String>> = std::cell::RefCell::new(std::collections::HashMap::new());
    pub(super) static CLOCK_TIMER: slint::Timer = slint::Timer::default();
    // Set when the meeting chip's text changed, so the next clock update repaints the bars as well.
    pub(crate) static MEET_CHANGED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static LAST_CLOCK: std::cell::RefCell<(String, String)> = const { std::cell::RefCell::new((String::new(), String::new())) };
}

// The fast tick is only needed while seconds are shown; otherwise the regular refresh keeps the clock current.
pub(crate) fn sync_clock_timer() {
    CLOCK_TIMER.with(|t| {
        let want = load_ui().clock_seconds;
        if want && !t.running() {
            t.start(slint::TimerMode::Repeated, Duration::from_millis(250), update_clocks);
        } else if !want && t.running() {
            t.stop();
        }
    });
}

