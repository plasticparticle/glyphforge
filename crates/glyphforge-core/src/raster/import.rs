//! Turning images into raster pixels: fit, resample, threshold alpha.
//!
//! Decoding file formats is the application's job; this module takes
//! RGBA8 data. Terminal cells are about twice as tall as wide, so each
//! raster mode has its own pixel shape (half-block pixels are square,
//! quadrant pixels tall, Braille and octant pixels square, sextant pixels
//! 3:4). The image keeps its proportions on screen.

use super::{Pixel, Raster, RasterMode};
use crate::document::{Color, Size};

/// An RGBA8 image, row-major, four bytes per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Raster pixels needed to show an `iw`×`ih` image as large as fits in
/// `cells`, keeping its aspect ratio (cells are 1:2).
pub fn fitted_size(iw: u32, ih: u32, cells: Size, mode: RasterMode) -> (u16, u16) {
    let (cw, ch) = mode.cell_px();
    // Physical units: one cell is 1 wide and 2 tall.
    let (box_w, box_h) = (f64::from(cells.width), f64::from(cells.height) * 2.0);
    let k = (box_w / f64::from(iw.max(1))).min(box_h / f64::from(ih.max(1)));
    let (phys_w, phys_h) = (f64::from(iw) * k, f64::from(ih) * k);
    let w = (phys_w * f64::from(cw)).round().max(1.0);
    let h = (phys_h * f64::from(ch) / 2.0).round().max(1.0);
    (
        w.min(f64::from(cells.width * cw)) as u16,
        h.min(f64::from(cells.height * ch)) as u16,
    )
}

/// Resamples `image` to `w`×`h` pixels (box filter, alpha-weighted).
/// Pixels less than half covered become transparent.
pub fn resample(image: &Rgba, w: u16, h: u16) -> Raster {
    let mut out = Raster::new(w, h);
    let (iw, ih) = (u64::from(image.width), u64::from(image.height));
    if iw == 0 || ih == 0 || image.data.len() < (iw * ih * 4) as usize {
        return out;
    }
    for y in 0..h {
        let y0 = u64::from(y) * ih / u64::from(h);
        let y1 = ((u64::from(y) + 1) * ih / u64::from(h)).max(y0 + 1);
        for x in 0..w {
            let x0 = u64::from(x) * iw / u64::from(w);
            let x1 = ((u64::from(x) + 1) * iw / u64::from(w)).max(x0 + 1);
            let (mut sum, mut alpha, mut count) = ([0u64; 3], 0u64, 0u64);
            for sy in y0..y1.min(ih) {
                for sx in x0..x1.min(iw) {
                    let at = ((sy * iw + sx) * 4) as usize;
                    let cover = u64::from(image.data[at + 3]);
                    for (k, total) in sum.iter_mut().enumerate() {
                        *total += u64::from(image.data[at + k]) * cover;
                    }
                    alpha += cover;
                    count += 1;
                }
            }
            if count == 0 || alpha * 2 < count * 255 {
                continue;
            }
            let channel = |k: usize| (sum[k] / alpha.max(1)).min(255) as u8;
            out.set(
                x,
                y,
                Pixel::Solid(Color::Rgb(channel(0), channel(1), channel(2))),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Rgba {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend(f(x, y));
            }
        }
        Rgba {
            width: w,
            height: h,
            data,
        }
    }

    #[test]
    fn square_images_stay_square_in_every_mode() {
        // 20×10 cells is a 20×20 physical box.
        let cells = Size::new(20, 10);
        assert_eq!(fitted_size(100, 100, cells, RasterMode::Half), (20, 20));
        assert_eq!(fitted_size(100, 100, cells, RasterMode::Braille), (40, 40));
        assert_eq!(fitted_size(100, 100, cells, RasterMode::Sextant), (40, 30));
        // A wide image is limited by the width.
        assert_eq!(fitted_size(200, 50, cells, RasterMode::Half), (20, 5));
    }

    #[test]
    fn resampling_averages_and_drops_transparent_areas() {
        // Left half red, right half transparent.
        let img = image(4, 2, |x, _| {
            if x < 2 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let r = resample(&img, 2, 1);
        assert_eq!(r.get(0, 0), Some(Pixel::Solid(Color::Rgb(255, 0, 0))));
        assert_eq!(r.get(1, 0), Some(Pixel::Clear));
        // Black and white average to grey.
        let img = image(2, 1, |x, _| {
            if x == 0 {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            }
        });
        assert_eq!(
            resample(&img, 1, 1).get(0, 0),
            Some(Pixel::Solid(Color::Rgb(127, 127, 127)))
        );
    }
}
