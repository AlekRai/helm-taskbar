use crate::prelude::*;
use slint::Weak;

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];
const MONTHS_SHORT: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const WEEKDAYS_SHORT: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

// One wheel notch is 60 units in Slint.
const WHEEL_STEP: f32 = 60.0;

// ---------- date math (days since 1970-01-01) ----------

fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i32;
    let doy = (153 * mp + 2) / 5 + d as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// Weeks start on Monday. 1970-01-01 was a Thursday.
fn week_of(days: i32) -> i32 {
    (days + 3).div_euclid(7)
}

fn monday_of(week: i32) -> i32 {
    week * 7 - 3
}

fn weekday_mon0(days: i32) -> usize {
    (days + 3).rem_euclid(7) as usize
}

fn today() -> i32 {
    let t = unsafe { GetLocalTime() };
    days_from_civil(t.wYear as i32, t.wMonth as u32, t.wDay as u32)
}

fn first_week_of_month(y: i32, m: u32) -> f64 {
    week_of(days_from_civil(y, m, 1)) as f64
}

// ---------- view state ----------

struct View {
    ui: Option<Weak<QuickPopup>>,
    cur: f64,
    target: f64,
    today: i32,
    selected: Option<i32>,
    picker_open: bool,
    year_mode: bool,
    pcur: f64,
    ptarget: f64,
    prendered: Option<(i32, i32, bool)>,
    wheel: f32,
    rendered: Option<(i32, u32, Option<i32>)>,
}

thread_local! {
    static VIEW: RefCell<View> = const {
        RefCell::new(View {
            ui: None,
            cur: 0.0,
            target: 0.0,
            today: 0,
            selected: None,
            picker_open: false,
            year_mode: false,
            pcur: 0.0,
            ptarget: 0.0,
            prendered: None,
            wheel: 0.0,
            rendered: None,
        })
    };
    static ANIM: slint::Timer = slint::Timer::default();
}

fn title_month(cur: f64) -> (i32, u32) {
    let mid = monday_of(cur.round() as i32) + 24;
    let (y, m, _) = civil_from_days(mid);
    (y, m)
}

fn with_view<R>(f: impl FnOnce(&mut View, &QuickPopup) -> R) -> Option<R> {
    let ui = VIEW.with(|v| v.borrow().ui.as_ref().and_then(|u| u.upgrade()))?;
    Some(VIEW.with(|v| f(&mut v.borrow_mut(), &ui)))
}

fn render(v: &mut View, ui: &QuickPopup, force: bool) {
    let base = v.cur.floor() as i32;
    let frac = (v.cur - base as f64) as f32;
    let (ty, tm) = title_month(v.cur);
    let key = (base, tm, v.selected);
    if force || v.rendered != Some(key) {
        v.rendered = Some(key);
        let mut cells = Vec::with_capacity(49);
        for r in 0..7 {
            for c in 0..7 {
                let days = monday_of(base + r) + c;
                let (_, m, d) = civil_from_days(days);
                cells.push(crate::ui::Cell {
                    day: d as i32,
                    key: days,
                    dim: m != tm,
                    today: days == v.today,
                    selected: v.selected == Some(days),
                });
            }
        }
        ui.set_cal_cells(ModelRc::new(VecModel::from(cells)));
        ui.set_cal_month_title(format!("{} {}", MONTHS[tm as usize - 1], ty).into());
    }
    ui.set_cal_frac(frac);
}

// Picker rows hold 4 cells: month index (year * 12 + month0) or plain years.
fn render_picker(v: &mut View, ui: &QuickPopup, force: bool) {
    let top = v.pcur.round() as i32;
    let base = v.pcur.floor() as i32;
    let (ty, tm) = title_month(v.target);
    let viewed = if v.year_mode { ty } else { ty * 12 + tm as i32 - 1 };
    if force || v.prendered != Some((base, top, v.year_mode)) {
        v.prendered = Some((base, top, v.year_mode));
        let label_year = (top * 4).div_euclid(12);
        let cells: Vec<crate::ui::PCell> = (0..20)
            .map(|i| {
                let key = (base + i / 4) * 4 + i % 4;
                let (label, dim) = if v.year_mode {
                    (key.to_string(), false)
                } else {
                    (MONTHS_SHORT[key.rem_euclid(12) as usize].to_string(), key.div_euclid(12) != label_year)
                };
                crate::ui::PCell { label: label.into(), key, dim, selected: key == viewed }
            })
            .collect();
        ui.set_cal_pcells(ModelRc::new(VecModel::from(cells)));
    }
    let title = if v.year_mode {
        format!("{} - {}", top * 4, top * 4 + 15)
    } else {
        (top * 4).div_euclid(12).to_string()
    };
    ui.set_cal_picker_title(title.into());
    ui.set_cal_pfrac((v.pcur - base as f64) as f32);
}

