//! Raster layers: sub-cell pixel art.
//!
//! A raster layer stores real pixels at a resolution chosen by its encoding
//! mode (half blocks, quadrants, sextants or Braille) together with render
//! settings (palette limit, dithering, drop shadow). The pixels are the
//! document; cells are derived by [`encode`] at render time against what
//! lies beneath the layer, so the mode, the dithering and the shadow can be
//! changed at any time without repainting.

pub mod draw;
mod encode;
pub mod gradient;
pub mod import;
pub mod octant;
mod repr;
mod shadow;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub use encode::{encode, shade_glyph};
pub use gradient::{ColorSpace, Gradient, GradientShape, GradientStop, GradientStyle};
pub use shadow::DropShadow;

use crate::document::{Color, Size};

/// Coverage of a shade glyph: `░` (light), `▒` (medium), `▓` (dark).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShadeLevel {
    Light,
    Medium,
    Dark,
}

impl ShadeLevel {
    pub const ALL: [Self; 3] = [Self::Light, Self::Medium, Self::Dark];

    /// Share of the foreground colour, in quarters (1, 2 or 3).
    pub const fn quarters(self) -> u8 {
        match self {
            Self::Light => 1,
            Self::Medium => 2,
            Self::Dark => 3,
        }
    }

    pub const fn from_quarters(q: u8) -> Option<Self> {
        match q {
            1 => Some(Self::Light),
            2 => Some(Self::Medium),
            3 => Some(Self::Dark),
            _ => None,
        }
    }
}

/// One sub-cell pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Pixel {
    /// Transparent: whatever lies beneath shows through.
    #[default]
    Clear,
    Solid(Color),
    /// A shade-glyph mix of `fg` over `bg`; `bg: None` mixes over whatever
    /// lies beneath (soft shadows). A cell of identical shade pixels renders
    /// as `░▒▓`; mixed cells blend the colours.
    Shade {
        fg: Color,
        bg: Option<Color>,
        level: ShadeLevel,
    },
}

impl Pixel {
    pub const fn is_clear(self) -> bool {
        matches!(self, Self::Clear)
    }
}

impl fmt::Display for Pixel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clear => f.write_str("clear"),
            Self::Solid(c) => write!(f, "{c}"),
            Self::Shade { fg, bg, level } => {
                write!(f, "shade:{}:{fg}", level.quarters())?;
                if let Some(bg) = bg {
                    write!(f, ":{bg}")?;
                }
                Ok(())
            }
        }
    }
}

/// Errors from the textual pixel form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid pixel {0:?}; expected \"clear\", a colour or \"shade:LEVEL:FG[:BG]\"")]
pub struct PixelParseError(String);

impl FromStr for Pixel {
    type Err = PixelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || PixelParseError(s.to_owned());
        if s == "clear" {
            return Ok(Self::Clear);
        }
        if let Some(rest) = s.strip_prefix("shade:") {
            let mut parts = rest.split(':');
            let level = parts
                .next()
                .and_then(|q| q.parse::<u8>().ok())
                .and_then(ShadeLevel::from_quarters)
                .ok_or_else(err)?;
            // Colours may contain a colon themselves ("ansi:4").
            let colours: Vec<&str> = parts.collect();
            let (fg, bg) = split_colours(&colours).ok_or_else(err)?;
            return Ok(Self::Shade { fg, bg, level });
        }
        s.parse().map(Self::Solid).map_err(|_| err())
    }
}

/// Splits `["ansi", "4", "#ffffff"]` into colours, rejoining `ansi:N`.
fn split_colours(parts: &[&str]) -> Option<(Color, Option<Color>)> {
    let mut colours = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i] == "ansi" {
            colours.push(format!("ansi:{}", parts.get(i + 1)?));
            i += 2;
        } else {
            colours.push(parts[i].to_owned());
            i += 1;
        }
    }
    let fg = colours.first()?.parse().ok()?;
    let bg = match colours.get(1) {
        Some(c) => Some(c.parse().ok()?),
        None => None,
    };
    (colours.len() <= 2).then_some((fg, bg))
}

impl Serialize for Pixel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Pixel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// How pixels map onto cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RasterMode {
    /// 1×2 pixels per cell with `▀ ▄ █`: two full colours per cell.
    Half,
    /// 2×2 pixels per cell with quadrant blocks.
    Quadrant,
    /// 2×3 pixels per cell with sextant blocks (Unicode 13).
    #[default]
    Sextant,
    /// 2×4 pixels per cell with Braille dots: one colour per cell.
    Braille,
    /// 2×4 pixels per cell with octant blocks (Unicode 16): two full
    /// colours per cell at Braille resolution, where fonts have them.
    Octant,
}

