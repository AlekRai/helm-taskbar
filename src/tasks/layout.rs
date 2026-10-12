use super::*;

pub(super) const BTN_RUNNING: f32 = 200.0;
pub(super) const BTN_COMPACT: f32 = 48.0;

// Width of a labelled button: icon area plus the title, capped at BTN_RUNNING.
pub(super) fn label_button_width(title: &str) -> f32 {
    thread_local! {
        static CACHE: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
    }
    let text_w = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(w) = c.get(title) {
            return *w;
        }
        if c.len() > 512 {
            c.clear();
        }
        let w = unsafe { measure_text(title) };
        c.insert(title.to_string(), w);
        w
    });
    // Slint's font renders a bit wider than the GDI measurement, so leave some slack.
    (49.0 + text_w * 1.12 + 8.0).clamp(BTN_COMPACT, BTN_RUNNING)
}

// Width in logical pixels of `text` at 14px Segoe UI.
pub(crate) unsafe fn measure_text(text: &str) -> f32 {
    unsafe {
        let hdc = CreateCompatibleDC(None);
        let face = to_wide("Segoe UI");
        let font = CreateFontW(-14, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, PCWSTR(face.as_ptr()));
        let old = SelectObject(hdc, font.into());
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
        SelectObject(hdc, old);
        let _ = DeleteObject(font.into());
        let _ = DeleteDC(hdc);
        size.cx as f32
    }
}
pub(super) const BTN_GAP: f32 = 4.0;
pub(crate) const CLOCK_RESERVE: f32 = 64.0;
pub(super) const SYS_ICONS_WIDTH: f32 = 36.0;
pub(super) const ANIM_MS: u64 = 120;

pub(crate) fn entry_key(e: &Entry) -> String {
    if e.hwnd.0.is_null() {
        format!("p:{}", e.exe.to_lowercase())
    } else {
        format!("w:{}", e.hwnd.0 as isize)
    }
}

// Merges the current entries into the model rows. Row order never changes (so Slint keeps
// each button's instance and can slide it); buttons still animating out stay until expired.
pub(super) fn merge_shown(old: &[Shown], entries: &[Entry]) -> Vec<Shown> {
    let now = std::time::Instant::now();
    let first = old.is_empty();
    let mut out: Vec<Shown> = Vec::new();
    for s in old {
        if let Some(e) = entries.iter().find(|e| entry_key(e) == s.key) {
            out.push(Shown { key: s.key.clone(), entry: e.clone(), closing: false, entered: s.entered, since: s.since });
            continue;
        }
        let mut c = Shown { key: s.key.clone(), entry: s.entry.clone(), closing: true, entered: true, since: s.since };
        if !s.closing {
            c.since = now;
        } else if now.duration_since(s.since) > Duration::from_millis(ANIM_MS + 40) {
            continue;
        }
        out.push(c);
    }
    for e in entries {
        let key = entry_key(e);
        if !old.iter().any(|s| s.key == key) {
            out.push(Shown { key, entry: e.clone(), closing: false, entered: first, since: now });
        }
    }
    out
}

