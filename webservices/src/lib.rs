//! Web services (mail, chat) hosted in WebView2 windows. Closing a window only hides it, so the
//! page keeps running and its adapter script can report unread counts.
//!
//! Everything must be called from one thread that pumps Win32 messages.

mod cfg;

pub use cfg::{ServiceCfg, default_services};

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, NewWindowRequestedEventHandler, PermissionRequestedEventHandler,
    WebMessageReceivedEventHandler, take_pwstr,
};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, PWSTR, w};

const CLASS: PCWSTR = w!("RustTaskbarService");
const PRELUDE: &str = include_str!("adapters/prelude.js");

struct Svc {
    cfg: ServiceCfg,
    hwnd: HWND,
    controller: Option<ICoreWebView2Controller>,
    unread: u32,
    meeting: bool,
    attention: bool,
    reported: bool,
}

struct Settings {
    base: PathBuf,
    log: fn(&str),
}

thread_local! {
    static SVCS: RefCell<Vec<Svc>> = const { RefCell::new(Vec::new()) };
    static ENVS: RefCell<HashMap<String, ICoreWebView2Environment>> = RefCell::new(HashMap::new());
    static PENDING: RefCell<HashMap<String, Vec<(ServiceCfg, HWND)>>> = RefCell::new(HashMap::new());
    static SETTINGS: RefCell<Settings> = RefCell::new(Settings { base: PathBuf::new(), log: |_| {} });
}

// What the bar needs to know about one service.
#[derive(Clone, Debug)]
pub struct Status {
    pub id: String,
    pub name: String,
    pub group: String,
    pub unread: u32,
    pub meeting: bool,
    pub attention: bool,
}

fn log(msg: &str) {
    let f = SETTINGS.with(|s| s.borrow().log);
    f(msg);
}

