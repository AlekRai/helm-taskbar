use super::*;

// ---------- window enumeration ----------

pub(crate) fn is_taskbar_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || is_ours(hwnd) {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).map(|h| !h.0.is_null()).unwrap_or(false) {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        );
        cloaked == 0
    }
}

pub(crate) unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let out = &mut *(lparam.0 as *mut Vec<(HWND, String)>);
        if !is_taskbar_window(hwnd) {
            return TRUE;
        }
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if title != "Program Manager" {
                out.push((hwnd, title));
            }
        }
        TRUE
    }
}

// Cached pid/exe and icon for a window; the caches are dropped once the window is gone.
unsafe fn window_entry(h: HWND, title: String) -> Entry {
    unsafe {
        let key = h.0 as isize;
        let cached = STATE.with(|s| s.borrow().info_cache.get(&key).cloned());
        let (pid, exe) = cached.unwrap_or_else(|| {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let info = (pid, exe_of(pid));
            STATE.with(|s| s.borrow_mut().info_cache.insert(key, info.clone()));
            info
        });
        let mut icon = STATE.with(|s| s.borrow().icon_cache.get(&key).copied()).unwrap_or_default();
        if icon.0.is_null() {
            icon = HICON::default();
            if file_name(&exe) == "applicationframehost.exe" {
                icon = uwp_icon(h);
            }
            if icon.0.is_null() {
                icon = window_icon(h);
            }
            if icon.0.is_null() {
                icon = exe_icon(&exe);
            }
            if !icon.0.is_null() {
                STATE.with(|s| s.borrow_mut().icon_cache.insert(key, icon));
            }
        }
        if let Some(i) = override_icon(&exe, &title) {
            icon = i;
        }
        Entry { hwnd: h, title, exe, pid, icon, pinned: false }
    }
}

// Pinned apps first in pin order (a pin owns all its running windows, or shows closed), then the rest.
unsafe fn merge_pins(running: Vec<Entry>, pinned: &[String]) -> Vec<Entry> {
    unsafe {
        let mut entries: Vec<Entry> = Vec::new();
        let mut used = vec![false; running.len()];
        for pin in pinned {
            let mut found = false;
            for (i, e) in running.iter().enumerate() {
                if e.exe.eq_ignore_ascii_case(pin) {
                    let mut e = e.clone();
                    e.pinned = true;
                    entries.push(e);
                    used[i] = true;
                    found = true;
                }
            }
            if !found {
                entries.push(Entry {
                    hwnd: HWND::default(),
                    title: file_stem(pin),
                    exe: pin.clone(),
                    pid: 0,
                    icon: exe_icon(pin),
                    pinned: true,
                });
            }
        }
        for (i, e) in running.into_iter().enumerate() {
            if !used[i] {
                entries.push(e);
            }
        }
        entries
    }
}

fn start_entry() -> Entry {
    Entry {
        hwnd: HWND::default(),
        title: "Start".into(),
        exe: START_EXE.into(),
        pid: 0,
        icon: HICON::default(),
        pinned: false,
    }
}

// Windows without a session rank get one from the saved order: same window first, then same title.
fn claim_saved_ranks(entries: &[Entry]) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.saved_windows.is_empty() {
            return;
        }
        let saved = s.saved_windows.clone();
        let mut claimed: HashSet<usize> = HashSet::new();
        let live: Vec<&Entry> = entries.iter().filter(|e| !e.hwnd.0.is_null()).collect();
        for pass in 0..2 {
            for e in &live {
                let key = e.hwnd.0 as isize;
                if s.win_rank.contains_key(&key) {
                    continue;
                }
                let found = saved.iter().enumerate().find(|(i, r)| {
                    !claimed.contains(i)
                        && r.exe.eq_ignore_ascii_case(&e.exe)
                        && if pass == 0 { r.hwnd == key && r.pid == e.pid } else { r.title == e.title }
                });
                if let Some((i, r)) = found {
                    claimed.insert(i);
                    s.win_rank.insert(key, r.rank);
                }
            }
        }
    });
}

