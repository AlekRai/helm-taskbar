#![allow(non_snake_case)]
use crate::prelude::*;
use slint::Weak;
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Globalization::GetLocaleInfoW;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eMultimedia, eRender,
};
use windows::Win32::Foundation::S_OK;
use windows::Win32::NetworkManagement::WiFi::*;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    STGM_READ,
};
use windows::core::{GUID, HRESULT};
use windows_core::{IUnknown, IUnknown_Vtbl, Interface};
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Win32::NetworkManagement::IpHelper::{GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH};


mod audio;
mod bluetooth;
mod lang;
mod wifi;
use audio::*;
use bluetooth::*;
pub(crate) use lang::*;
use wifi::*;
mod lists;
mod wiring;
use lists::*;
pub(crate) use wiring::*;


#[derive(Clone)]
struct NetInfo {
    ssid: String,
    profile: String,
    secure: bool,
    connected: bool,
    signal: u32,
}

enum ListItem {
    Net(NetInfo),
    Dev(String),
    Bt(u64, bool),
    BtSettings,
}

struct BtDev {
    addr: u64,
    name: String,
    connected: bool,
    kind: &'static str,
    battery: Option<u8>,
}

struct Status {
    wifi_supported: bool,
    wifi_on: bool,
    wifi_name: String,
    eth: bool,
    bt_supported: bool,
    bt_on: bool,
    bt_device: Option<String>,
}

thread_local! {
    static ITEMS: RefCell<Vec<ListItem>> = const { RefCell::new(Vec::new()) };
}

// ---------- status + UI ----------

fn status() -> Status {
    let mut s = Status {
        wifi_supported: false,
        wifi_on: false,
        wifi_name: String::new(),
        eth: ethernet_up(),
        bt_supported: false,
        bt_on: false,
        bt_device: None,
    };
    if let Some((wl, g)) = wlan_open() {
        s.wifi_supported = true;
        s.wifi_on = wlan_radio_on(&wl, &g);
        if s.wifi_on {
            s.wifi_name = wlan_networks(&wl, &g).into_iter().find(|n| n.connected).map(|n| n.ssid).unwrap_or_default();
        }
    }
    if let Some(r) = bluetooth_radio() {
        s.bt_supported = true;
        s.bt_on = r.State() == Ok(RadioState::On);
        if s.bt_on {
            s.bt_device = bt_devices().into_iter().find(|d| d.connected).map(|d| match d.battery {
                Some(b) => format!("{} · {b}%", d.name),
                None => d.name,
            });
        }
    }
    s
}

fn apply_status(ui: &QuickPopup, s: Status) {
    ui.set_wifi_supported(s.wifi_supported);
    ui.set_wifi_on(s.wifi_on);
    // Fall back to the wired connection when Wi-Fi isn't carrying the connection.
    ui.set_net_eth(s.eth && s.wifi_name.is_empty());
    ui.set_wifi_sub(
        if !s.wifi_on {
            "Off".to_string()
        } else if s.wifi_name.is_empty() {
            "Not connected".to_string()
        } else {
            s.wifi_name
        }
        .into(),
    );
    ui.set_bt_supported(s.bt_supported);
    ui.set_bt_on(s.bt_on);
    ui.set_bt_device(s.bt_device.unwrap_or_default().into());
}

fn refresh_status_later(weak: Weak<QuickPopup>, delay_ms: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));
        let s = status();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                apply_status(&ui, s);
            }
        });
    });
}

fn refresh_audio(ui: &QuickPopup) {
    let (v, m) = read_volume();
    ui.set_volume(v);
    ui.set_muted(m);
    ui.set_device_name(default_device().map(|d| device_name(&d)).unwrap_or_default().into());
}

