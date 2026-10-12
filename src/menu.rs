use crate::prelude::*;

// ---------- context menus ----------

#[derive(Default, Clone)]
pub(crate) struct AppInfo {
    pub(crate) hwnd: HWND,
    pub(crate) pid: u32,
    pub(crate) exe: String,
    pub(crate) pinned: bool,
    pub(crate) device: String,
}

pub(crate) struct MenuItem {
    pub(crate) id: usize,
    pub(crate) label: String,
    pub(crate) enabled: bool,
}

// Each provider contributes one group of items. To add app-specific entries later,
// implement this trait, gate it in `applies_to` (by exe, window class, ...) and
// add it to `app_providers`.
pub(crate) trait MenuProvider {
    fn applies_to(&self, info: &AppInfo) -> bool;
    fn items(&self, info: &AppInfo) -> Vec<MenuItem>;
    fn run(&self, id: usize, info: &AppInfo);
}

pub(crate) struct WindowProvider;
pub(crate) struct PinProvider;
pub(crate) struct BarProvider;
pub(crate) struct StartProvider;

pub(crate) fn is_protected(info: &AppInfo) -> bool {
    let name = file_name(&info.exe);
    info.pid == unsafe { GetCurrentProcessId() } || name == "explorer.exe"
}

impl MenuProvider for WindowProvider {
    fn applies_to(&self, info: &AppInfo) -> bool {
        !info.hwnd.0.is_null()
    }
    fn items(&self, info: &AppInfo) -> Vec<MenuItem> {
        let running = !info.hwnd.0.is_null();
        vec![
            MenuItem { id: 2, label: "End Task".into(), enabled: running && !is_protected(info) },
            MenuItem { id: 1, label: "Close Window".into(), enabled: running },
        ]
    }
    fn run(&self, id: usize, info: &AppInfo) {
        unsafe {
            match id {
                1 => {
                    let _ = PostMessageW(Some(info.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                2 => {
                    if let Some(h) = open_process(PROCESS_TERMINATE, info.pid) {
                                    let _ = TerminateProcess(h.0, 1);
                                }
                }
                _ => {}
            }
        }
    }
}

impl MenuProvider for PinProvider {
    fn applies_to(&self, info: &AppInfo) -> bool {
        info.exe != START_EXE
    }
    fn items(&self, info: &AppInfo) -> Vec<MenuItem> {
        let label = if info.pinned { "Unpin from Taskbar" } else { "Pin to Taskbar" };
        // UWP windows are hosted by ApplicationFrameHost, so the exe isn't the app.
        let pinnable = !info.exe.is_empty() && file_name(&info.exe) != "applicationframehost.exe";
        let icon_only = STATE.with(|s| s.borrow().icon_only.contains(&file_name(&info.exe)));
        let mut items = vec![MenuItem { id: 1, label: label.into(), enabled: pinnable || info.pinned }];
        if pinnable {
            let l = if icon_only { "Show Label" } else { "Show Icon Only" };
            items.push(MenuItem { id: 2, label: l.into(), enabled: true });
        }
        items
    }
    fn run(&self, id: usize, info: &AppInfo) {
        if id == 2 {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                let name = file_name(&info.exe);
                if !s.icon_only.remove(&name) {
                    s.icon_only.insert(name);
                }
                save_icon_only(&s.icon_only);
            });
            for m in bar_keys() {
                push_ui(m);
            }
            return;
        }
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let same = |p: &(String, String)| p.0 == info.device && p.1.eq_ignore_ascii_case(&info.exe);
            match s.pinned.iter().position(same) {
                Some(pos) => {
                    s.pinned.remove(pos);
                }
                None => s.pinned.push((info.device.clone(), info.exe.clone())),
            }
            save_pins(&s.pinned);
        });
    }
}

impl MenuProvider for BarProvider {
    fn applies_to(&self, _: &AppInfo) -> bool {
        true
    }
    fn items(&self, info: &AppInfo) -> Vec<MenuItem> {
        let mut items = vec![MenuItem { id: 2, label: "Quit".into(), enabled: true }];
        items.push(MenuItem { id: 4, label: "Settings...".into(), enabled: true });
        if !info.device.is_empty() && !monitor_cfg(&info.device).show_start {
            items.push(MenuItem { id: 5, label: "Show Start Button".into(), enabled: true });
        }
        items.push(MenuItem { id: 1, label: "Task Manager".into(), enabled: true });
        items
    }
    fn run(&self, id: usize, info: &AppInfo) {
        unsafe {
            match id {
                4 => {
                    let _ = slint::invoke_from_event_loop(crate::settings::open_settings);
                }
                5 => set_start_visible(&info.device, true),
                1 => {
                    ShellExecuteW(
                        None,
                        w!("open"),
                        w!("taskmgr.exe"),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    );
                }
                2 => quit_all(),
                _ => {}
            }
        }
    }
}

pub(crate) fn app_providers() -> Vec<Box<dyn MenuProvider>> {
    vec![Box::new(StartProvider), Box::new(PinProvider), Box::new(WindowProvider)]
}

pub(crate) fn set_start_visible(device: &str, visible: bool) {
    update_monitor(device, |c| c.show_start = visible);
}

impl MenuProvider for StartProvider {
    fn applies_to(&self, info: &AppInfo) -> bool {
        info.exe == START_EXE
    }
    fn items(&self, _: &AppInfo) -> Vec<MenuItem> {
        vec![MenuItem { id: 1, label: "Hide Start Button".into(), enabled: true }]
    }
    fn run(&self, _: usize, info: &AppInfo) {
        set_start_visible(&info.device, false);
    }
}

pub(crate) fn bar_providers() -> Vec<Box<dyn MenuProvider>> {
    vec![Box::new(BarProvider)]
}

// Entry ids encode the provider: (index + 1) * 100 + the provider's own id.
pub(crate) fn menu_entries(providers: &[Box<dyn MenuProvider>], info: &AppInfo) -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::new();
    for (i, p) in providers.iter().enumerate() {
        if !p.applies_to(info) {
            continue;
        }
        let items = p.items(info);
        if items.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(MenuEntry { label: SharedString::new(), enabled: false, separator: true, id: 0 });
        }
        for item in items {
            out.push(MenuEntry {
                label: item.label.as_str().into(),
                enabled: item.enabled,
                separator: false,
                id: ((i + 1) * 100 + item.id) as i32,
            });
        }
    }
    out
}

pub(crate) fn run_menu_command(providers: &[Box<dyn MenuProvider>], info: &AppInfo, cmd: usize) {
    if cmd >= 100
        && let Some(p) = providers.get(cmd / 100 - 1) {
            p.run(cmd % 100, info);
        }
}

pub(crate) fn app_info(e: &Entry, device: &str) -> AppInfo {
    AppInfo {
        hwnd: e.hwnd,
        pid: e.pid,
        exe: e.exe.clone(),
        pinned: e.pinned,
        device: device.to_string(),
    }
}
