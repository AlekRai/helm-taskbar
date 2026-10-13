//! Idle CPU profiler, active when `dev = true`: times the periodic callbacks and logs per-thread CPU every 10 s.
use crate::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

thread_local! {
    static SECTIONS: RefCell<HashMap<&'static str, (u64, f64)>> = RefCell::new(HashMap::new());
    static LAST: RefCell<Option<(Instant, f64, HashMap<u32, f64>)>> = const { RefCell::new(None) };
}

// Runs `f`, adding its wall time to the section `name`.
pub(crate) fn t<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    if !load_svc().dev {
        return f();
    }
    let start = Instant::now();
    let r = f();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    SECTIONS.with(|s| {
        let mut s = s.borrow_mut();
        let e = s.entry(name).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += ms;
    });
    r
}

fn ft_ms(a: FILETIME, b: FILETIME) -> f64 {
    let v = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 10_000.0;
    v(a) + v(b)
}

// CPU milliseconds used so far by every thread of this process.
fn thread_cpu() -> HashMap<u32, f64> {
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next};
    use windows::Win32::System::Threading::{GetCurrentProcessId, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION};
    let mut out = HashMap::new();
    unsafe {
        let pid = GetCurrentProcessId();
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else { return out };
        let mut e = THREADENTRY32 { dwSize: std::mem::size_of::<THREADENTRY32>() as u32, ..Default::default() };
        let mut ok = Thread32First(snap, &mut e).is_ok();
        while ok {
            if e.th32OwnerProcessID == pid
                && let Ok(h) = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, e.th32ThreadID)
            {
                let (mut c, mut x, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
                if GetThreadTimes(h, &mut c, &mut x, &mut k, &mut u).is_ok() {
                    out.insert(e.th32ThreadID, ft_ms(k, u));
                }
                let _ = CloseHandle(h);
            }
            ok = Thread32Next(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

// Logs the CPU share per thread and per timed section since the last report.
pub(crate) fn report() {
    if !load_svc().dev {
        return;
    }
    let now = Instant::now();
    let cur = thread_cpu();
    let total: f64 = cur.values().sum();
    let ui = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let Some((then, prev_total, prev)) = LAST.with(|l| l.borrow_mut().replace((now, total, cur.clone()))) else { return };
    let wall = now.duration_since(then).as_secs_f64() * 1000.0;
    if wall < 9_000.0 {
        LAST.with(|l| *l.borrow_mut() = Some((then, prev_total, prev)));
        return;
    }
    let mut threads: Vec<(u32, f64)> = cur.iter().map(|(id, v)| (*id, v - prev.get(id).copied().unwrap_or(0.0))).collect();
    threads.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut line = format!("prof: process {:.2}% of one core;", (total - prev_total) / wall * 100.0);
    for (id, ms) in threads.iter().take(5) {
        line += &format!(" thread {}{} {:.2}%;", id, if *id == ui { " (UI)" } else { "" }, ms / wall * 100.0);
    }
    log(&line);
    let mut secs: Vec<(&str, u64, f64)> = SECTIONS.with(|s| s.borrow_mut().drain().map(|(k, v)| (k, v.0, v.1)).collect());
    secs.sort_by(|a, b| b.2.total_cmp(&a.2));
    let timed: f64 = secs.iter().map(|s| s.2).sum();
    for (name, calls, ms) in secs {
        log(&format!("prof:   {name}: {calls} calls, {:.1} ms total, {:.2} ms/call, {:.2}% of one core", ms, ms / calls as f64, ms / wall * 100.0));
    }
    let ui_ms = threads.iter().find(|t| t.0 == ui).map_or(0.0, |t| t.1);
    log(&format!("prof:   UI thread outside timed sections (rendering, event loop): {:.2}% of one core", (ui_ms - timed).max(0.0) / wall * 100.0));
}
