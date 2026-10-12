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
    CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler, FaviconChangedEventHandler, GetFaviconCompletedHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler,
    WebMessageReceivedEventHandler, take_pwstr,
};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{IStream, STREAM_SEEK_SET};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, PCWSTR, PWSTR, w};

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
    mail: Vec<MailItem>,
    shown: bool,
}

// One unread mail as shown in the bar's mail list.
#[derive(Clone, Debug)]
pub struct MailItem {
    pub from: String,
    pub title: String,
    pub url: String,
    pub preview: String,
}

struct Settings {
    base: PathBuf,
    log: fn(&str),
}

// Settings the adapters read at run time (as `window.__opts`).
#[derive(Clone, PartialEq, Debug)]
pub struct Opts {
    pub stale_hours: u32,
    pub teams_activity: bool,
    pub teams_threads: bool,
    pub poll_ms: u32,
    pub panel_widen: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { stale_hours: 24, teams_activity: true, teams_threads: true, poll_ms: 3000, panel_widen: true }
    }
}

fn opts_script(o: &Opts) -> String {
    format!(
        "window.__opts = {{stale: {}, activity: {}, threads: {}, poll: {}}};",
        o.stale_hours, o.teams_activity, o.teams_threads, o.poll_ms
    )
}

// Applies new options to every running page (adapters read them on each pass).
pub fn set_opts(o: Opts) {
    if OPTS.with(|x| *x.borrow() == o) {
        return;
    }
    let script = opts_script(&o);
    OPTS.with(|x| *x.borrow_mut() = o);
    let ids: Vec<String> = SVCS.with(|s| s.borrow().iter().map(|v| v.cfg.id.clone()).collect());
    for id in ids {
        run_script(&id, &script);
    }
}

