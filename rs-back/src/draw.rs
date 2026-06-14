use ab_glyph::FontRef;
use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

const GREEN: Rgb<u8> = Rgb([0, 255, 0]);
const BLUE: Rgb<u8> = Rgb([0, 0, 255]); // OpenCV BGR (255,0,0) == RGB blue

static FONT_BYTES: &[u8] = include_bytes!("../assets/font.ttf");

fn font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BYTES).expect("bundled font is valid")
}

/// Draw a 2px-thick rectangle from (x1,y1) to (x2,y2).
fn rect_2px(img: &mut RgbImage, x1: i32, y1: i32, x2: i32, y2: i32, color: Rgb<u8>) {
    let w = (x2 - x1).max(0) as u32;
    let h = (y2 - y1).max(0) as u32;
    for t in 0..2i32 {
        // inset each successive pass by one pixel to approximate 2px thickness
        let rx = x1 + t;
        let ry = y1 + t;
        let rw = w.saturating_sub((2 * t) as u32);
        let rh = h.saturating_sub((2 * t) as u32);
        if rw == 0 || rh == 0 {
            continue;
        }
        draw_hollow_rect_mut(img, Rect::at(rx, ry).of_size(rw, rh), color);
    }
}

/// Draw detection boxes (green) with score labels. boxes: [x1,y1,x2,y2,score].
pub fn draw_boxes(img: &mut RgbImage, boxes: &[[f32; 5]]) {
    let font = font();
    let scale = ab_glyph::PxScale::from(18.0);
    for b in boxes {
        let (x1, y1, x2, y2) = (b[0] as i32, b[1] as i32, b[2] as i32, b[3] as i32);
        rect_2px(img, x1, y1, x2, y2, GREEN);
        let label = format!("{:.2}", b[4]);
        draw_text_mut(img, GREEN, x1, y1, scale, &font, &label);
    }
}

/// Draw tiling windows (blue). windows_lt: top-left coords; size = (h, w).
pub fn draw_windows(img: &mut RgbImage, windows_lt: &[(i32, i32)], window_h: i32, window_w: i32) {
    for &(x, y) in windows_lt {
        rect_2px(img, x, y, x + window_w, y + window_h, BLUE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_without_panicking_and_changes_pixels() {
        let mut img = RgbImage::from_pixel(200, 200, Rgb([0, 0, 0]));
        draw_boxes(&mut img, &[[10.0, 10.0, 100.0, 100.0, 0.91]]);
        draw_windows(&mut img, &[(0, 0)], 50, 50);
        // a pixel on the left edge of the box rectangle (x=10, y=50) should be green
        let on_border = img.get_pixel(10, 50);
        assert_eq!(*on_border, GREEN);
    }
}
