use super::*;

pub(crate) fn reset(ui: &QuickPopup) {
    let u = load_ui();
    ui.set_show_cal(u.show_calendar);
    ui.set_show_wifi(u.show_wifi);
    ui.set_show_bt(u.show_bluetooth);
    ui.set_show_mixer(u.show_mixer);
    MIX.with(|m| m.borrow_mut().clear());
    ui.set_mix(ModelRc::new(VecModel::<MixItem>::default()));
    ui.set_expand_h(0.0);
    ui.set_exp_panel(0);
    ui.set_panel(0);
    ui.set_loading(false);
    ui.set_items(ModelRc::new(VecModel::<QItem>::default()));
    refresh_audio(ui);
    refresh_status_later(ui.as_weak(), 0);
}


pub(crate) fn wire(ui: &QuickPopup) {
    ui.on_volume_changed({
        let weak = ui.as_weak();
        move |v| {
            set_volume(v);
            if let Some(ui) = weak.upgrade()
                && ui.get_muted() {
                    set_mute(false);
                    ui.set_muted(false);
                }
        }
    });
    ui.on_mute_clicked({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                let m = !ui.get_muted();
                set_mute(m);
                ui.set_muted(m);
            }
        }
    });
    ui.on_wifi_toggled({
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let on = !ui.get_wifi_on();
            ui.set_wifi_on(on);
            ui.set_wifi_sub(if on { "Connecting…" } else { "Off" }.into());
            let weak = weak.clone();
            std::thread::spawn(move || {
                wlan_set_radio(on);
                refresh_status_later(weak, 2500);
            });
        }
    });
    ui.on_bt_toggled({
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let on = !ui.get_bt_on();
            ui.set_bt_on(on);
            let weak = weak.clone();
            std::thread::spawn(move || {
                bluetooth_set(on);
                refresh_status_later(weak, 500);
            });
        }
    });
    wire_lists(ui);
    wire_mixer(ui);
    wire_items(ui);
}

// Opens the Wi-Fi / Bluetooth / device / mixer drop-down lists.
pub(super) fn wire_lists(ui: &QuickPopup) {
    ui.on_open_list({
        let weak = ui.as_weak();
        move |kind| {
            let Some(ui) = weak.upgrade() else { return };
            if ui.get_panel() == kind {
                ui.set_panel(0);
                ui.set_loading(false);
                crate::popup::resize_quick(crate::popup::quick_main_height());
                return;
            }
            ui.set_items(ModelRc::new(VecModel::<QItem>::default()));
            ui.set_panel(kind);
            match kind {
                1 => {
                    ui.set_loading(true);
                    show_items(&ui, Vec::new(), Vec::new());
                    load_networks(weak.clone(), true);
                }
                2 => {
                    ui.set_loading(true);
                    show_items(&ui, Vec::new(), Vec::new());
                    load_bt(weak.clone());
                }
                5 => show_mixer(&ui),
                _ => show_devices(&ui),
            }
        }
    });
}

pub(super) fn wire_mixer(ui: &QuickPopup) {
    ui.on_mix_volume({
        let weak = ui.as_weak();
        move |i, v| {
            if let Some(ui) = weak.upgrade() {
                mix_apply(&ui, i as usize, Some(v), None);
            }
        }
    });
    ui.on_mix_mute({
        let weak = ui.as_weak();
        move |i| {
            if let Some(ui) = weak.upgrade() {
                let muted = ui.get_mix().row_data(i as usize).is_some_and(|r| r.muted);
                mix_apply(&ui, i as usize, None, Some(!muted));
            }
        }
    });
    ui.on_mix_hide({
        let weak = ui.as_weak();
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let Some(key) = MIX_KEYS.with(|k| k.borrow().get(i as usize).cloned()) else { return };
            update_ui(|u| {
                if key == "system" {
                    u.mixer_hide_system = true;
                } else if !u.mixer_hidden.contains(&key) {
                    u.mixer_hidden.push(key);
                }
            });
            show_mixer(&ui);
        }
    });
}

// Clicks on list rows and the Ethernet tile.
pub(super) fn wire_items(ui: &QuickPopup) {
    ui.on_eth_clicked(|| {
        unsafe {
            ShellExecuteW(None, w!("open"), w!("ms-settings:network"), None, None, SW_SHOWNORMAL);
        }
        close_popups();
    });
    ui.on_item_clicked({
        let weak = ui.as_weak();
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let picked = ITEMS.with(|v| {
                v.borrow().get(i as usize).map(|it| match it {
                    ListItem::Net(n) => ListItem::Net(n.clone()),
                    ListItem::Dev(d) => ListItem::Dev(d.clone()),
                    ListItem::Bt(a, c) => ListItem::Bt(*a, *c),
                    ListItem::BtSettings => ListItem::BtSettings,
                })
            });
            match picked {
                Some(ListItem::Bt(addr, connected)) => {
                    if BT_BUSY.with(|b| b.get()).is_some() {
                        return;
                    }
                    BT_BUSY.with(|b| b.set(Some((addr, !connected))));
                    let model = ui.get_items();
                    if let Some(mut row) = model.row_data(i as usize) {
                        row.busy = true;
                        row.sub = if connected { "Disconnecting…" } else { "Connecting…" }.into();
                        model.set_row_data(i as usize, row);
                    }
                    let weak = weak.clone();
                    std::thread::spawn(move || bt_toggle_worker(weak, addr, !connected));
                }
                Some(ListItem::BtSettings) => {
                    unsafe {
                        ShellExecuteW(None, w!("open"), w!("ms-settings:bluetooth"), None, None, SW_SHOWNORMAL);
                    }
                    close_popups();
                }
                Some(ListItem::Net(n)) => {
                    if n.connected {
                        std::thread::spawn(wlan_disconnect);
                    } else if !n.profile.is_empty() {
                        std::thread::spawn(move || wlan_connect(&n.profile));
                    } else {
                        unsafe {
                            ShellExecuteW(None, w!("open"), w!("ms-availablenetworks:"), None, None, SW_SHOWNORMAL);
                        }
                        close_popups();
                        return;
                    }
                    ui.set_loading(true);
                    let weak = weak.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(2500));
                        load_networks(weak.clone(), false);
                        refresh_status_later(weak, 0);
                    });
                }
                Some(ListItem::Dev(id)) => {
                    set_default_device(&id);
                    show_devices(&ui);
                    refresh_audio(&ui);
                }
                None => {}
            }
        }
    });
}
