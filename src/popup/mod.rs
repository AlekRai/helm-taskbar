use crate::prelude::*;
use std::time::Instant;

mod anim;
mod context;
mod dismiss;
mod flyout;
mod launcher;
mod quick_popup;
mod tooltip;

use anim::*;
pub(crate) use context::*;
pub(crate) use dismiss::*;
pub(crate) use flyout::*;
pub(crate) use launcher::*;
pub(crate) use quick_popup::*;
pub(crate) use tooltip::*;


struct Popups {
    menu: MenuPopup,
    menu_hwnd: HWND,
    menu_open: bool,
    start: StartPopup,
    start_hwnd: HWND,
    start_open: bool,
    quick: QuickPopup,
    quick_hwnd: HWND,
    quick_open: bool,
    fly: TrayFlyout,
    fly_hwnd: HWND,
    fly_open: bool,
    tip: TipPopup,
    tip_hwnd: HWND,
}

// Which monitor each popup opened on, plus outside-click bookkeeping.
#[derive(Default)]
struct Track {
    prev_down: Cell<bool>,
    closed_at: Cell<Option<(Instant, u8, isize)>>,
    open_mon: Cell<isize>,
    fly_mon: Cell<isize>,
    // What the flyout lists: TRAY_KIND for overflow tray icons, else a service group index.
    fly_kind: Cell<i32>,
    // Service whose unread mail list the flyout shows (when fly_kind is MAIL_KIND).
    mail_id: RefCell<String>,
    quick_mon: Cell<isize>,
    keep_fly: Cell<bool>,
}

type OpenMenu = (Vec<Box<dyn MenuProvider>>, AppInfo);

thread_local! {
    static POPUPS: RefCell<Option<Popups>> = const { RefCell::new(None) };
    static OPEN_MENU: RefCell<Option<OpenMenu>> = const { RefCell::new(None) };
    static DISMISS: slint::Timer = slint::Timer::default();
    static TIP_TIMER: slint::Timer = slint::Timer::default();
    static TRACK: Track = Track::default();
}

const MENU_WIDTH: f32 = 220.0;
const TRAY_KIND: i32 = -1;
const CAL_HEIGHT_FULL: f32 = 420.0;

// Calendar plus its 1px divider; zero when the calendar is turned off.
pub(crate) fn cal_height() -> f32 {
    if load_ui().show_calendar { CAL_HEIGHT_FULL + 1.0 } else { 0.0 }
}

// Height of the quick-settings cards (everything below the calendar) with no list expanded.
pub(crate) fn quick_main_height() -> f32 {
    let u = load_ui();
    let top = 8.0 + if u.show_wifi { 80.0 } else { 0.0 } + if u.show_bluetooth { 72.0 } else { 0.0 };
    top + if u.show_mixer { 198.0 } else { 144.0 }
}
const START_WIDTH: f32 = 560.0;
const START_HEIGHT: f32 = 620.0;
const QUICK_WIDTH: f32 = 360.0;
const GAP: i32 = 8;

pub(crate) fn with_popup_uis(f: impl FnOnce(&MenuPopup, &StartPopup, &QuickPopup, &TrayFlyout, &TipPopup)) {
    POPUPS.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            f(&p.menu, &p.start, &p.quick, &p.fly, &p.tip);
        }
    });
}

pub(crate) fn try_with_popup_uis(f: impl FnOnce(&MenuPopup, &StartPopup, &QuickPopup, &TrayFlyout, &TipPopup)) {
    POPUPS.with(|p| {
        if let Ok(p) = p.try_borrow()
            && let Some(p) = p.as_ref()
        {
            f(&p.menu, &p.start, &p.quick, &p.fly, &p.tip);
        }
    });
}

