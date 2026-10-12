use super::*;

// Enumerates every app Windows lists in the Start menu (the AppsFolder).
pub(crate) fn build_index() {
    let mut apps: Vec<App> = Vec::new();
    unsafe {
        let Ok(folder) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else {
            return;
        };
        let Ok(list) = folder.BindToHandler::<_, IEnumShellItems>(None::<&IBindCtx>, &BHID_EnumItems) else {
            return;
        };
        loop {
            let mut batch: [Option<IShellItem>; 16] = Default::default();
            let mut got = 0u32;
            let _ = list.Next(&mut batch, Some(&mut got));
            if got == 0 {
                break;
            }
            for item in batch.into_iter().flatten() {
                let Ok(name) = item.GetDisplayName(SIGDN_NORMALDISPLAY).map(take_string) else { continue };
                let Ok(id) = item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING).map(take_string) else { continue };
                if name.is_empty() || id.is_empty() {
                    continue;
                }
                apps.push(App { name_lc: name.to_lowercase(), name, id, item });
            }
        }
    }
    apps.sort_by(|a, b| a.name_lc.cmp(&b.name_lc));
    apps.dedup_by(|a, b| a.id == b.id);
    INDEX.with(|i| {
        let mut i = i.borrow_mut();
        i.apps = apps;
        i.built = Some(Instant::now());
    });
}

pub(super) fn ensure_index() {
    let stale = INDEX.with(|i| i.borrow().built.is_none_or(|t| t.elapsed() > REINDEX_AFTER));
    if stale {
        build_index();
    }
}

// Loads the app's icon from the shell as straight-alpha RGBA. Some packaged apps return a blank
// tile for icon-only requests, so retry with a plain thumbnail request.
pub(super) unsafe fn shell_image(item: &IShellItem) -> Option<slint::Image> {
    unsafe {
        shell_image_with(item, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .or_else(|| shell_image_with(item, SIIGBF_BIGGERSIZEOK))
    }
}

pub(super) unsafe fn shell_image_with(item: &IShellItem, flags: SIIGBF) -> Option<slint::Image> {
    unsafe {
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let bmp = factory.GetImage(SIZE { cx: ICON_PX, cy: ICON_PX }, flags).ok()?;
        let mut bm = BITMAP::default();
        if GetObjectW(bmp.into(), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut c_void)) == 0 {
            let _ = DeleteObject(bmp.into());
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let px = bitmap_bgra(bmp, w, h);
        let _ = DeleteObject(bmp.into());
        if px.iter().all(|&b| b == 0) {
            return None;
        }
        let has_alpha = px.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w as u32, h as u32);
        for (out, p) in buf.make_mut_slice().iter_mut().zip(px.as_chunks::<4>().0) {
            let a = if has_alpha { p[3] } else { 255 };
            // The shell hands back premultiplied alpha.
            let un = |c: u8| if a == 0 || a == 255 { c } else { ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
            *out = slint::Rgba8Pixel { r: un(p[2]), g: un(p[1]), b: un(p[0]), a };
        }
        Some(slint::Image::from_rgba8(buf))
    }
}

pub(super) fn icon_of(index: &mut Index, i: usize) -> slint::Image {
    let id = index.apps[i].id.clone();
    if let Some(img) = index.icons.get(&id) {
        return img.clone();
    }
    let img = unsafe { shell_image(&index.apps[i].item) }.unwrap_or_default();
    index.icons.insert(id, img.clone());
    img
}

pub(super) fn item_for(index: &mut Index, i: usize, pins: &[String]) -> StartItem {
    StartItem {
        name: index.apps[i].name.as_str().into(),
        icon: icon_of(index, i),
        id: index.apps[i].id.as_str().into(),
        pinned: pins.contains(&index.apps[i].id),
    }
}

// Higher is better; None means no match.
pub(super) fn score(name: &str, q: &str) -> Option<i32> {
    if name.starts_with(q) {
        return Some(1000 - name.len() as i32);
    }
    if name.contains(&format!(" {q}")) {
        return Some(800 - name.len() as i32);
    }
    if let Some(pos) = name.find(q) {
        return Some(600 - pos as i32);
    }
    let mut it = name.chars();
    let mut gaps = 0;
    for c in q.chars() {
        let mut skipped = 0;
        loop {
            match it.next() {
                Some(x) if x == c => break,
                Some(_) => skipped += 1,
                None => return None,
            }
        }
        gaps += skipped;
    }
    Some(300 - gaps)
}

