//! Dynamic drop shadows: computed from the painted pixels at render time.

use serde::{Deserialize, Serialize};

use super::{Dither, Pixel, Raster, RasterMode, ShadeLevel, bayer};
use crate::document::Color;

/// A drop shadow behind everything painted on a raster layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DropShadow {
    /// Offset in pixels.
    pub dx: i16,
    pub dy: i16,
    #[serde(default = "black")]
    pub color: Color,
    /// Blur radius in pixels (0 = hard edge).
    #[serde(default)]
    pub softness: u8,
    /// 0..=100.
    #[serde(default = "full")]
    pub opacity: u8,
}

const fn black() -> Color {
    Color::BLACK
}

const fn full() -> u8 {
    100
}

impl Default for DropShadow {
    fn default() -> Self {
        Self {
            dx: 2,
            dy: 2,
            color: Color::BLACK,
            softness: 1,
            opacity: 70,
        }
    }
}

impl DropShadow {
    /// Shadow strength per pixel, 0..=1: the painted pixels shifted by the
    /// offset, box-blurred by the softness, scaled by the opacity.
    pub fn alpha(&self, raster: &Raster) -> Vec<f32> {
        let (w, h) = (raster.width(), raster.height());
        let (wu, hu) = (usize::from(w), usize::from(h));
        // Summed-area table of the shifted mask.
        let mut sat = vec![0u32; (wu + 1) * (hu + 1)];
        for y in 0..h {
            let mut row = 0u32;
            for x in 0..w {
                let source_x = i32::from(x) - i32::from(self.dx);
                let source_y = i32::from(y) - i32::from(self.dy);
                let on = u16::try_from(source_x)
                    .ok()
                    .zip(u16::try_from(source_y).ok())
                    .and_then(|(sx, sy)| raster.get(sx, sy))
                    .is_some_and(|p| !p.is_clear());
                row += u32::from(on);
                let (xu, yu) = (usize::from(x), usize::from(y));
                sat[(yu + 1) * (wu + 1) + xu + 1] = sat[yu * (wu + 1) + xu + 1] + row;
            }
        }
        let radius = usize::from(self.softness);
        let opacity = f32::from(self.opacity.min(100)) / 100.0;
        let mut out = vec![0.0; wu * hu];
        for yu in 0..hu {
            for xu in 0..wu {
                let (x0, y0) = (xu.saturating_sub(radius), yu.saturating_sub(radius));
                let (x1, y1) = ((xu + radius + 1).min(wu), (yu + radius + 1).min(hu));
                let sum = sat[y1 * (wu + 1) + x1] + sat[y0 * (wu + 1) + x0]
                    - sat[y0 * (wu + 1) + x1]
                    - sat[y1 * (wu + 1) + x0];
                let area = (2 * radius + 1) * (2 * radius + 1);
                out[yu * wu + xu] = sum as f32 / area as f32 * opacity;
            }
        }
        out
    }

    /// Shadow pixels in the style of `dither`: hard, ordered patterns, or
    /// shade glyphs (one level per cell so `░▒▓` render cleanly).
    pub fn render(&self, raster: &Raster, mode: RasterMode, dither: Dither) -> Raster {
        let alpha = self.alpha(raster);
        let (width, height) = (raster.width(), raster.height());
        let mut out = Raster::new(width, height);
        let solid = Pixel::Solid(self.color);
        let from_quarters = |q: u8| match ShadeLevel::from_quarters(q) {
            Some(level) => Pixel::Shade {
                fg: self.color,
                bg: None,
                level,
            },
            None if q >= 4 => solid,
            None => Pixel::Clear,
        };
        let (cw, ch) = mode.cell_px();
        for y in 0..height {
            for x in 0..width {
                let strength = alpha[usize::from(y) * usize::from(width) + usize::from(x)];
                let pixel = match dither {
                    Dither::None => {
                        if strength >= 0.5 {
                            solid
                        } else {
                            Pixel::Clear
                        }
                    }
                    // Diffusion needs colours; a shadow mask is
                    // thresholded with the ordered pattern instead.
                    Dither::Ordered | Dither::FloydSteinberg | Dither::Atkinson => {
                        if strength > bayer(x, y) {
                            solid
                        } else {
                            Pixel::Clear
                        }
                    }
                    Dither::Shade | Dither::OrderedShade => {
                        let (cx, cy) = (x / cw, y / ch);
                        let mut sum = 0.0;
                        let mut n = 0.0;
                        for py in cy * ch..((cy + 1) * ch).min(height) {
                            for px in cx * cw..((cx + 1) * cw).min(width) {
                                sum +=
                                    alpha[usize::from(py) * usize::from(width) + usize::from(px)];
                                n += 1.0;
                            }
                        }
                        let v = sum / n * 4.0;
                        let q = if dither == Dither::OrderedShade {
                            v.floor() + f32::from(u8::from(v.fract() > bayer(cx, cy)))
                        } else {
                            v.round()
                        };
                        from_quarters(q as u8)
                    }
                };
                out.set(x, y, pixel);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Raster {
        let mut r = Raster::new(8, 8);
        for y in 0..4 {
            for x in 0..4 {
                r.set(x, y, Pixel::Solid(Color::Indexed(1)));
            }
        }
        r
    }

    #[test]
    fn alpha_is_shifted_and_blurred() {
        let hard = DropShadow {
            dx: 2,
            dy: 2,
            color: Color::BLACK,
            softness: 0,
            opacity: 100,
        };
        let a = hard.alpha(&square());
        assert!((a[2 * 8 + 2] - 1.0).abs() < f32::EPSILON);
        assert!(a[0].abs() < f32::EPSILON);
        assert!(a[6 * 8 + 6].abs() < f32::EPSILON);
        let soft = DropShadow {
            softness: 1,
            opacity: 50,
            ..hard
        };
        let a = soft.alpha(&square());
        assert!(a[3 * 8 + 3] > 0.49 && a[3 * 8 + 3] <= 0.5);
        assert!(a[6 * 8 + 3] > 0.0 && a[6 * 8 + 3] < 0.5, "feathered edge");
    }

    #[test]
    fn dither_styles_differ() {
        let s = DropShadow {
            dx: 2,
            dy: 2,
            color: Color::BLACK,
            softness: 1,
            opacity: 60,
        };
        let raster = square();
        let hard = s.render(&raster, RasterMode::Quadrant, Dither::None);
        assert!(
            hard.pixels()
                .iter()
                .all(|p| matches!(p, Pixel::Clear | Pixel::Solid(_)))
        );
        let shade = s.render(&raster, RasterMode::Quadrant, Dither::Shade);
        assert!(
            shade
                .pixels()
                .iter()
                .any(|p| matches!(p, Pixel::Shade { .. }))
        );
        // Shade levels are uniform per cell.
        for cy in 0..4 {
            for cx in 0..4 {
                let first = shade.get(cx * 2, cy * 2);
                assert!(
                    (0..2).all(|dy| (0..2).all(|dx| shade.get(cx * 2 + dx, cy * 2 + dy) == first))
                );
            }
        }
        let ordered = s.render(&raster, RasterMode::Quadrant, Dither::Ordered);
        assert_ne!(ordered, hard);
    }
}
