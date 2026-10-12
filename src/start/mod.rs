use crate::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::core::Interface;

mod index;
mod window;
mod winkey;

use index::*;
pub(crate) use window::*;
pub(crate) use winkey::*;

const MAX_RESULTS: usize = 8;
const ICON_PX: i32 = 64;
const REINDEX_AFTER: Duration = Duration::from_secs(600);

struct App {
    name: String,
    name_lc: String,
    // Shell parsing name; `ShellExecute` opens it (desktop apps and UWP alike).
    id: String,
    item: IShellItem,
}

#[derive(Default)]
struct Index {
    apps: Vec<App>,
    icons: HashMap<String, slint::Image>,
    built: Option<Instant>,
}

thread_local! {
    static INDEX: RefCell<Index> = RefCell::new(Index::default());
}

fn take_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    s
}