// `base` holds the browser profiles (`base\web`) and optional adapter overrides (`base\services`).
// `debug` exposes the DevTools protocol on a random port (see DevToolsActivePort in the profile).
pub fn init(base: PathBuf, debug: bool, log: fn(&str)) {
    SETTINGS.with(|s| *s.borrow_mut() = Settings { base, log });
    // WebView2 needs an apartment-threaded COM thread; harmless if the host already set one up.
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    let mut args = String::from(
        "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows",
    );
    if debug {
        args.push_str(" --remote-debugging-port=0");
    }
    // SAFETY: called once at startup, before any other thread reads the environment.
    unsafe { std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", args) };
}

// Creates a window (hidden unless `show`) and web view for every enabled service not yet running.
pub fn start(cfgs: &[ServiceCfg], show: bool) {
    for cfg in cfgs.iter().filter(|c| c.enabled) {
        if SVCS.with(|s| s.borrow().iter().any(|v| v.cfg.id == cfg.id)) {
            continue;
        }
        launch(cfg, show);
    }
}

// Shows a service window, bringing it to the front.
pub fn open(id: &str) {
    let hwnd = SVCS.with(|s| s.borrow().iter().find(|v| v.cfg.id == id).map(|v| v.hwnd));
    let Some(hwnd) = hwnd else { return };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

pub fn status() -> Vec<Status> {
    SVCS.with(|s| {
        s.borrow()
            .iter()
            .map(|v| Status {
                id: v.cfg.id.clone(),
                name: v.cfg.name.clone(),
                group: v.cfg.group.clone(),
                unread: v.unread,
                meeting: v.meeting,
                attention: v.attention,
            })
            .collect()
    })
}

fn launch(cfg: &ServiceCfg, show: bool) {
    let Some(hwnd) = (unsafe { create_window(&cfg.name, show) }) else {
        log(&format!("service {}: window creation failed", cfg.id));
        return;
    };
    SVCS.with(|s| {
        s.borrow_mut().push(Svc {
            cfg: cfg.clone(),
            hwnd,
            controller: None,
            unread: 0,
            meeting: false,
            attention: false,
            reported: false,
        })
    });
    start_webview(cfg.clone(), hwnd);
}

unsafe fn create_window(title: &str, show: bool) -> Option<HWND> {
    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let style = if show { WS_OVERLAPPEDWINDOW | WS_VISIBLE } else { WS_OVERLAPPEDWINDOW };
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR(wide.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            800,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .ok()?;
        if show {
            // The process's first ShowWindow call obeys STARTUPINFO, so a hidden launch would hide this one.
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        Some(hwnd)
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn start_webview(cfg: ServiceCfg, hwnd: HWND) {
    if let Some(env) = ENVS.with(|e| e.borrow().get(&cfg.profile).cloned()) {
        create_controller(cfg, hwnd, env);
        return;
    }
    // Services sharing a profile wait for the first one's environment instead of racing it.
    let first = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let waiting = p.entry(cfg.profile.clone()).or_default();
        waiting.push((cfg.clone(), hwnd));
        waiting.len() == 1
    });
    if !first {
        return;
    }
    let dir = SETTINGS.with(|s| s.borrow().base.join("web").join(&cfg.profile));
    let _ = std::fs::create_dir_all(&dir);
    let dir_w = wide(&dir.to_string_lossy());
    let profile = cfg.profile.clone();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |res, env| {
        let waiting = PENDING.with(|p| p.borrow_mut().remove(&profile)).unwrap_or_default();
        match (res, env) {
            (Ok(()), Some(env)) => {
                ENVS.with(|e| e.borrow_mut().insert(profile.clone(), env.clone()));
                for (cfg, hwnd) in waiting {
                    create_controller(cfg, hwnd, env.clone());
                }
            }
            (res, _) => log(&format!("profile {profile}: WebView2 environment failed: {res:?}")),
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), PCWSTR(dir_w.as_ptr()), None, &handler) {
            log(&format!("WebView2 environment start failed: {e}"));
        }
    }
}

fn create_controller(cfg: ServiceCfg, hwnd: HWND, env: ICoreWebView2Environment) {
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |res, controller| {
        match (res, controller) {
            (Ok(()), Some(controller)) => attach(&cfg, hwnd, controller),
            _ => log(&format!("service {}: WebView2 controller failed", cfg.id)),
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = env.CreateCoreWebView2Controller(hwnd, &handler) {
            log(&format!("WebView2 controller start failed: {e}"));
        }
    }
}

// Adapter script for a service: a user override file first, then the built-in one.
fn adapter_script(cfg: &ServiceCfg) -> String {
    let base = SETTINGS.with(|s| s.borrow().base.clone());
    let custom = base.join("services").join(format!("{}.js", cfg.id));
    let body = std::fs::read_to_string(custom).unwrap_or_else(|_| {
        match cfg.adapter.as_str() {
            "gmail" => include_str!("adapters/gmail.js"),
            "outlook" => include_str!("adapters/outlook.js"),
            "messenger" => include_str!("adapters/messenger.js"),
            "teams" => include_str!("adapters/teams.js"),
            _ => include_str!("adapters/generic.js"),
        }
        .to_string()
    });
    format!("{PRELUDE}\n{body}")
}

fn attach(cfg: &ServiceCfg, hwnd: HWND, controller: ICoreWebView2Controller) {
    unsafe {
        let Ok(web) = controller.CoreWebView2() else { return };
        let mut token = 0i64;

        // Calls and screen sharing need mic, camera and screen permissions without prompts.
        let perms = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
            }
            Ok(())
        }));
        let _ = web.add_PermissionRequested(&perms, &mut token);

        // Sign-in pop-ups load in the same view, same-site and Microsoft links open normally,
        // and everything else goes to the default browser.
        let own = site(&cfg.url);
        let popups = NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            args.Uri(&mut uri)?;
            let uri = take_pwstr(uri);
            if uri.is_empty() || uri == "about:blank" {
                return Ok(());
            }
            let host = host_of(&uri);
            if is_auth(&host) {
                args.SetHandled(true)?;
                if let Some(web) = sender {
                    let u = wide(&uri);
                    let _ = web.Navigate(PCWSTR(u.as_ptr()));
                }
            } else if site(&uri) != own && !is_microsoft(&host) {
                args.SetHandled(true)?;
                let u = wide(&uri);
                ShellExecuteW(None, w!("open"), PCWSTR(u.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
            }
            Ok(())
        }));
        let _ = web.add_NewWindowRequested(&popups, &mut token);

        let id = cfg.id.clone();
        let messages = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let mut msg = PWSTR::null();
                if args.TryGetWebMessageAsString(&mut msg).is_ok() {
                    on_message(&id, &take_pwstr(msg));
                }
            }
            Ok(())
        }));
        let _ = web.add_WebMessageReceived(&messages, &mut token);

        // Navigate only once the script is registered, or the first page load could miss it.
        let script = wide(&adapter_script(cfg));
        let url = wide(&cfg.url);
        let nav = web.clone();
        let added = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |_, _| {
            let _ = nav.Navigate(PCWSTR(url.as_ptr()));
            Ok(())
        }));
        let _ = web.AddScriptToExecuteOnDocumentCreated(PCWSTR(script.as_ptr()), &added);

        let _ = controller.SetIsVisible(true);
        resize(&controller, hwnd);
    }
    SVCS.with(|s| {
        if let Some(v) = s.borrow_mut().iter_mut().find(|v| v.cfg.id == cfg.id) {
            v.controller = Some(controller);
        }
    });
}

