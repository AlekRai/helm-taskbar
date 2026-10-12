use super::*;

// ---------- Bluetooth ----------

pub(super) fn bluetooth_radio() -> Option<Radio> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _ = Radio::RequestAccessAsync().ok()?.join();
    Radio::GetRadiosAsync().ok()?.join().ok()?.into_iter().find(|r| r.Kind() == Ok(RadioKind::Bluetooth))
}

pub(super) fn bluetooth_set(on: bool) {
    if let Some(r) = bluetooth_radio()
        && let Err(e) = r.SetStateAsync(if on { RadioState::On } else { RadioState::Off }).and_then(|a| a.join())
    {
        log(&format!("bluetooth: could not switch radio: {e}"));
    }
}


// Battery levels by Bluetooth address. Windows stores the level on the device's profile nodes
// (e.g. Hands-Free) under this property while the device is connected.
pub(super) fn bt_batteries() -> std::collections::HashMap<u64, u8> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::Win32::Devices::Properties::DEVPROPTYPE;
    use windows::Win32::Foundation::DEVPROPKEY;
    let mut map = std::collections::HashMap::new();
    let key = DEVPROPKEY { fmtid: GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5), pid: 2 };
    unsafe {
        let filter = to_wide("BTHENUM");
        let mut len = 0u32;
        if CM_Get_Device_ID_List_SizeW(&mut len, PCWSTR(filter.as_ptr()), CM_GETIDLIST_FILTER_ENUMERATOR) != CR_SUCCESS {
            return map;
        }
        let mut buf = vec![0u16; len as usize];
        if CM_Get_Device_ID_ListW(PCWSTR(filter.as_ptr()), &mut buf, CM_GETIDLIST_FILTER_ENUMERATOR) != CR_SUCCESS {
            return map;
        }
        for id in buf.split(|c| *c == 0).filter(|s| !s.is_empty()) {
            let id_str = String::from_utf16_lossy(id);
            let mut z = id.to_vec();
            z.push(0);
            let mut node = 0u32;
            if CM_Locate_DevNodeW(&mut node, PCWSTR(z.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) != CR_SUCCESS {
                continue;
            }
            let (mut ty, mut size, mut val) = (DEVPROPTYPE(0), 1u32, 0u8);
            if CM_Get_DevNode_PropertyW(node, &key, &mut ty, Some(&mut val), &mut size, 0) != CR_SUCCESS {
                continue;
            }
            // Instance ids end in "...&<12 hex digit address>_C00000000".
            let tail = id_str.rsplit('&').next().unwrap_or("");
            let hex = tail.split('_').next().unwrap_or("");
            if let Ok(addr) = u64::from_str_radix(hex, 16) {
                map.insert(addr, val);
            }
        }
    }
    map
}

pub(super) fn bt_devices() -> Vec<BtDev> {
    use windows::Devices::Bluetooth::BluetoothMajorClass;
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut out = Vec::new();
    let Ok(selector) = BluetoothDevice::GetDeviceSelectorFromPairingState(true) else { return out };
    let Ok(infos) = DeviceInformation::FindAllAsyncAqsFilter(&selector).and_then(|a| a.join()) else { return out };
    let batteries = bt_batteries();
    for info in infos {
        let Ok(id) = info.Id() else { continue };
        let Ok(dev) = BluetoothDevice::FromIdAsync(&id).and_then(|a| a.join()) else { continue };
        let addr = dev.BluetoothAddress().unwrap_or(0);
        let kind = match dev.ClassOfDevice().and_then(|c| c.MajorClass()) {
            Ok(BluetoothMajorClass::AudioVideo) => "Audio",
            Ok(BluetoothMajorClass::Peripheral) => "Input",
            Ok(BluetoothMajorClass::Phone) => "Phone",
            Ok(BluetoothMajorClass::Computer) => "Computer",
            Ok(BluetoothMajorClass::Wearable) => "Wearable",
            Ok(BluetoothMajorClass::Health) => "Health",
            Ok(BluetoothMajorClass::Imaging) => "Imaging",
            _ => "Other",
        };
        out.push(BtDev {
            addr,
            name: dev.Name().map(|n| n.to_string()).unwrap_or_default(),
            connected: dev.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected),
            kind,
            battery: batteries.get(&addr).copied(),
        });
    }
    out.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
    out
}