thread_local! {
    static OPTS: RefCell<Opts> = RefCell::new(Opts::default());
    static SVCS: RefCell<Vec<Svc>> = const { RefCell::new(Vec::new()) };
    static ENVS: RefCell<HashMap<String, ICoreWebView2Environment>> = RefCell::new(HashMap::new());
    static PENDING: RefCell<HashMap<String, Vec<(ServiceCfg, HWND)>>> = RefCell::new(HashMap::new());
    static PANEL_SRC: RefCell<String> = const { RefCell::new(String::new()) };
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
    // Whether new mail makes this service flash.
    pub flash: bool,
    // The window's current icon (an HICON, 0 until the page's favicon arrives).
    pub icon: isize,
    pub mail: Vec<MailItem>,
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

// Closes every service of a group (and the mail panel if it uses their profile), so their browser processes exit.
pub fn stop_group(group: &str) {
    let gone: Vec<Svc> = SVCS.with(|s| {
        let mut s = s.borrow_mut();
        let profiles: Vec<String> = s.iter().filter(|v| v.cfg.group == group).map(|v| v.cfg.profile.clone()).collect();
        let (gone, keep): (Vec<Svc>, Vec<Svc>) = std::mem::take(&mut *s).into_iter().partition(|v| profiles.contains(&v.cfg.profile));
        *s = keep;
        gone
    });
    for v in gone {
        ENVS.with(|e| e.borrow_mut().remove(&v.cfg.profile));
        PENDING.with(|p| p.borrow_mut().remove(&v.cfg.profile));
        unsafe {
            if let Some(c) = &v.controller {
                let _ = c.Close();
            }
            let _ = DestroyWindow(v.hwnd);
        }
    }
}

// Closes one service's window and view; its profile stays loaded for the others that share it.
pub fn stop_service(id: &str) {
    let gone = SVCS.with(|s| {
        let mut s = s.borrow_mut();
        s.iter().position(|v| v.cfg.id == id).map(|i| s.remove(i))
    });
    if let Some(v) = gone {
        unsafe {
            if let Some(c) = &v.controller {
                let _ = c.Close();
            }
            let _ = DestroyWindow(v.hwnd);
        }
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

// Shows a service window and loads `url` in it (e.g. a specific mail).
pub fn open_url(id: &str, url: &str) {
    if url.starts_with("teams-chat:") {
        run_script(id, &format!("window.__openChat && window.__openChat('{url}')"));
        open(id);
        return;
    }
    let web = SVCS.with(|s| {
        s.borrow().iter().find(|v| v.cfg.id == id).and_then(|v| v.controller.as_ref().and_then(|c| unsafe { c.CoreWebView2().ok() }))
    });
    if let Some(web) = web {
        let u = wide(url);
        unsafe {
            let _ = web.Navigate(PCWSTR(u.as_ptr()));
        }
    }
    open(id);
}

const PANEL_ID: &str = "mailpanel";

fn panel_hwnd() -> Option<HWND> {
    SVCS.with(|s| s.borrow().iter().find(|v| v.cfg.id == PANEL_ID).map(|v| v.hwnd))
}

// Screen rect (left, top, right, bottom) of the mail panel while it is showing.
pub fn panel_rect() -> Option<(i32, i32, i32, i32)> {
    let hwnd = panel_hwnd()?;
    let mut r = RECT::default();
    unsafe {
        (IsWindowVisible(hwnd).as_bool() && GetWindowRect(hwnd, &mut r).is_ok()).then_some((r.left, r.top, r.right, r.bottom))
    }
}

pub fn hide_panel() {
    if let Some(hwnd) = panel_hwnd() {
        let shown = unsafe { IsWindowVisible(hwnd).as_bool() };
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        if shown {
            refresh_source();
        }
    }
}

// Makes the service that opened the panel re-read its unread mails (the opened one is now read).
fn refresh_source() {
    let id = PANEL_SRC.with(|p| p.borrow().clone());
    run_script(&id, "window.__loadMails && window.__loadMails()");
}

// Stops a service's attention flash (e.g. once the user looked at its new mail).
pub fn clear_attention(id: &str) {
    SVCS.with(|s| {
        if let Some(v) = s.borrow_mut().iter_mut().find(|v| v.cfg.id == id) {
            v.attention = false;
        }
    });
}

// Runs a script in a service's page.
pub fn run_script(id: &str, script: &str) {
    let web = SVCS.with(|s| {
        s.borrow().iter().find(|v| v.cfg.id == id).and_then(|v| v.controller.as_ref().and_then(|c| unsafe { c.CoreWebView2().ok() }))
    });
    if let Some(web) = web {
        let js = wide(script);
        let done = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
        unsafe {
            let _ = web.ExecuteScript(PCWSTR(js.as_ptr()), &done);
        }
    }
}

// Shows one mail on its own (no folders or toolbar) in a borderless window at `rect` (x, y, w, h).
// It uses the profile of service `id` and hides itself when it loses focus.
pub fn open_panel(id: &str, url: &str, rect: (i32, i32, i32, i32)) {
    let deep;
    let url = match url.strip_prefix("teams-chat:") {
        Some(conv) if conv.starts_with("slice-") => {
            open_url(id, url);
            return;
        }
        _ if url.starts_with("teams-thread:") => {
            open_url(id, url);
            return;
        }
        Some(conv) => {
            deep = format!("https://teams.cloud.microsoft/l/chat/{conv}/conversations");
            deep.as_str()
        }
        None => url,
    };
    let Some(mut cfg) = SVCS.with(|s| s.borrow().iter().find(|v| v.cfg.id == id).map(|v| v.cfg.clone())) else { return };
    PANEL_SRC.with(|p| *p.borrow_mut() = id.to_string());
    let existing = SVCS.with(|s| s.borrow().iter().find(|v| v.cfg.id == PANEL_ID).map(|v| (v.hwnd, v.controller.clone())));
    let hwnd = match existing {
        Some((hwnd, controller)) => {
            if let Some(web) = controller.and_then(|c| unsafe { c.CoreWebView2().ok() }) {
                let u = wide(url);
                unsafe {
                    let _ = web.Navigate(PCWSTR(u.as_ptr()));
                }
            }
            hwnd
        }
        None => {
            cfg.id = PANEL_ID.into();
            cfg.name = "Mail".into();
            cfg.group = "_panel".into();
            cfg.adapter = "panel".into();
            cfg.url = url.into();
            launch(&cfg, false);
            let Some(h) = SVCS.with(|s| s.borrow().iter().find(|v| v.cfg.id == PANEL_ID).map(|v| v.hwnd)) else { return };
            h
        }
    };
    unsafe {
        let ok = SetWindowPos(hwnd, Some(HWND_TOPMOST), rect.0, rect.1, rect.2, rect.3, SWP_SHOWWINDOW);
        log(&format!("panel: show at {rect:?} -> {ok:?}, visible {}", IsWindowVisible(hwnd).as_bool()));
        let _ = SetForegroundWindow(hwnd);
    }
}

// Tells each page whether its window is on screen (adapters may rearrange a hidden page).
fn sync_shown() {
    let changed: Vec<(String, bool)> = SVCS.with(|s| {
        s.borrow_mut()
            .iter_mut()
            .filter_map(|v| {
                let now = unsafe { IsWindowVisible(v.hwnd).as_bool() };
                (now != v.shown).then(|| {
                    v.shown = now;
                    (v.cfg.id.clone(), now)
                })
            })
            .collect()
    });
    for (id, shown) in changed {
        run_script(&id, &format!("window.__shown = {shown}"));
    }
}

pub fn status() -> Vec<Status> {
    sync_shown();
    SVCS.with(|s| {
        s.borrow()
            .iter()
            .filter(|v| v.cfg.id != PANEL_ID)
            .map(|v| Status {
                id: v.cfg.id.clone(),
                name: v.cfg.name.clone(),
                group: v.cfg.group.clone(),
                unread: v.unread,
                meeting: v.meeting,
                attention: v.attention,
                flash: v.cfg.flash,
                icon: unsafe {
                    SendMessageW(v.hwnd, WM_GETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(0))).0
                },
                mail: v.mail.clone(),
            })
            .collect()
    })
}

fn launch(cfg: &ServiceCfg, show: bool) {
    let panel = cfg.id == PANEL_ID;
    let Some(hwnd) = (unsafe { create_window_ex(&cfg.name, show, panel) }) else {
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
            mail: Vec::new(),
            shown: false,
        })
    });
    start_webview(cfg.clone(), hwnd);
}