pub(crate) fn init_popups() {
    let Ok(menu) = MenuPopup::new() else { return };
    let Ok(start) = StartPopup::new() else { return };
    let Ok(quick) = QuickPopup::new() else { return };
    let Ok(fly) = TrayFlyout::new() else { return };
    let Ok(tip) = TipPopup::new() else { return };
    menu.on_activated(|id| menu_activated(id as usize));
    fly.on_clicked(|i, b| crate::tray::on_flyout_clicked(TRACK.with(|t| t.fly_mon.get()), i as usize, b));
    fly.on_hovered(fly_hovered);
    fly.on_svc_clicked(|i| svc_row_clicked(i as usize));
    fly.on_svc_hovered(svc_row_hovered);
    fly.on_mail_clicked(|i| mail_row_clicked(i as usize));
    crate::calendar::wire(&quick);
    crate::start::wire(&start);
    crate::quick::wire(&quick);
    for w in [menu.window(), start.window(), quick.window(), fly.window(), tip.window()] {
        w.set_position(slint::PhysicalPosition::new(-32000, -32000));
    }
    let _ = menu.show();
    let _ = start.show();
    let _ = quick.show();
    let _ = fly.show();
    let _ = tip.show();
    POPUPS.with(|p| {
        *p.borrow_mut() = Some(Popups {
            menu,
            menu_hwnd: HWND::default(),
            menu_open: false,
            start,
            start_hwnd: HWND::default(),
            start_open: false,
            quick,
            quick_hwnd: HWND::default(),
            quick_open: false,
            fly,
            fly_hwnd: HWND::default(),
            fly_open: false,
            tip,
            tip_hwnd: HWND::default(),
        });
    });
    crate::theme::apply_popup_theme(&load_theme());
}

pub(crate) fn popup_owns(h: HWND) -> bool {
    POPUPS.with(|p| match p.try_borrow() {
        Ok(p) => p.as_ref().is_some_and(|p| {
            [p.menu_hwnd, p.start_hwnd, p.quick_hwnd, p.fly_hwnd, p.tip_hwnd].contains(&h)
        }),
        Err(_) => false,
    })
}

pub(crate) fn popups_ready() -> bool {
    POPUPS.with(|p| {
        p.borrow().as_ref().is_none_or(|p| {
            ![p.menu_hwnd, p.start_hwnd, p.quick_hwnd, p.fly_hwnd, p.tip_hwnd].iter().any(|h| h.0.is_null())
        })
    })
}

// Gives the popup windows native handles and hides them until needed.
pub(crate) unsafe fn finalize_popups() {
    unsafe {
        if popups_ready() {
            return;
        }
        let Some((menu, start, quick, fly, tip)) = POPUPS.with(|p| {
            p.borrow().as_ref().map(|p| {
                (p.menu.clone_strong(), p.start.clone_strong(), p.quick.clone_strong(), p.fly.clone_strong(), p.tip.clone_strong())
            })
        }) else {
            return;
        };
        let setup = |w: &slint::Window, shadow: bool| -> Option<HWND> {
            let hwnd = winit_hwnd(w)?;
            style_overlay(hwnd);
            use slint::winit_030::WinitWindowAccessor;
            use slint::winit_030::winit::platform::windows::WindowExtWindows;
            w.with_winit_window(|ww| {
                ww.set_resizable(false);
                ww.set_undecorated_shadow(shadow);
            });
            let round: u32 = 2; // DWMWCP_ROUND
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &round as *const u32 as *const c_void,
                size_of::<u32>() as u32,
            );
            let _ = ShowWindow(hwnd, SW_HIDE);
            Some(hwnd)
        };
        let m = setup(menu.window(), true);
        let s = setup(start.window(), true);
        let q = setup(quick.window(), true);
        let f = setup(fly.window(), true);
        let t = setup(tip.window(), false);
        if let Some(t) = t {
            // The tooltip must never intercept the mouse.
            let ex = GetWindowLongPtrW(t, GWL_EXSTYLE);
            SetWindowLongPtrW(t, GWL_EXSTYLE, ex | WS_EX_TRANSPARENT.0 as isize);
        }
        POPUPS.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                if let Some(m) = m {
                    p.menu_hwnd = m;
                }
                if let Some(s) = s {
                    p.start_hwnd = s;
                }
                if let Some(q) = q {
                    p.quick_hwnd = q;
                }
                if let Some(f) = f {
                    p.fly_hwnd = f;
                }
                if let Some(t) = t {
                    p.tip_hwnd = t;
                }
            }
        });
    }
}