fn ease(cur: f64, target: f64) -> f64 {
    let diff = target - cur;
    if diff.abs() < 0.01 { target } else { cur + diff * 0.3 }
}

fn animate() {
    ANIM.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(16), || {
            let done = with_view(|v, ui| {
                v.cur = ease(v.cur, v.target);
                v.pcur = ease(v.pcur, v.ptarget);
                render(v, ui, false);
                render_picker(v, ui, false);
                v.cur == v.target && v.pcur == v.ptarget
            });
            if done != Some(false) {
                ANIM.with(|t| t.stop());
            }
        });
    });
}

fn go_to_month(v: &mut View, y: i32, m: i32) {
    let (y, m) = match m {
        0 => (y - 1, 12),
        13 => (y + 1, 1),
        _ => (y, m),
    };
    v.target = first_week_of_month(y, m as u32);
}

fn step_month(delta: i32) {
    with_view(|v, _| {
        let (y, m) = title_month(v.target);
        go_to_month(v, y, m as i32 + delta);
    });
    animate();
}

fn set_picker(v: &mut View, ui: &QuickPopup, open: bool) {
    v.picker_open = open;
    if !open {
        v.year_mode = false;
    }
    ui.set_cal_picker_open(open);
    ui.set_cal_year_mode(v.year_mode);
    render_picker(v, ui, true);
}

pub(crate) fn wire(ui: &QuickPopup) {
    VIEW.with(|v| v.borrow_mut().ui = Some(ui.as_weak()));
    ui.set_cal_weekday_names(ModelRc::new(VecModel::from(
        WEEKDAYS_SHORT.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>(),
    )));

    ui.on_cal_header_clicked(|| {
        with_view(|v, ui| {
            v.today = today();
            let (y, m, _) = civil_from_days(v.today);
            v.target = first_week_of_month(y, m);
            v.selected = None;
            set_picker(v, ui, false);
            render(v, ui, true);
        });
        animate();
    });
    ui.on_cal_prev(|| step_month(-1));
    ui.on_cal_next(|| step_month(1));
    ui.on_cal_title_clicked(|| {
        with_view(|v, ui| {
            if !v.picker_open {
                v.year_mode = false;
                let year = title_month(v.target).0;
                v.pcur = (year * 3) as f64;
                v.ptarget = v.pcur;
                set_picker(v, ui, true);
            } else if !v.year_mode {
                let year = ((v.pcur.round() as i32) * 4).div_euclid(12);
                v.year_mode = true;
                v.pcur = (year.div_euclid(4) - 1) as f64;
                v.ptarget = v.pcur;
                set_picker(v, ui, true);
            } else {
                let year = (v.pcur.round() as i32) * 4 + 4;
                v.year_mode = false;
                v.pcur = (year * 3) as f64;
                v.ptarget = v.pcur;
                set_picker(v, ui, true);
            }
        });
    });
    ui.on_cal_pick_cell(|key| {
        let month_picked = with_view(|v, ui| {
            if v.year_mode {
                v.year_mode = false;
                v.pcur = (key * 3) as f64;
                v.ptarget = v.pcur;
                set_picker(v, ui, true);
                false
            } else {
                go_to_month(v, key.div_euclid(12), key.rem_euclid(12) + 1);
                set_picker(v, ui, false);
                true
            }
        });
        if month_picked == Some(true) {
            animate();
        }
    });
    ui.on_cal_picker_step(|d| {
        with_view(|v, _| v.ptarget += (d * 3) as f64);
        animate();
    });
    ui.on_cal_day_clicked(|key| {
        with_view(|v, ui| {
            v.selected = Some(key);
            render(v, ui, false);
        });
    });
    ui.on_cal_scrolled(|delta| {
        let moved = with_view(|v, _| {
            v.wheel += delta;
            let steps = (v.wheel / WHEEL_STEP).trunc();
            v.wheel -= steps * WHEEL_STEP;
            // Wheel down moves forward.
            if v.picker_open {
                v.ptarget -= steps as f64;
            } else {
                v.target -= steps as f64;
            }
            steps != 0.0
        });
        if moved == Some(true) {
            animate();
        }
    });
}

// Resets to today's month each time the calendar opens.
pub(crate) fn reset(ui: &QuickPopup) {
    let t = today();
    let (y, m, d) = civil_from_days(t);
    ui.set_cal_header(format!("{}, {} {}", WEEKDAYS[weekday_mon0(t)], MONTHS[m as usize - 1], d).into());
    VIEW.with(|v| {
        let mut v = v.borrow_mut();
        v.today = t;
        v.cur = first_week_of_month(y, m);
        v.target = v.cur;
        v.selected = None;
        v.wheel = 0.0;
        v.year_mode = false;
        set_picker(&mut v, ui, false);
        render(&mut v, ui, true);
    });
}

pub(crate) fn stop() {
    ANIM.with(|t| t.stop());
}
