use super::*;

// Sets up the bar's edge area (tray, language, clock) for this monitor and returns how much
// width it reserves on each side of the app buttons.
pub(super) fn apply_edges(ui: &BarWindow, monitor: isize, mon_cfg: &MonitorCfg) -> f32 {
    let primary = is_primary(monitor);
    let ucfg = load_ui();
    let time_w = CLOCK_RESERVE
        + if ucfg.clock_seconds { 22.0 } else { 0.0 }
        + if !load_theme().clock_24h { 22.0 } else { 0.0 };
    let clock_w = time_w.max(date_width(ucfg.clock_date) + 28.0);
    let lang_on = mon_cfg.lang_on(primary);
    let tray_on = mon_cfg.tray_on(primary);
    crate::tray::sync_bar_tray(ui, tray_on);
    let clock_reserve = (if mon_cfg.show_clock { clock_w } else { 8.0 }) + if lang_on { SYS_ICONS_WIDTH } else { 0.0 };
    let tray_w = if tray_on { clock_reserve + visible_count() as f32 * TRAY_BTN } else { clock_reserve };
    let reserve = tray_w + crate::services::sync_bar(ui, mon_cfg.services_on(primary));
    ui.set_show_clock(mon_cfg.show_clock);
    ui.set_side_left(mon_cfg.tray_left);
    ui.set_show_sys(lang_on);
    ui.set_clock_reserve(clock_reserve);
    ui.set_reserve(reserve);
    reserve
}

// (key, entry, closing, entered) for each model row.
type Row = (String, Entry, bool, bool);

fn is_present(r: &Row) -> bool {
    !r.2 && r.3
}

// Whether the button shows its title next to the icon.
fn is_labelled(e: &Entry, mon_cfg: &MonitorCfg, icon_only: &HashSet<String>) -> bool {
    mon_cfg.show_labels && !e.hwnd.0.is_null() && !icon_only.contains(&file_name(&e.exe))
}

type Slots = HashMap<String, (f32, f32)>;

// Present buttons share `avail` pixels, centered, in visual order, shrinking labelled ones first when
// they don't fit. Returns (x, width) per row key, each button's centre, and the end of the last slot.
fn button_slots(
    rows: &[Row],
    visual: &[String],
    avail: f32,
    mon_cfg: &MonitorCfg,
    icon_only: &HashSet<String>,
) -> (Slots, Vec<(String, f32)>, f32) {
    let want = |e: &Entry| if is_labelled(e, mon_cfg, icon_only) { label_button_width(&e.title) } else { BTN_COMPACT };
    let order: Vec<&Row> = visual
        .iter()
        .filter_map(|k| rows.iter().find(|r| &r.0 == k))
        .filter(|r| is_present(r))
        .collect();
    let gaps = BTN_GAP * order.len().saturating_sub(1) as f32;
    let total: f32 = order.iter().map(|r| want(&r.1)).sum::<f32>() + gaps;
    let shrinkable: f32 = order.iter().map(|r| want(&r.1) - BTN_COMPACT).sum();
    let ratio = if total > avail && shrinkable > 0.0 { ((total - avail) / shrinkable).min(1.0) } else { 0.0 };
    let width_of = |e: &Entry| want(e) - (want(e) - BTN_COMPACT) * ratio;
    let used: f32 = order.iter().map(|r| width_of(&r.1)).sum::<f32>() + gaps;
    let mut cursor = ((avail - used) / 2.0).max(0.0);
    let mut slots = Slots::new();
    let mut centers: Vec<(String, f32)> = Vec::new();
    for r in &order {
        let w = width_of(&r.1);
        slots.insert(r.0.clone(), (cursor, w));
        centers.push((r.0.clone(), cursor + w / 2.0));
        cursor += w + BTN_GAP;
    }
    (slots, centers, cursor)
}