// Reorders the windows of each exe among their own slots by session rank.
fn rank_within_exe(entries: Vec<Entry>) -> Vec<Entry> {
    let ranks = STATE.with(|s| s.borrow().win_rank.clone());
    let mut by_exe: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        by_exe.entry(e.exe.to_lowercase()).or_default().push(i);
    }
    let mut sorted = entries.clone();
    for idxs in by_exe.values() {
        let mut group: Vec<Entry> = idxs.iter().map(|&i| entries[i].clone()).collect();
        group.sort_by_key(|e| ranks.get(&(e.hwnd.0 as isize)).copied().unwrap_or(usize::MAX));
        for (&i, e) in idxs.iter().zip(group) {
            sorted[i] = e;
        }
    }
    sorted
}

// User-chosen order wins; an exe listed N times places its first N windows. Others go last.
fn apply_layout(entries: Vec<Entry>, layout: &[String]) -> Vec<Entry> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut keyed: Vec<(usize, Entry)> = entries
        .into_iter()
        .map(|e| {
            let exe = e.exe.to_lowercase();
            let default_pos = if exe == START_EXE { 0 } else { usize::MAX };
            let n = seen.entry(exe.clone()).or_insert(0);
            let pos = layout.iter().enumerate().filter(|(_, x)| **x == exe).nth(*n).map(|(i, _)| i);
            *n += 1;
            (pos.unwrap_or(default_pos), e)
        })
        .collect();
    keyed.sort_by_key(|(p, _)| *p);
    keyed.into_iter().map(|(_, e)| e).collect()
}

pub(crate) unsafe fn refresh(monitor: isize) {
    unsafe {
        let Some((m, device)) = with_bar(monitor, |b| (b.monitor, b.device.clone())) else { return };

        let mut raw: Vec<(HWND, String)> = Vec::new();
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut raw as *mut _ as isize));

        let mut fg = GetForegroundWindow();
        let minute = GetLocalTime().wMinute;

        // Stable button order: new windows go to the end instead of following Z-order.
        let live: HashSet<isize> = raw.iter().map(|(h, _)| h.0 as isize).collect();
        let (pinned, order, layout) = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.order.retain(|h| live.contains(h));
            for (h, _) in &raw {
                if !s.order.contains(&(h.0 as isize)) {
                    s.order.push(h.0 as isize);
                }
            }
            s.info_cache.retain(|h, _| live.contains(h));
            s.icon_cache.retain(|h, _| live.contains(h));
            let pins: Vec<String> =
                s.pinned.iter().filter(|(d, _)| *d == device).map(|(_, e)| e.clone()).collect();
            let layout: Vec<String> =
                s.layout.iter().filter(|(d, _)| *d == device).map(|(_, e)| e.to_lowercase()).collect();
            (pins, s.order.clone(), layout)
        });

        // Only windows on this bar's monitor count; pinned apps without one show as closed.
        let mut running: Vec<Entry> = raw
            .into_iter()
            .filter(|(h, _)| window_monitor(*h) == m)
            .map(|(h, title)| window_entry(h, title))
            .collect();
        running.sort_by_key(|e| order.iter().position(|&o| o == e.hwnd.0 as isize));

        let mut entries = merge_pins(running, &pinned);
        // Movable Start button; first unless the user dragged it elsewhere.
        if monitor_cfg(&device).show_start {
            entries.insert(0, start_entry());
        }
        claim_saved_ranks(&entries);
        let entries = apply_layout(rank_within_exe(entries), &layout);

        let fg_is_ours = is_ours(fg);
        let changed = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let Some(b) = s.bars.get_mut(&monitor) else { return false };
            // While one of our windows has focus, keep the last selection.
            if fg_is_ours {
                fg = b.foreground;
            }
            let changed = b.entries != entries || b.foreground != fg || b.minute != minute;
            b.entries = entries;
            b.foreground = fg;
            b.minute = minute;
            changed
        });
        if changed {
            push_ui(monitor);
        }
    }
}