// `panel` windows are borderless tool windows (no taskbar button) used for the mail panel.
unsafe fn create_window_ex(title: &str, show: bool, panel: bool) -> Option<HWND> {
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
        let (style, ex) = if panel {
            (WS_POPUP | WS_BORDER, WS_EX_TOOLWINDOW | WS_EX_TOPMOST)
        } else if show {
            (WS_OVERLAPPEDWINDOW | WS_VISIBLE, WINDOW_EX_STYLE(0))
        } else {
            (WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE(0))
        };
        let hwnd = CreateWindowExW(
            ex,
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
            "panel" => include_str!("adapters/panel.js"),
            _ => include_str!("adapters/generic.js"),
        }
        .to_string()
    });
    let opts = OPTS.with(|o| opts_script(&o.borrow()));
    format!("{opts}\n{PRELUDE}\n{body}")
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

        // The page's favicon becomes the window (and so the taskbar button) icon.
        if let Ok(web15) = web.cast::<ICoreWebView2_15>() {
            let changed = FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(web15) = sender.and_then(|w| w.cast::<ICoreWebView2_15>().ok()) {
                    request_icon(&web15, hwnd);
                }
                Ok(())
            }));
            let _ = web15.add_FaviconChanged(&changed, &mut token);
        }
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

fn request_icon(web: &ICoreWebView2_15, hwnd: HWND) {
    let done = GetFaviconCompletedHandler::create(Box::new(move |res, stream| {
        if res.is_ok()
            && let Some(bytes) = stream.and_then(|s| read_stream(&s))
        {
            set_window_icon(hwnd, &bytes);
        }
        Ok(())
    }));
    unsafe {
        let _ = web.GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &done);
    }
}

fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 16 * 1024];
    unsafe {
        let _ = stream.Seek(0, STREAM_SEEK_SET, None);
        loop {
            let mut read = 0u32;
            if stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read)).is_err() || read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
        }
    }
    (!out.is_empty()).then_some(out)
}

// PNG data is accepted directly as icon resource bits.
fn set_window_icon(hwnd: HWND, png: &[u8]) {
    unsafe {
        for (kind, size) in [(ICON_BIG, 32), (ICON_SMALL, 16)] {
            let Ok(icon) = CreateIconFromResourceEx(png, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR) else {
                continue;
            };
            let prev = SendMessageW(hwnd, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(icon.0 as isize)));
            if prev.0 != 0 {
                let _ = DestroyIcon(HICON(prev.0 as *mut _));
            }
        }
    }
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