impl RasterMode {
    pub const ALL: [Self; 5] = [
        Self::Half,
        Self::Quadrant,
        Self::Sextant,
        Self::Braille,
        Self::Octant,
    ];

    /// Pixels per cell, horizontally and vertically.
    pub const fn cell_px(self) -> (u16, u16) {
        match self {
            Self::Half => (1, 2),
            Self::Quadrant => (2, 2),
            Self::Sextant => (2, 3),
            Self::Braille | Self::Octant => (2, 4),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Half => "half",
            Self::Quadrant => "quadrant",
            Self::Sextant => "sextant",
            Self::Braille => "braille",
            Self::Octant => "octant",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s)
    }

    /// A glyph that shows what the mode looks like.
    pub const fn sample(self) -> &'static str {
        match self {
            Self::Half => "▀",
            Self::Quadrant => "▚",
            Self::Sextant => "🬗",
            Self::Braille => "⣿",
            Self::Octant => "\u{1CD3A}",
        }
    }
}

/// How colours that the output cannot show directly are approximated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dither {
    /// Nearest colour, hard edges.
    #[default]
    None,
    /// Bayer patterns across the sub-cell pixels.
    Ordered,
    /// Shade glyphs (`░▒▓`) mixing two colours per cell.
    Shade,
    /// Shade glyphs, with ordered patterns choosing between shade levels.
    OrderedShade,
    /// Error diffusion (Floyd–Steinberg): smooth gradients, organic grain.
    FloydSteinberg,
    /// Error diffusion (Atkinson): spreads 3/4 of the error, so highlights
    /// and shadows stay crisp.
    Atkinson,
}

impl Dither {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Ordered,
        Self::Shade,
        Self::OrderedShade,
        Self::FloydSteinberg,
        Self::Atkinson,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ordered => "ordered",
            Self::Shade => "shade",
            Self::OrderedShade => "ordered-shade",
            Self::FloydSteinberg => "floyd-steinberg",
            Self::Atkinson => "atkinson",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == s)
    }

    pub const fn ordered(self) -> bool {
        matches!(self, Self::Ordered | Self::OrderedShade)
    }

    pub const fn shade(self) -> bool {
        matches!(self, Self::Shade | Self::OrderedShade)
    }

    /// Whether the error of each reduced pixel spreads to its neighbours.
    pub const fn diffuses(self) -> bool {
        matches!(self, Self::FloydSteinberg | Self::Atkinson)
    }
}

/// The colours a raster may use in its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteLimit {
    /// Any colour.
    #[default]
    Truecolor,
    /// The xterm 256-colour palette.
    Ansi256,
    /// The 16 ANSI colours.
    Ansi16,
}

impl PaletteLimit {
    pub const ALL: [Self; 3] = [Self::Truecolor, Self::Ansi256, Self::Ansi16];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Truecolor => "truecolor",
            Self::Ansi256 => "ansi256",
            Self::Ansi16 => "ansi16",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }
}

/// The 4×4 Bayer threshold at a pixel, in `(0, 1)`.
pub fn bayer(x: u16, y: u16) -> f32 {
    const M: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    (f32::from(M[usize::from(y % 4)][usize::from(x % 4)]) + 0.5) / 16.0
}

/// A grid of pixels.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Raster {
    width: u16,
    height: u16,
    pixels: Vec<Pixel>,
}

impl Raster {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            pixels: vec![Pixel::Clear; usize::from(width) * usize::from(height)],
        }
    }

    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.width && y < self.height)
            .then(|| usize::from(y) * usize::from(self.width) + usize::from(x))
    }

    pub fn get(&self, x: u16, y: u16) -> Option<Pixel> {
        self.index(x, y).map(|i| self.pixels[i])
    }

    /// Sets a pixel; returns the previous value, `None` when out of range.
    pub fn set(&mut self, x: u16, y: u16, pixel: Pixel) -> Option<Pixel> {
        let i = self.index(x, y)?;
        Some(std::mem::replace(&mut self.pixels[i], pixel))
    }

    pub fn pixels(&self) -> &[Pixel] {
        &self.pixels
    }

    pub fn painted(&self) -> usize {
        self.pixels.iter().filter(|p| !p.is_clear()).count()
    }

    /// A copy with another size; the overlapping part is kept.
    #[must_use]
    pub fn resized(&self, width: u16, height: u16) -> Self {
        let mut out = Self::new(width, height);
        for y in 0..height.min(self.height) {
            for x in 0..width.min(self.width) {
                out.set(x, y, self.get(x, y).unwrap_or_default());
            }
        }
        out
    }
}

/// The content of a raster layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterLayer {
    pub mode: RasterMode,
    pub dither: Dither,
    pub palette: PaletteLimit,
    pub shadow: Option<DropShadow>,
    pub pixels: Raster,
}

