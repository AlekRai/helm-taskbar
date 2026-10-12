use crate::prelude::*;

// Converts an HICON to a Slint image. Uses the color bitmap's own alpha if it has
// any, else the AND mask.
pub(crate) unsafe fn icon_image(icon: HICON, target: u32) -> Option<slint::Image> {
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let image = convert(&info, target);
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(info.hbmColor.into());
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(info.hbmMask.into());
        }
        image
    }
}

unsafe fn convert(info: &ICONINFO, target: u32) -> Option<slint::Image> {
    unsafe {
        if info.hbmColor.is_invalid() || info.hbmMask.is_invalid() {
            return None;
        }
        let mut bm = BITMAP::default();
        if GetObjectW(info.hbmColor.into(), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut c_void)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut color = bitmap_bgra(info.hbmColor, w, h);
        let mask = bitmap_bgra(info.hbmMask, w, h);

        let has_alpha = color.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w as u32, h as u32);
        for (i, out) in buf.make_mut_slice().iter_mut().enumerate() {
            let p = &mut color[i * 4..i * 4 + 4];
            let a = if has_alpha { p[3] } else if mask[i * 4] == 0 { 255 } else { 0 };
            *out = slint::Rgba8Pixel { r: p[2], g: p[1], b: p[0], a };
        }
        Some(slint::Image::from_rgba8(resample(&buf, target)))
    }
}


// Area-averaged resize (premultiplied alpha) so icons stay crisp at the size they are shown.
fn resample(src: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, size: u32) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    let (sw, sh) = (src.width(), src.height());
    if size == 0 || (sw == size && sh == size) {
        return src.clone();
    }
    let px = src.as_slice();
    let mut out = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(size, size);
    let (fx, fy) = (sw as f32 / size as f32, sh as f32 / size as f32);
    for (i, o) in out.make_mut_slice().iter_mut().enumerate() {
        let (dx, dy) = ((i as u32 % size) as f32, (i as u32 / size) as f32);
        let (x0, x1) = (dx * fx, (dx + 1.0) * fx);
        let (y0, y1) = (dy * fy, (dy + 1.0) * fy);
        let (mut r, mut g, mut b, mut a, mut wsum) = (0f32, 0f32, 0f32, 0f32, 0f32);
        // Upscaling samples at least one source pixel around the centre.
        let (x0, x1) = if x1 - x0 < 1.0 { let c = (x0 + x1) / 2.0; (c - 0.5, c + 0.5) } else { (x0, x1) };
        let (y0, y1) = if y1 - y0 < 1.0 { let c = (y0 + y1) / 2.0; (c - 0.5, c + 0.5) } else { (y0, y1) };
        let mut sy = y0.floor() as i32;
        while (sy as f32) < y1 {
            let wy = ((sy + 1) as f32).min(y1) - (sy as f32).max(y0);
            let mut sx = x0.floor() as i32;
            while (sx as f32) < x1 {
                let wx = ((sx + 1) as f32).min(x1) - (sx as f32).max(x0);
                let p = px[(sy.clamp(0, sh as i32 - 1) as u32 * sw + sx.clamp(0, sw as i32 - 1) as u32) as usize];
                let wgt = wx * wy;
                let pa = p.a as f32 / 255.0;
                r += p.r as f32 * pa * wgt;
                g += p.g as f32 * pa * wgt;
                b += p.b as f32 * pa * wgt;
                a += pa * wgt;
                wsum += wgt;
                sx += 1;
            }
            sy += 1;
        }
        if a > 0.0 && wsum > 0.0 {
            *o = slint::Rgba8Pixel {
                r: (r / a).round().clamp(0.0, 255.0) as u8,
                g: (g / a).round().clamp(0.0, 255.0) as u8,
                b: (b / a).round().clamp(0.0, 255.0) as u8,
                a: (a / wsum * 255.0).round().clamp(0.0, 255.0) as u8,
            };
        }
    }
    out
}

pub(crate) fn cached_icon_image(icon: HICON, scale: f32) -> slint::Image {
    if icon.0.is_null() {
        return slint::Image::default();
    }
    let px = (load_ui().icon_size.clamp(12, 32) as f32 * scale).round() as u32;
    let key = (icon.0 as isize, px);
    if let Some(img) = STATE.with(|s| s.borrow().icon_images.get(&key).cloned()) {
        return img.unwrap_or_default();
    }
    let img = unsafe { icon_image(icon, px) };
    STATE.with(|s| s.borrow_mut().icon_images.insert(key, img.clone()));
    img.unwrap_or_default()
}