fn host_of(uri: &str) -> String {
    uri.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("").to_lowercase()
}

// The last two labels of the host, e.g. "google.com".
fn site(uri: &str) -> String {
    let host = host_of(uri);
    let labels: Vec<&str> = host.split('.').collect();
    labels[labels.len().saturating_sub(2)..].join(".")
}

fn host_in(host: &str, list: &[&str]) -> bool {
    list.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

fn is_auth(host: &str) -> bool {
    host_in(host, &["accounts.google.com", "login.microsoftonline.com", "login.live.com", "facebook.com"])
}

fn is_microsoft(host: &str) -> bool {
    host_in(host, &["microsoft.com", "office.com", "cloud.microsoft", "live.com", "sharepoint.com", "office365.com", "microsoftonline.com"])
}

// Messages from adapters: "unread|meeting" (meeting is 0 or 1) or "log|text".
fn on_message(id: &str, msg: &str) {
    let Some((a, b)) = msg.split_once('|') else { return };
    if a == "log" {
        log(&format!("[{id}] {b}"));
        return;
    }
    let (Ok(unread), meeting) = (a.parse::<u32>(), b == "1") else { return };
    SVCS.with(|s| {
        if let Some(v) = s.borrow_mut().iter_mut().find(|v| v.cfg.id == id) {
            let more = unread > v.unread || (meeting && !v.meeting);
            if v.reported && more {
                v.attention = true;
            }
            v.reported = true;
            v.unread = unread;
            v.meeting = meeting;
        }
    });
}

unsafe fn resize(controller: &ICoreWebView2Controller, hwnd: HWND) {
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let _ = controller.SetBounds(rc);
    }
}

fn controller_of(hwnd: HWND) -> Option<ICoreWebView2Controller> {
    SVCS.with(|s| s.borrow().iter().find(|v| v.hwnd == hwnd).and_then(|v| v.controller.clone()))
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_SIZE => {
                if let Some(c) = controller_of(hwnd) {
                    resize(&c, hwnd);
                }
                LRESULT(0)
            }
            WM_SETFOCUS => {
                if let Some(c) = controller_of(hwnd) {
                    let _ = c.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                }
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if (wp.0 & 0xffff) != 0 {
                    SVCS.with(|s| {
                        if let Some(v) = s.borrow_mut().iter_mut().find(|v| v.hwnd == hwnd) {
                            v.attention = false;
                        }
                    });
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