// A dark page gets a dark title bar in the page's own colour; a light page keeps the default one.
fn set_caption_theme(hwnd: HWND, (r, g, b): (u32, u32, u32)) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let dark = (r * 299 + g * 587 + b * 114) / 1000 < 128;
    let flag = dark as u32;
    let (caption, text) = if dark { (r | g << 8 | b << 16, 0x00ff_ffff_u32) } else { (0xffff_ffff, 0xffff_ffff) };
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &flag as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &text as *const _ as *const _, 4);
    }
}

// Widens the panel to fit its content (`client_w` physical px), growing away from the screen edge side it is nearest to.
fn grow_panel(hwnd: HWND, client_w: i32) {
    unsafe {
        let mut r = RECT::default();
        let mut c = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() || GetClientRect(hwnd, &mut c).is_err() {
            return;
        }
        let mon = windows::Win32::Graphics::Gdi::MonitorFromWindow(hwnd, windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST);
        let mut mi = windows::Win32::Graphics::Gdi::MONITORINFO { cbSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32, ..Default::default() };
        if !windows::Win32::Graphics::Gdi::GetMonitorInfoW(mon, &mut mi).as_bool() {
            return;
        }
        let frame = (r.right - r.left) - (c.right - c.left);
        let width = (client_w + frame).min(mi.rcMonitor.right - mi.rcMonitor.left);
        if width <= r.right - r.left {
            return;
        }
        let mid = (mi.rcMonitor.left + mi.rcMonitor.right) / 2;
        let mut left = if r.right > mid { r.right - width } else { r.left };
        left = left.clamp(mi.rcMonitor.left, mi.rcMonitor.right - width);
        let _ = SetWindowPos(hwnd, None, left, r.top, width, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
// Messages from adapters: "unread|meeting" (meeting is 0 or 1) or "log|text".
fn on_message(id: &str, msg: &str) {
    let Some((a, b)) = msg.split_once('|') else { return };
    if a == "log" {
        log(&format!("[{id}] {b}"));
        return;
    }
    if a == "theme" {
        let v: Vec<u32> = b.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [r, g, bl] = v[..] {
            let hwnd = SVCS.with(|s| s.borrow().iter().find(|v| v.cfg.id == id).map(|v| v.hwnd));
            if let Some(hwnd) = hwnd {
                set_caption_theme(hwnd, (r, g, bl));
            }
        }
        return;
    }
    if a == "panelw" {
        if let (true, Ok(w), Some(hwnd)) = (OPTS.with(|o| o.borrow().panel_widen), b.parse::<i32>(), panel_hwnd()) {
            grow_panel(hwnd, w);
        }
        return;
    }
    if a == "items" {
        let mail = b
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\t');
                Some(MailItem { from: p.next()?.into(), title: p.next()?.into(), url: p.next()?.into(), preview: p.next().unwrap_or_default().into() })
            })
            .collect();
        SVCS.with(|s| {
            if let Some(v) = s.borrow_mut().iter_mut().find(|v| v.cfg.id == id) {
                v.mail = mail;
            }
        });
        return;
    }
    if a == "name" {
        SVCS.with(|s| {
            if let Some(v) = s.borrow_mut().iter_mut().find(|v| v.cfg.id == id) {
                v.cfg.name = b.to_string();
                set_title(v);
            }
        });
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
            set_title(v);
        }
    });
}

// Window title (shown on the taskbar button): "(5) name" while something is unread.
fn set_title(v: &Svc) {
    let title = if v.unread > 0 { format!("({}) {}", v.unread, v.cfg.name) } else { v.cfg.name.clone() };
    let t = wide(&title);
    unsafe {
        let _ = SetWindowTextW(v.hwnd, PCWSTR(t.as_ptr()));
    }
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
                if (wp.0 & 0xffff) == 0 {
                    let panel = SVCS.with(|s| s.borrow().iter().any(|v| v.hwnd == hwnd && v.cfg.id == PANEL_ID));
                    if panel {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                        refresh_source();
                    }
                }
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
