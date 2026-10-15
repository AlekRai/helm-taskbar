pub(crate) use std::cell::{Cell, RefCell};
pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::ffi::c_void;
pub(crate) use std::path::PathBuf;
pub(crate) use std::rc::Rc;
pub(crate) use std::time::Duration;

pub(crate) use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
pub(crate) use windows::Win32::Foundation::*;
pub(crate) use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_WINDOW_CORNER_PREFERENCE, DwmGetWindowAttribute, DwmSetWindowAttribute,
};
pub(crate) use windows::Win32::Graphics::Gdi::*;
pub(crate) use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
pub(crate) use windows::Win32::System::SystemInformation::GetLocalTime;
pub(crate) use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
};
pub(crate) use windows::Win32::UI::HiDpi::*;
pub(crate) use windows::Win32::UI::Input::KeyboardAndMouse::*;
pub(crate) use windows::Win32::UI::Shell::*;
pub(crate) use windows::Win32::UI::WindowsAndMessaging::*;
pub(crate) use windows::core::{BOOL, PCWSTR, PWSTR, w};

pub(crate) use crate::actions::*;
pub(crate) use crate::icons::*;
pub(crate) use crate::menu::*;
pub(crate) use crate::config::*;
pub(crate) use crate::popup::*;
pub(crate) use crate::shell::*;
pub(crate) use crate::state::*;
pub(crate) use crate::tasks::*;
pub(crate) use crate::tray::*;
pub(crate) use crate::ui::{
    AppItem, BarWindow, MenuEntry, MenuPopup, MixItem, QItem, QuickPopup, StartItem, StartPopup, SvcItem, MeetRow, TipPopup,
    TrayFlyout, TrayItem,
};
pub(crate) use crate::util::*;
