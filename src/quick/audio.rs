#![allow(non_snake_case)]
use super::*;

#[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
pub(super) unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, device: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, device: PCWSTR, a: *mut c_void, b: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, device: PCWSTR, default: i32, a: *mut i64, b: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, device: PCWSTR, fx: i32, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, device: PCWSTR, fx: i32, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, device: PCWSTR, role: i32) -> HRESULT;
    fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
}

pub(super) const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

// ---------- audio ----------

pub(super) fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

pub(super) fn enumerator() -> Option<IMMDeviceEnumerator> {
    init_com();
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() }
}

pub(super) fn default_device() -> Option<IMMDevice> {
    unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eMultimedia).ok() }
}

pub(super) fn endpoint_volume() -> Option<IAudioEndpointVolume> {
    unsafe { default_device()?.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok() }
}

pub(super) fn device_id(dev: &IMMDevice) -> Option<String> {
    unsafe {
        let id = dev.GetId().ok()?;
        let s = id.to_string().ok();
        CoTaskMemFree(Some(id.0 as *const c_void));
        s
    }
}

pub(super) fn device_name(dev: &IMMDevice) -> String {
    unsafe {
        dev.OpenPropertyStore(STGM_READ)
            .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
            .map(|v| v.to_string())
            .unwrap_or_default()
    }
}

pub(super) fn read_volume() -> (f32, bool) {
    let Some(ep) = endpoint_volume() else { return (0.0, false) };
    unsafe {
        (ep.GetMasterVolumeLevelScalar().unwrap_or(0.0), ep.GetMute().map(|m| m.as_bool()).unwrap_or(false))
    }
}

pub(super) fn set_volume(v: f32) {
    if let Some(ep) = endpoint_volume() {
        unsafe {
            let _ = ep.SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), std::ptr::null());
        }
    }
}

pub(super) fn set_mute(m: bool) {
    if let Some(ep) = endpoint_volume() {
        unsafe {
            let _ = ep.SetMute(m, std::ptr::null());
        }
    }
}

// (id, friendly name, is default)
pub(super) fn output_devices() -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let Some(en) = enumerator() else { return out };
    let default = default_device().and_then(|d| device_id(&d));
    unsafe {
        let Ok(coll) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
        for i in 0..coll.GetCount().unwrap_or(0) {
            let Ok(dev) = coll.Item(i) else { continue };
            let Some(id) = device_id(&dev) else { continue };
            let is_default = default.as_deref() == Some(id.as_str());
            out.push((id, device_name(&dev), is_default));
        }
    }
    out
}

pub(super) fn set_default_device(id: &str) {
    init_com();
    unsafe {
        let Ok(pc) = CoCreateInstance::<_, IPolicyConfig>(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) else { return };
        let w = to_wide(id);
        for role in 0..3 {
            let _ = pc.SetDefaultEndpoint(PCWSTR(w.as_ptr()), role).ok();
        }
    }
}


thread_local! {
    // Per-app volume handles behind the mixer rows (an app can own several sessions).
    pub(super) static MIX: RefCell<Vec<Vec<ISimpleAudioVolume>>> = const { RefCell::new(Vec::new()) };
    // Lowercase exe (or "system") per mixer row, for "Hide from mixer".
    pub(super) static MIX_KEYS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

// Mixer sessions as (key, item, handles); key is the lowercase exe path, or "system".
pub(super) fn mixer_sessions() -> Vec<(String, MixItem, Vec<ISimpleAudioVolume>)> {
    let cfg = load_ui();
    let mut out: Vec<(String, MixItem, Vec<ISimpleAudioVolume>)> = Vec::new();
    unsafe {
        let Some(en) = enumerator() else { return Vec::new() };
        let Ok(coll) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return Vec::new() };
        for d in 0..coll.GetCount().unwrap_or(0) {
        let Ok(dev) = coll.Item(d) else { continue };
        let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
        let Ok(en) = mgr.GetSessionEnumerator() else { continue };
        for i in 0..en.GetCount().unwrap_or(0) {
            let Ok(ctl) = en.GetSession(i) else { continue };
            if ctl.GetState().is_ok_and(|s| s == AudioSessionStateExpired) {
                continue;
            }
            let (Ok(c2), Ok(vol)) = (ctl.cast::<IAudioSessionControl2>(), ctl.cast::<ISimpleAudioVolume>()) else {
                continue;
            };
            let (key, name, exe) = if c2.IsSystemSoundsSession() == S_OK {
                ("system".to_string(), "System sounds".to_string(), r"C:\Windows\System32\SndVol.exe".to_string())
            } else {
                let pid = c2.GetProcessId().unwrap_or(0);
                let exe = exe_of(pid);
                if exe.is_empty() {
                    (format!("pid{pid}"), "Unknown app".to_string(), String::new())
                } else {
                    let stem = file_stem(&exe);
                    let mut chars = stem.chars();
                    let name =
                        chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
                    let key = std::path::Path::new(&exe)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_lowercase())
                        .unwrap_or_else(|| exe.to_lowercase());
                    (key, name, exe)
                }
            };
            if (key == "system" && cfg.mixer_hide_system) || cfg.mixer_hidden.contains(&key) {
                continue;
            }
            if let Some(e) = out.iter_mut().find(|e| e.0 == key) {
                e.2.push(vol);
                continue;
            }
            let item = MixItem {
                name: name.into(),
                icon: icon_image(exe_icon(&exe), 48).unwrap_or_default(),
                volume: vol.GetMasterVolume().unwrap_or(1.0),
                muted: vol.GetMute().map(|m| m.as_bool()).unwrap_or(false),
            };
            out.push((key, item, vec![vol]));
        }
        }
    }
    out.sort_by_key(|a| a.1.name.to_lowercase());
    out
}

pub(super) fn show_mixer(ui: &QuickPopup) {
    let mut rows = Vec::new();
    let mut handles = Vec::new();
    let mut keys = Vec::new();
    for (k, i, v) in mixer_sessions() {
        keys.push(k);
        rows.push(i);
        handles.push(v);
    }
    let n = rows.len();
    MIX.with(|m| *m.borrow_mut() = handles);
    MIX_KEYS.with(|m| *m.borrow_mut() = keys);
    ui.set_mix(ModelRc::new(VecModel::from(rows)));
    let panel_h = (n.max(1) as f32 * 52.0).min(208.0) + 16.0;
    ui.set_panel_h(panel_h);
    crate::popup::resize_quick(crate::popup::quick_main_height() + panel_h);
}

pub(super) fn mix_apply(ui: &QuickPopup, i: usize, volume: Option<f32>, muted: Option<bool>) {
    MIX.with(|m| {
        for vol in m.borrow().get(i).into_iter().flatten() {
            unsafe {
                if let Some(v) = volume {
                    let _ = vol.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null());
                }
                if let Some(mute) = muted {
                    let _ = vol.SetMute(mute, std::ptr::null());
                }
            }
        }
    });
    let model = ui.get_mix();
    if let Some(mut row) = model.row_data(i) {
        if let Some(v) = volume {
            row.volume = v;
            if row.muted && v > 0.0 {
                row.muted = false;
                mix_apply(ui, i, None, Some(false));
            }
        }
        if let Some(m) = muted {
            row.muted = m;
        }
        model.set_row_data(i, row);
    }
}
