use super::*;

pub(super) fn populate_profiles(ui: &SettingsWindow) {
    ui.set_profiles(strings(list_profiles()));
    ui.set_active_profile(active_profile().unwrap_or_default().into());
}

pub(super) fn pick_file(owner: Option<HWND>, save: bool, default_name: &str) -> Option<PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::*;
    let mut buf = [0u16; 520];
    for (i, c) in default_name.encode_utf16().take(256).enumerate() {
        buf[i] = c;
    }
    let filter: Vec<u16> = "TOML files (*.toml)\0*.toml\0All files\0*.*\0\0".encode_utf16().collect();
    let ext: Vec<u16> = "toml\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner.unwrap_or_default(),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: if save { OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST },
        ..Default::default()
    };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } }.as_bool();
    ok.then(|| {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        PathBuf::from(String::from_utf16_lossy(&buf[..n]))
    })
}

thread_local! {
    pub(super) static PENDING_PROFILE: RefCell<Option<String>> = const { RefCell::new(None) };
}

// Applies a saved profile everywhere and refreshes the settings window.
pub(super) fn switch_profile(ui: &SettingsWindow, name: &str) {
    PENDING_PROFILE.with(|p| p.borrow_mut().take());
    ui.set_confirm_profile("".into());
    if load_profile(name) {
        changed();
        populate(ui);
        ui.set_profile_status(format!("Loaded \"{name}\".").into());
    } else {
        ui.set_profile_status(format!("Could not load \"{name}\".").into());
    }
}

pub(super) fn wire_profiles(ui: &SettingsWindow) {
    let w = ui.as_weak();
    ui.on_profile_save(move |name| {
        let Some(ui) = w.upgrade() else { return };
        let clean = clean_profile_name(&name);
        let msg = if clean.is_empty() {
            "Enter a profile name first.".to_string()
        } else if save_profile(&clean) {
            ui.set_profile_name("".into());
            format!("Saved \"{clean}\".")
        } else {
            "Could not save the profile.".to_string()
        };
        ui.set_profile_status(msg.into());
        populate_profiles(&ui);
    });

    let w = ui.as_weak();
    ui.on_profile_load(move |i| {
        let Some(ui) = w.upgrade() else { return };
        let Some(name) = list_profiles().get(i as usize).cloned() else { return };
        if active_profile().is_some() {
            switch_profile(&ui, &name);
        } else {
            PENDING_PROFILE.with(|p| *p.borrow_mut() = Some(name.clone()));
            ui.set_confirm_profile(name.into());
        }
    });

    let w = ui.as_weak();
    ui.on_profile_confirm(move || {
        let Some(ui) = w.upgrade() else { return };
        if let Some(name) = PENDING_PROFILE.with(|p| p.borrow_mut().take()) {
            switch_profile(&ui, &name);
        }
    });

    let w = ui.as_weak();
    ui.on_profile_cancel(move || {
        PENDING_PROFILE.with(|p| p.borrow_mut().take());
        if let Some(ui) = w.upgrade() {
            ui.set_confirm_profile("".into());
        }
    });

    let w = ui.as_weak();
    ui.on_profile_delete(move |i| {
        let Some(ui) = w.upgrade() else { return };
        if let Some(name) = list_profiles().get(i as usize) {
            delete_profile(name);
        }
        populate_profiles(&ui);
    });

    let w = ui.as_weak();
    ui.on_profile_export(move || {
        let Some(ui) = w.upgrade() else { return };
        let owner = crate::shell::winit_hwnd(ui.window());
        let msg = match pick_file(owner, true, "taskbar-settings.toml") {
            Some(path) if export_settings(&path) => format!("Exported to {}.", path.display()),
            Some(_) => "Could not write the file.".to_string(),
            None => return,
        };
        ui.set_profile_status(msg.into());
    });

    let w = ui.as_weak();
    ui.on_profile_import(move || {
        let Some(ui) = w.upgrade() else { return };
        let owner = crate::shell::winit_hwnd(ui.window());
        let Some(path) = pick_file(owner, false, "") else { return };
        let msg = match import_profile(&path) {
            Some(name) => format!("Imported as \"{name}\". Press Load to use it."),
            None => "That file is not a valid settings file.".to_string(),
        };
        ui.set_profile_status(msg.into());
        populate_profiles(&ui);
    });
}

