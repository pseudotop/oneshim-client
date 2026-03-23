//! Dynamic tray icon generation with status dot overlay.
//!
//! Takes the base template icon and overlays a colored status dot
//! at the bottom-right corner to indicate capture state:
//! - Green dot: actively capturing
//! - Amber dot: paused
//!
//! Uses the @2x (44×44) base icon for retina quality. The generated
//! icons use `icon_as_template(false)` so colors are preserved —
//! base shape is rendered in white for dark menu bar visibility.

// Use `::image` to disambiguate from `tauri::image`.
use ::image::{GenericImageView, Rgba, RgbaImage};

/// Base icon (44×44 @2x) embedded at compile time.
const BASE_2X: &[u8] = include_bytes!("../icons/tray_icon@2x.png");

// Status dot colors (tailwind palette)
const GREEN: [u8; 4] = [34, 197, 94, 255]; // green-500 — capturing
const AMBER: [u8; 4] = [245, 158, 11, 255]; // amber-500 — paused

/// Generate a tray icon with a colored status dot overlay.
///
/// Returns `(rgba_bytes, width, height)` suitable for
/// `tauri::image::Image::from_rgba()`.
pub fn status_icon(paused: bool) -> (Vec<u8>, u32, u32) {
    let base = image::load_from_memory(BASE_2X).expect("embedded tray icon must be valid PNG");
    let (w, h) = base.dimensions();
    let mut img = base.to_rgba8();

    // Invert opaque pixels: black template → white for non-template rendering.
    // Keeps the icon visible on dark menu bars (macOS dark mode).
    for pixel in img.pixels_mut() {
        if pixel[3] > 0 {
            pixel[0] = 255;
            pixel[1] = 255;
            pixel[2] = 255;
        }
    }

    // Draw status dot at bottom-right quadrant
    let dot_color = if paused { AMBER } else { GREEN };
    let radius: f64 = 6.5;
    let cx = w as f64 - radius - 2.0;
    let cy = h as f64 - radius - 2.0;
    draw_filled_circle_aa(&mut img, cx, cy, radius, Rgba(dot_color));

    (img.into_raw(), w, h)
}

/// Draw an anti-aliased filled circle onto an RGBA image.
fn draw_filled_circle_aa(img: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: Rgba<u8>) {
    let (w, h) = img.dimensions();
    let r_outer = radius + 1.0; // AA fringe radius

    let x_min = ((cx - r_outer).floor().max(0.0)) as u32;
    let x_max = ((cx + r_outer).ceil().min(w as f64 - 1.0)) as u32;
    let y_min = ((cy - r_outer).floor().max(0.0)) as u32;
    let y_max = ((cy + r_outer).ceil().min(h as f64 - 1.0)) as u32;

    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius - 0.5 {
                // Fully inside — overwrite
                img.put_pixel(x, y, color);
            } else if dist < radius + 0.5 {
                // AA fringe — blend
                let coverage = ((radius + 0.5 - dist).clamp(0.0, 1.0) * 255.0) as u8;
                let mut fg = color;
                fg[3] = ((color[3] as u16 * coverage as u16) / 255) as u8;
                img.put_pixel(x, y, alpha_blend(*img.get_pixel(x, y), fg));
            }
        }
    }
}

/// Alpha-composite foreground over background (standard Porter-Duff over).
fn alpha_blend(bg: Rgba<u8>, fg: Rgba<u8>) -> Rgba<u8> {
    let fa = fg[3] as f64 / 255.0;
    let ba = bg[3] as f64 / 255.0;
    let out_a = fa + ba * (1.0 - fa);

    if out_a < 0.001 {
        return Rgba([0, 0, 0, 0]);
    }

    Rgba([
        ((fg[0] as f64 * fa + bg[0] as f64 * ba * (1.0 - fa)) / out_a) as u8,
        ((fg[1] as f64 * fa + bg[1] as f64 * ba * (1.0 - fa)) / out_a) as u8,
        ((fg[2] as f64 * fa + bg[2] as f64 * ba * (1.0 - fa)) / out_a) as u8,
        (out_a * 255.0) as u8,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_icon_recording_has_correct_dimensions() {
        let (rgba, w, h) = status_icon(false);
        assert_eq!(w, 44);
        assert_eq!(h, 44);
        assert_eq!(rgba.len(), (44 * 44 * 4) as usize);
    }

    #[test]
    fn status_icon_paused_has_correct_dimensions() {
        let (rgba, w, h) = status_icon(true);
        assert_eq!(w, 44);
        assert_eq!(h, 44);
        assert_eq!(rgba.len(), (44 * 44 * 4) as usize);
    }

    #[test]
    fn status_dot_contains_expected_color() {
        // The dot is drawn at bottom-right — check that green pixels exist
        let (rgba, w, _h) = status_icon(false);
        // Check pixel near the center of the dot (cx ≈ 35.5, cy ≈ 35.5)
        let px = 36_u32;
        let py = 36_u32;
        let idx = ((py * w + px) * 4) as usize;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        // Green dot: high green channel
        assert!(g > 150, "expected green channel > 150, got {g}");
        assert!(r < 100, "expected red channel < 100, got {r}");
    }

    #[test]
    fn paused_dot_contains_amber_color() {
        let (rgba, w, _h) = status_icon(true);
        let px = 36_u32;
        let py = 36_u32;
        let idx = ((py * w + px) * 4) as usize;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        // Amber dot: high red + moderate green
        assert!(r > 200, "expected red channel > 200, got {r}");
        assert!(g > 100, "expected green channel > 100, got {g}");
    }

    #[test]
    fn alpha_blend_fully_opaque_fg_overwrites() {
        let bg = Rgba([100, 100, 100, 255]);
        let fg = Rgba([200, 50, 50, 255]);
        let result = alpha_blend(bg, fg);
        assert_eq!(result[0], 200);
        assert_eq!(result[1], 50);
    }

    #[test]
    fn alpha_blend_transparent_fg_keeps_bg() {
        let bg = Rgba([100, 100, 100, 255]);
        let fg = Rgba([200, 50, 50, 0]);
        let result = alpha_blend(bg, fg);
        assert_eq!(result[0], 100);
        assert_eq!(result[3], 255);
    }
}
