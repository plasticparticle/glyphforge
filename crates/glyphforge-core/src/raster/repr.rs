//! Diff-friendly serialisation of raster layers: a palette of distinct
//! pixel values and one text row per pixel row, one short code per pixel.
//! Painting a stroke changes the rows it touches, not the whole file.

use serde::{Deserialize, Serialize};

use super::{Dither, DropShadow, PaletteLimit, Pixel, Raster, RasterLayer, RasterMode};

/// Pixel codes: printable ASCII without `.` (clear), `"` and `\` (which
/// JSON would escape).
const ALPHABET: &[u8] =
    b"!#$%&'()*+,-/0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[]^_`abcdefghijklmnopqrstuvwxyz{|}~";
const CLEAR: char = '.';

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RasterRepr {
    pub mode: RasterMode,
    #[serde(default, skip_serializing_if = "is_default")]
    pub dither: Dither,
    #[serde(default, skip_serializing_if = "is_default")]
    pub palette_limit: PaletteLimit,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<DropShadow>,
    /// Size in pixels.
    pub width: u16,
    pub height: u16,
    /// Distinct pixel values; row codes index into this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette: Vec<Pixel>,
    /// Characters per code (1 for up to 90 colours, more beyond).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub code_width: u8,
    /// One string per pixel row; trailing clear pixels are omitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<String>,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

const fn one() -> u8 {
    1
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if takes a reference"
)]
const fn is_one(v: &u8) -> bool {
    *v == 1
}

fn code(mut index: usize, width: u8) -> String {
    let n = ALPHABET.len();
    let mut out = vec![0u8; usize::from(width)];
    for slot in out.iter_mut().rev() {
        *slot = ALPHABET[index % n];
        index /= n;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn decode(code: &[u8]) -> Option<usize> {
    code.iter().try_fold(0usize, |acc, b| {
        ALPHABET
            .iter()
            .position(|a| a == b)
            .map(|d| acc * ALPHABET.len() + d)
    })
}

impl From<&RasterLayer> for RasterRepr {
    fn from(layer: &RasterLayer) -> Self {
        let raster = &layer.pixels;
        let mut palette: Vec<Pixel> = Vec::new();
        for p in raster.pixels() {
            if !p.is_clear() && !palette.contains(p) {
                palette.push(*p);
            }
        }
        let mut code_width = 1u8;
        while ALPHABET.len().pow(u32::from(code_width)) < palette.len() {
            code_width += 1;
        }
        let clear = CLEAR.to_string().repeat(usize::from(code_width));
        let mut rows: Vec<String> = (0..raster.height())
            .map(|y| {
                let mut row = String::new();
                for x in 0..raster.width() {
                    match raster.get(x, y).unwrap_or_default() {
                        Pixel::Clear => row.push_str(&clear),
                        p => {
                            let i = palette.iter().position(|q| *q == p).unwrap_or(0);
                            row.push_str(&code(i, code_width));
                        }
                    }
                }
                while row.ends_with(&clear) {
                    row.truncate(row.len() - clear.len());
                }
                row
            })
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        Self {
            mode: layer.mode,
            dither: layer.dither,
            palette_limit: layer.palette,
            shadow: layer.shadow,
            width: raster.width(),
            height: raster.height(),
            palette,
            code_width,
            rows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RasterReprError {
    #[error("raster has {rows} rows but is {height} pixels high")]
    TooManyRows { rows: usize, height: u16 },
    #[error("row {y} is longer than the raster width {width}")]
    RowTooLong { y: usize, width: u16 },
    #[error("row {y} has an unknown pixel code {code:?}")]
    BadCode { y: usize, code: String },
    #[error("raster size {width}x{height} is not a multiple of the {mode} cell size")]
    NotCellAligned {
        width: u16,
        height: u16,
        mode: &'static str,
    },
}

impl TryFrom<RasterRepr> for RasterLayer {
    type Error = RasterReprError;

    fn try_from(r: RasterRepr) -> Result<Self, Self::Error> {
        let (cw, ch) = r.mode.cell_px();
        if r.width % cw != 0 || r.height % ch != 0 {
            return Err(RasterReprError::NotCellAligned {
                width: r.width,
                height: r.height,
                mode: r.mode.name(),
            });
        }
        if r.rows.len() > usize::from(r.height) {
            return Err(RasterReprError::TooManyRows {
                rows: r.rows.len(),
                height: r.height,
            });
        }
        let width = usize::from(r.code_width.max(1));
        let mut pixels = Raster::new(r.width, r.height);
        for (y, row) in r.rows.iter().enumerate() {
            let bytes = row.as_bytes();
            if bytes.len() % width != 0 || bytes.len() / width > usize::from(r.width) {
                return Err(RasterReprError::RowTooLong { y, width: r.width });
            }
            for (x, chunk) in bytes.chunks(width).enumerate() {
                if chunk.iter().all(|b| *b == CLEAR as u8) {
                    continue;
                }
                let pixel = decode(chunk)
                    .and_then(|i| r.palette.get(i))
                    .ok_or_else(|| RasterReprError::BadCode {
                        y,
                        code: String::from_utf8_lossy(chunk).into_owned(),
                    })?;
                pixels.set(x as u16, y as u16, *pixel);
            }
        }
        Ok(Self {
            mode: r.mode,
            dither: r.dither,
            palette: r.palette_limit,
            shadow: r.shadow,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Color, Size};

    #[test]
    fn rows_are_compact_and_round_trip() {
        let mut layer = RasterLayer::new(Size::new(3, 2), RasterMode::Quadrant);
        layer.pixels.set(1, 0, Pixel::Solid(Color::Indexed(1)));
        layer.pixels.set(2, 0, Pixel::Solid(Color::Rgb(0, 255, 0)));
        layer.pixels.set(0, 1, Pixel::Solid(Color::Indexed(1)));
        let repr = RasterRepr::from(&layer);
        assert_eq!(repr.rows, vec![".!#", "!"]);
        assert_eq!(repr.palette.len(), 2);
        let json = serde_json::to_string(&layer).unwrap();
        assert!(!json.contains("dither"), "defaults omitted: {json}");
        let back: RasterLayer = serde_json::from_str(&json).unwrap();
        assert_eq!(back, layer);
    }

    #[test]
    fn many_colours_use_wider_codes() {
        let mut layer = RasterLayer::new(Size::new(100, 1), RasterMode::Quadrant);
        for x in 0..200u16 {
            layer
                .pixels
                .set(x, 0, Pixel::Solid(Color::Rgb(x as u8, 0, 0)));
        }
        let repr = RasterRepr::from(&layer);
        assert_eq!(repr.code_width, 2);
        let back = RasterLayer::try_from(repr).unwrap();
        assert_eq!(back, layer);
    }

    #[test]
    fn bad_input_is_rejected() {
        let base = RasterRepr::from(&RasterLayer::new(Size::new(1, 1), RasterMode::Half));
        let bad = RasterRepr {
            rows: vec!["!".into()],
            ..base.clone()
        };
        assert!(matches!(
            RasterLayer::try_from(bad),
            Err(RasterReprError::BadCode { .. })
        ));
        let misaligned = RasterRepr { height: 3, ..base };
        assert!(RasterLayer::try_from(misaligned).is_err());
    }
}
