use super::*;

pub(super) fn ethernet_up() -> bool {
    unsafe {
        // GAA_FLAG_SKIP_ANYCAST | SKIP_MULTICAST | SKIP_DNS_SERVER | INCLUDE_GATEWAYS
        let flags = windows::Win32::NetworkManagement::IpHelper::GET_ADAPTERS_ADDRESSES_FLAGS(0x2 | 0x4 | 0x8 | 0x80);
        let mut size = 0u32;
        GetAdaptersAddresses(0, flags, None, None, &mut size);
        if size == 0 {
            return false;
        }
        let mut buf = vec![0u8; size as usize];
        let head = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        if GetAdaptersAddresses(0, flags, None, Some(head), &mut size) != 0 {
            return false;
        }
        let mut p = head as *const IP_ADAPTER_ADDRESSES_LH;
        while !p.is_null() {
            let a = &*p;
            // IF_TYPE_ETHERNET_CSMACD = 6, IfOperStatusUp = 1
            if a.IfType == 6 && a.OperStatus.0 == 1 && !a.FirstGatewayAddress.is_null() {
                return true;
            }
            p = a.Next;
        }
        false
    }
}

// Two layouts toggle directly; more than two open a picker.
// ---------- Wi-Fi ----------

pub(super) struct Wlan(pub(super) HANDLE);

impl Drop for Wlan {
    fn drop(&mut self) {
        unsafe {
            WlanCloseHandle(self.0, None);
        }
    }
}

pub(super) fn wlan_open() -> Option<(Wlan, GUID)> {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return None;
        }
        let wl = Wlan(h);
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) != 0 || list.is_null() {
            return None;
        }
        let guid = ((*list).dwNumberOfItems > 0).then(|| (*list).InterfaceInfo[0].InterfaceGuid);
        WlanFreeMemory(list as *const c_void);
        guid.map(|g| (wl, g))
    }
}

pub(super) fn wlan_radio_on(wl: &Wlan, g: &GUID) -> bool {
    unsafe {
        let mut size = 0u32;
        let mut data: *mut c_void = std::ptr::null_mut();
        let r = WlanQueryInterface(wl.0, g, wlan_intf_opcode_radio_state, None, &mut size, &mut data, None);
        if r != 0 || data.is_null() {
            return true;
        }
        let st = &*(data as *const WLAN_RADIO_STATE);
        let on = (0..st.dwNumberOfPhys as usize).any(|i| {
            let p = &st.PhyRadioState[i];
            p.dot11SoftwareRadioState == dot11_radio_state_on && p.dot11HardwareRadioState == dot11_radio_state_on
        });
        WlanFreeMemory(data);
        on
    }
}

pub(super) fn wlan_set_radio(on: bool) {
    let Some((wl, g)) = wlan_open() else { return };
    unsafe {
        let mut size = 0u32;
        let mut data: *mut c_void = std::ptr::null_mut();
        let r = WlanQueryInterface(wl.0, &g, wlan_intf_opcode_radio_state, None, &mut size, &mut data, None);
        let phys = if r == 0 && !data.is_null() { (*(data as *const WLAN_RADIO_STATE)).dwNumberOfPhys } else { 1 };
        if !data.is_null() {
            WlanFreeMemory(data);
        }
        for i in 0..phys {
            let st = WLAN_PHY_RADIO_STATE {
                dwPhyIndex: i,
                dot11SoftwareRadioState: if on { dot11_radio_state_on } else { dot11_radio_state_off },
                dot11HardwareRadioState: dot11_radio_state_on,
            };
            let _ = WlanSetInterface(
                wl.0,
                &g,
                wlan_intf_opcode_radio_state,
                size_of::<WLAN_PHY_RADIO_STATE>() as u32,
                &st as *const _ as *const c_void,
                None,
            );
        }
    }
}

pub(super) fn wlan_networks(wl: &Wlan, g: &GUID) -> Vec<NetInfo> {
    let mut nets: Vec<NetInfo> = Vec::new();
    unsafe {
        let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
        if WlanGetAvailableNetworkList(wl.0, g, 0, None, &mut list) != 0 || list.is_null() {
            return nets;
        }
        let base = (*list).Network.as_ptr();
        for i in 0..(*list).dwNumberOfItems as usize {
            let n = &*base.add(i);
            let len = (n.dot11Ssid.uSSIDLength as usize).min(32);
            let ssid = String::from_utf8_lossy(&n.dot11Ssid.ucSSID[..len]).to_string();
            if ssid.is_empty() {
                continue;
            }
            let end = n.strProfileName.iter().position(|&c| c == 0).unwrap_or(n.strProfileName.len());
            let info = NetInfo {
                ssid,
                profile: String::from_utf16_lossy(&n.strProfileName[..end]),
                secure: n.bSecurityEnabled.as_bool(),
                connected: n.dwFlags & 1 != 0,
                signal: n.wlanSignalQuality,
            };
            match nets.iter_mut().find(|x| x.ssid == info.ssid) {
                Some(x) => {
                    if info.connected || (!x.connected && info.signal > x.signal) {
                        *x = info;
                    }
                }
                None => nets.push(info),
            }
        }
        WlanFreeMemory(list as *const c_void);
    }
    nets.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal.cmp(&a.signal)));
    nets
}

pub(super) fn wlan_connect(profile: &str) {
    let Some((wl, g)) = wlan_open() else { return };
    let name = to_wide(profile);
    let params = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_profile,
        strProfile: PCWSTR(name.as_ptr()),
        pDot11Ssid: std::ptr::null_mut(),
        pDesiredBssidList: std::ptr::null_mut(),
        dot11BssType: dot11_BSS_type_infrastructure,
        dwFlags: 0,
    };
    unsafe {
        let _ = WlanConnect(wl.0, &g, &params, None);
    }
}

pub(super) fn wlan_disconnect() {
    if let Some((wl, g)) = wlan_open() {
        unsafe {
            let _ = WlanDisconnect(wl.0, &g, None);
        }
    }
}

