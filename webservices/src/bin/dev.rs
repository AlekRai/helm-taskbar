//! Standalone runner for developing the web services without building the taskbar.
//!
//! `cargo run -p webservices --bin dev -- [--debug] [--show gmail1,teams|all] [--only teams,outlook]`

use std::path::PathBuf;
use windows::Win32::UI::WindowsAndMessaging::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let list = |s: Option<String>| s.map(|s| s.split(',').map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let debug = args.iter().any(|a| a == "--debug");
    let show = list(value("--show"));
    let only = list(value("--only"));

    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("Helm");
    webservices::init(base, debug, |m| eprintln!("{m}"));

    let mut cfgs = webservices::default_services();
    if !only.is_empty() {
        cfgs.retain(|c| only.contains(&c.id));
    }
    let (visible, hidden): (Vec<_>, Vec<_>) =
        cfgs.into_iter().partition(|c| show.iter().any(|s| s == "all" || *s == c.id));
    webservices::start(&visible, true);
    webservices::start(&hidden, false);

    let mut last = String::new();
    unsafe {
        SetTimer(None, 1, 1000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.hwnd.0.is_null() {
                let line: Vec<String> = webservices::status()
                    .iter()
                    .map(|s| format!("{}={}{}{}", s.id, s.unread, if s.meeting { "(meeting)" } else { "" }, if s.attention { "!" } else { "" }))
                    .collect();
                let line = line.join("  ");
                if line != last {
                    println!("{line}");
                    last = line;
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