// Profiles toggled to connect/disconnect a device: A2DP sink, headset, hands-free, AVRCP, HID.
pub(super) const BT_SERVICES: [u32; 6] = [0x110B, 0x1108, 0x111E, 0x110C, 0x110E, 0x1124];

pub(super) fn bt_set_connected(addr: u64, connect: bool) {
    use windows::Win32::Devices::Bluetooth::*;
    unsafe {
        let mut info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
        info.Address.Anonymous.ullLong = addr;
        if BluetoothGetDeviceInfo(None, &mut info) != 0 {
            return;
        }
        for s in BT_SERVICES {
            let guid = GUID::from_u128(((s as u128) << 96) | 0x0000_1000_8000_0080_5f9b_34fb);
            BluetoothSetServiceState(None, &info, &guid, if connect { BLUETOOTH_SERVICE_ENABLE } else { BLUETOOTH_SERVICE_DISABLE });
        }
    }
}

thread_local! {
    // Device whose connect/disconnect is in flight, and the connection state we are waiting for.
    pub(super) static BT_BUSY: Cell<Option<(u64, bool)>> = const { Cell::new(None) };
}

pub(super) fn show_bt(ui: &QuickPopup, devs: Vec<BtDev>) {
    let pending = BT_BUSY.with(|b| b.get());
    if pending.is_some_and(|(a, target)| devs.iter().any(|d| d.addr == a && d.connected == target)) {
        BT_BUSY.with(|b| b.set(None));
    }
    let first_connected = devs.iter().find(|d| d.connected).map(|d| match d.battery {
        Some(b) => format!("{} · {b}%", d.name),
        None => d.name.clone(),
    });
    ui.set_bt_device(first_connected.unwrap_or_default().into());
    let pending = BT_BUSY.with(|b| b.get());
    let mut rows: Vec<QItem> = devs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let busy = pending.is_some_and(|(a, _)| a == d.addr);
            let mut sub = String::from(if busy {
                if d.connected { "Disconnecting…" } else { "Connecting…" }
            } else if d.connected {
                "Connected — click to disconnect"
            } else {
                "Paired — click to connect"
            });
            sub.push_str(" · ");
            sub.push_str(d.kind);
            if let Some(b) = d.battery {
                sub.push_str(&format!(" · {b}%"));
            }
            QItem { label: d.name.clone().into(), sub: sub.into(), active: d.connected, busy, id: i as i32 }
        })
        .collect();
    let mut items: Vec<ListItem> = devs.into_iter().map(|d| ListItem::Bt(d.addr, d.connected)).collect();
    rows.push(QItem { label: "More Bluetooth settings".into(), sub: "".into(), active: false, busy: false, id: -1 });
    items.push(ListItem::BtSettings);
    show_items(ui, rows, items);
    // Connecting can change the default output device.
    refresh_audio(ui);
}

pub(super) fn load_bt(weak: Weak<QuickPopup>) {
    std::thread::spawn(move || push_bt(&weak));
}

pub(super) fn push_bt(weak: &Weak<QuickPopup>) {
    send_bt(weak, bt_devices());
}

pub(super) fn send_bt(weak: &Weak<QuickPopup>, devs: Vec<BtDev>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade()
            && ui.get_panel() == 2 {
                show_bt(&ui, devs);
                ui.set_loading(false);
            }
    });
}

// Runs the connect/disconnect and polls until Windows reports the new state.
pub(super) fn bt_toggle_worker(weak: Weak<QuickPopup>, addr: u64, target: bool) {
    bt_set_connected(addr, target);
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(1000));
        let devs = bt_devices();
        let done = devs.iter().any(|d| d.addr == addr && d.connected == target);
        send_bt(&weak, devs);
        if done {
            std::thread::sleep(Duration::from_millis(1500));
            push_bt(&weak);
            return;
        }
    }
    let _ = slint::invoke_from_event_loop(|| BT_BUSY.with(|b| b.set(None)));
    push_bt(&weak);
}