impl RasterLayer {
    /// An empty raster covering `cells`.
    pub fn new(cells: Size, mode: RasterMode) -> Self {
        let (cw, ch) = mode.cell_px();
        Self {
            mode,
            dither: Dither::None,
            palette: PaletteLimit::Truecolor,
            shadow: None,
            pixels: Raster::new(
                cells.width.saturating_mul(cw),
                cells.height.saturating_mul(ch),
            ),
        }
    }

    /// The size in cells.
    pub const fn cells(&self) -> Size {
        let (cw, ch) = self.mode.cell_px();
        Size::new(self.pixels.width / cw, self.pixels.height / ch)
    }

    /// Resizes to cover `cells`, keeping the overlapping pixels.
    pub fn resize_cells(&mut self, cells: Size) {
        let (cw, ch) = self.mode.cell_px();
        self.pixels = self.pixels.resized(
            cells.width.saturating_mul(cw),
            cells.height.saturating_mul(ch),
        );
    }

    /// Switches the encoding mode, resampling the pixels (nearest
    /// neighbour per cell). Switching back is not lossless when the new
    /// mode has fewer pixels per cell.
    pub fn set_mode(&mut self, mode: RasterMode) {
        if mode == self.mode {
            return;
        }
        let cells = self.cells();
        let (ow, oh) = self.mode.cell_px();
        let (nw, nh) = mode.cell_px();
        let mut out = Raster::new(cells.width * nw, cells.height * nh);
        for y in 0..out.height {
            for x in 0..out.width {
                let (cx, cy) = (x / nw, y / nh);
                // Sample at the centre of the new pixel.
                let (sx, sy) = (
                    ((x % nw) * 2 + 1) * ow / (2 * nw),
                    ((y % nh) * 2 + 1) * oh / (2 * nh),
                );
                out.set(
                    x,
                    y,
                    self.pixels
                        .get(cx * ow + sx, cy * oh + sy)
                        .unwrap_or_default(),
                );
            }
        }
        self.pixels = out;
        self.mode = mode;
    }
}

impl Serialize for RasterLayer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        repr::RasterRepr::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RasterLayer {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let r = repr::RasterRepr::deserialize(deserializer)?;
        Self::try_from(r).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_text_round_trips() {
        let pixels = [
            Pixel::Clear,
            Pixel::Solid(Color::Rgb(1, 2, 3)),
            Pixel::Solid(Color::Indexed(4)),
            Pixel::Shade {
                fg: Color::Indexed(0),
                bg: None,
                level: ShadeLevel::Medium,
            },
            Pixel::Shade {
                fg: Color::Indexed(1),
                bg: Some(Color::Rgb(9, 9, 9)),
                level: ShadeLevel::Dark,
            },
        ];
        for p in pixels {
            assert_eq!(p.to_string().parse::<Pixel>().unwrap(), p, "{p}");
        }
        assert_eq!(pixels[3].to_string(), "shade:2:ansi:0");
        assert!("shade:4:#000000".parse::<Pixel>().is_err());
        assert!("mauve".parse::<Pixel>().is_err());
    }

    #[test]
    fn mode_switch_resamples_per_cell() {
        let mut layer = RasterLayer::new(Size::new(2, 1), RasterMode::Half);
        assert_eq!((layer.pixels.width(), layer.pixels.height()), (2, 2));
        let red = Pixel::Solid(Color::Indexed(1));
        layer.pixels.set(1, 1, red);
        layer.set_mode(RasterMode::Sextant);
        assert_eq!((layer.pixels.width(), layer.pixels.height()), (4, 3));
        assert_eq!(layer.cells(), Size::new(2, 1));
        // The lower half of cell 1 is red: rows 1 and 2 of the sextant cell.
        assert_eq!(layer.pixels.get(2, 2), Some(red));
        assert_eq!(layer.pixels.get(3, 1), Some(red));
        assert_eq!(layer.pixels.get(2, 0), Some(Pixel::Clear));
        assert_eq!(layer.pixels.get(0, 2), Some(Pixel::Clear));
        layer.resize_cells(Size::new(3, 2));
        assert_eq!((layer.pixels.width(), layer.pixels.height()), (6, 6));
        assert_eq!(layer.pixels.get(2, 2), Some(red));
    }

    #[test]
    fn bayer_thresholds_are_spread() {
        let mut all: Vec<f32> = (0..4)
            .flat_map(|y| (0..4).map(move |x| bayer(x, y)))
            .collect();
        all.sort_by(f32::total_cmp);
        assert!(all.first().unwrap() > &0.0 && all.last().unwrap() < &1.0);
        all.dedup();
        assert_eq!(all.len(), 16);
    }
}
