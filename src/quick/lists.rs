use super::*;

pub(super) fn show_items(ui: &QuickPopup, rows: Vec<QItem>, items: Vec<ListItem>) {
    let n = rows.len();
    ITEMS.with(|i| *i.borrow_mut() = items);
    ui.set_items(ModelRc::new(VecModel::from(rows)));
    // 8px gap above + 8px inner padding around at most five visible rows.
    let panel_h = (n.max(1) as f32 * 44.0).min(220.0) + 16.0;
    ui.set_panel_h(panel_h);
    crate::popup::resize_quick(crate::popup::quick_main_height() + panel_h);
}

pub(super) fn show_networks(ui: &QuickPopup, nets: Vec<NetInfo>) {
    let rows = nets
        .iter()
        .enumerate()
        .map(|(i, n)| QItem {
            label: n.ssid.clone().into(),
            sub: if n.connected {
                "Connected — click to disconnect".into()
            } else if !n.profile.is_empty() {
                "Saved".into()
            } else if n.secure {
                "Secured — opens Windows network list".into()
            } else {
                "Open".into()
            },
            active: n.connected,
            busy: false,
            id: i as i32,
        })
        .collect();
    show_items(ui, rows, nets.into_iter().map(ListItem::Net).collect());
}

pub(super) fn load_networks(weak: Weak<QuickPopup>, scan: bool) {
    std::thread::spawn(move || {
        let Some((wl, g)) = wlan_open() else { return };
        let send = |nets: Vec<NetInfo>, loading: bool, weak: Weak<QuickPopup>| {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade()
                    && ui.get_panel() == 1 {
                        show_networks(&ui, nets);
                        ui.set_loading(loading);
                    }
            });
        };
        send(wlan_networks(&wl, &g), scan, weak.clone());
        if scan {
            unsafe {
                let _ = WlanScan(wl.0, &g, None, None, None);
            }
            std::thread::sleep(Duration::from_millis(2200));
            send(wlan_networks(&wl, &g), false, weak);
        }
    });
}

pub(super) fn show_devices(ui: &QuickPopup) {
    let devs = output_devices();
    let rows = devs
        .iter()
        .enumerate()
        .map(|(i, d)| QItem { label: d.1.clone().into(), sub: "".into(), active: d.2, busy: false, id: i as i32 })
        .collect();
    show_items(ui, rows, devs.into_iter().map(|d| ListItem::Dev(d.0)).collect());
}


