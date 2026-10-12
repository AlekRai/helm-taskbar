use crate::prelude::*;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

mod badges;
mod clicks;
mod host;
mod model;
mod notify;

pub(crate) use badges::*;
pub(crate) use clicks::*;
pub(crate) use host::*;
pub(crate) use model::*;
pub(crate) use notify::*;

// System tray host. We register our own "Shell_TrayWnd" window above Explorer's, so apps that call
// Shell_NotifyIcon send their icons to us; everything we don't handle is forwarded to Explorer's tray.

const SIGNATURE: u32 = 0x3475_3423;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const NIF_MESSAGE: u32 = 0x1;
const NIF_ICON: u32 = 0x2;
const NIF_TIP: u32 = 0x4;
const NIF_STATE: u32 = 0x8;
const NIF_GUID: u32 = 0x20;
const NIN_SELECT: u32 = 0x400;

pub(crate) const TRAY_BTN: f32 = 28.0;

struct TrayIcon {
    hwnd: isize,
    uid: u32,
    guid: [u8; 16],
    callback: u32,
    version: u32,
    hidden: bool,
    tip: String,
    image: slint::Image,
    // Lowercase owner exe name; keys the overflow setting.
    exe: String,
}

thread_local! {
    // Kept apart from STATE: the window proc can run while STATE is borrowed (nested sent messages).
    static TRAY: RefCell<Vec<TrayIcon>> = const { RefCell::new(Vec::new()) };
    static TRAY_HWND: Cell<isize> = const { Cell::new(0) };
    static CLICKED: Cell<(u32, std::time::Instant)> = Cell::new((0, std::time::Instant::now()));
    static SHELLHOOK_MSG: Cell<u32> = const { Cell::new(0) };
    static FLASHING: RefCell<std::collections::HashSet<isize>> = RefCell::new(std::collections::HashSet::new());
}