// Copies the bar's entries into its Slint model, updating rows in place so Slint can animate them.
pub(crate) fn push_ui(monitor: isize) {
    let Some(info) = with_bar(monitor, |b| {
        let old_keys: Vec<String> = b.shown.iter().map(|s| s.key.clone()).collect();
        b.shown = merge_shown(&b.shown, &b.entries);
        let width = b.ui.window().size().width as f32 / b.ui.window().scale_factor();
        let rows: Vec<_> = b.shown.iter().map(|s| (s.key.clone(), s.entry.clone(), s.closing, s.entered)).collect();
        let mut vis: Vec<&Entry> = b.entries.iter().collect();
        if let Some(l) = &b.drag_order {
            vis.sort_by_key(|e| l.iter().position(|x| *x == entry_key(e)).unwrap_or(usize::MAX));
        }
        let visual: Vec<String> = vis.into_iter().map(entry_key).collect();
        (b.ui.clone_strong(), b.model.clone(), old_keys, rows, visual, b.foreground, width, b.drag.clone())
    }) else {
        return;
    };
    let (ui, model, old_keys, rows, visual, fg, width, drag) = info;
    let scale = ui.window().scale_factor();

    // Geometry: present buttons share the area between the clock reserves, centered, in
    // visual (entry) order; model rows keep their own order.
    let mon_cfg = monitor_cfg(&with_bar(monitor, |b| b.device.clone()).unwrap_or_default());
    let reserve = apply_edges(&ui, monitor, &mon_cfg);
    let avail = (width - 2.0 * reserve).max(0.0);
    let icon_only = STATE.with(|s| s.borrow().icon_only.clone());
    let wide = |e: &Entry| is_labelled(e, &mon_cfg, &icon_only);
    let (slots, centers, cursor) = button_slots(&rows, &visual, avail, &mon_cfg, &icon_only);
    with_bar(monitor, |b| b.slots = centers);

    let items: Vec<AppItem> = rows
        .iter()
        .map(|r| {
            let (mut x, w) = slots.get(r.0.as_str()).copied().unwrap_or((cursor, 0.0));
            if let Some((k, px)) = &drag
                && *k == r.0 {
                    x = *px;
                }
            let (overlay, has_overlay, progress_state, progress) = if r.1.hwnd.0.is_null() {
                (slint::Image::default(), false, 0, 0.0)
            } else {
                task_info(r.1.hwnd)
            };
            AppItem {
                overlay,
                has_overlay,
                progress_state,
                progress,
                title: r.1.title.as_str().into(),
                icon: cached_icon_image(r.1.icon, scale),
                running: !r.1.hwnd.0.is_null(),
                selected: !r.1.hwnd.0.is_null() && r.1.hwnd == fg && !r.2,
                x,
                w,
                present: is_present(r),
                label: wide(&r.1),
                flashing: !r.1.hwnd.0.is_null() && is_flashing(r.1.hwnd, fg),
                is_start: r.1.exe == START_EXE,
            }
        })
        .collect();

    // Rows only ever get removed (expired) or appended, in model order.
    let mut keys = old_keys;
    for j in (0..keys.len()).rev() {
        if !rows.iter().any(|r| r.0 == keys[j]) {
            model.remove(j);
            keys.remove(j);
        }
    }
    for (i, item) in items.into_iter().enumerate() {
        if i < keys.len() {
            model.set_row_data(i, item);
        } else {
            model.push(item);
        }
    }

    set_clock(&ui, &clock_parts());
    ui.set_lang(crate::quick::bar_lang().into());

    // New buttons appear collapsed first; flip them on the next tick. Closing ones get purged.
    let pending = rows.iter().any(|r| !r.3 || r.2);
    if pending {
        slint::Timer::single_shot(Duration::from_millis(30), move || {
            with_bar(monitor, |b| {
                for s in b.shown.iter_mut() {
                    s.entered = true;
                }
            });
            push_ui(monitor);
        });
        if rows.iter().any(|r| r.2) {
            slint::Timer::single_shot(Duration::from_millis(ANIM_MS + 60), move || push_ui(monitor));
        }
    }
}
