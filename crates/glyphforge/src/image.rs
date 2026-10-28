//! Image import: PNG files become pixel layers. Decoding lives here (the
//! core stays free of file formats); fitting and resampling are in
//! `glyphforge_core::raster::import`.

use std::path::Path;

use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::raster::RasterMode;
use glyphforge_core::raster::import::{Rgba, fitted_size, resample};
use glyphforge_core::{Document, Layer, ObjectId, Size};

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} is not a PNG image Glyphforge can read: {source}")]
    Decode {
        path: String,
        source: png::DecodingError,
    },
    #[error("unsupported PNG colour type {0:?}")]
    ColorType(png::ColorType),
    #[error("no screen {0}")]
    NoScreen(ObjectId),
}

/// Decodes a PNG file to RGBA8 (palette, grey and 16-bit images included).
pub fn load_png(path: &Path) -> Result<Rgba, ImageError> {
    let shown = || path.display().to_string();
    let file = std::fs::File::open(path).map_err(|source| ImageError::Read {
        path: shown(),
        source,
    })?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let decode = |source| ImageError::Decode {
        path: shown(),
        source,
    };
    let mut reader = decoder.read_info().map_err(decode)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(decode)?;
    let bytes = &buf[..info.buffer_size()];
    let data: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => bytes
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => bytes.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other @ png::ColorType::Indexed => return Err(ImageError::ColorType(other)),
    };
    Ok(Rgba {
        width: info.width,
        height: info.height,
        data,
    })
}

/// How to place an imported image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    pub mode: RasterMode,
    /// Cell offset of the image's top-left corner.
    pub at: (u16, u16),
    /// Area to fit into, in cells (default: the rest of the screen).
    pub fit: Option<Size>,
    pub name: String,
}

/// A patch adding a pixel layer with `image` on top of `screen`, and the
/// new layer's id.
pub fn import_patch(
    doc: &Document,
    screen: &ObjectId,
    image: &Rgba,
    options: &ImportOptions,
) -> Result<(Patch, ObjectId), ImageError> {
    let s = doc
        .screen(screen)
        .ok_or_else(|| ImageError::NoScreen(screen.clone()))?;
    let size = s.size();
    let room = Size::new(
        size.width.saturating_sub(options.at.0).max(1),
        size.height.saturating_sub(options.at.1).max(1),
    );
    let fit = options.fit.unwrap_or(room);
    let fit = Size::new(fit.width.min(room.width), fit.height.min(room.height));
    let (w, h) = fitted_size(image.width, image.height, fit, options.mode);
    let pixels = resample(image, w, h);
    let ids = doc.ids();
    let id = ObjectId::slugify(&options.name).unique_among(|i| ids.contains(i));
    let mut layer = Layer::raster_layer(id.clone(), &options.name, size, options.mode);
    if let Some(raster) = layer.raster_mut() {
        let (cw, ch) = options.mode.cell_px();
        let (ox, oy) = (options.at.0 * cw, options.at.1 * ch);
        for y in 0..h {
            for x in 0..w {
                if let Some(p) = pixels.get(x, y).filter(|p| !p.is_clear()) {
                    raster.pixels.set(ox + x, oy + y, p);
                }
            }
        }
    }
    Ok((
        Patch::single(Operation::CreateLayer {
            layer,
            screen: Some(screen.clone()),
            index: None,
        }),
        id,
    ))
}

/// A patch adding an artwork layer with text or ANSI art (SGR colours are
/// kept) at cell `at`, and the new layer's id.
pub fn text_patch(
    doc: &Document,
    screen: &ObjectId,
    text: &str,
    at: (u16, u16),
    name: &str,
) -> Result<(Patch, ObjectId), ImageError> {
    let s = doc
        .screen(screen)
        .ok_or_else(|| ImageError::NoScreen(screen.clone()))?;
    let art = glyphforge_core::export::ansi::from_ansi(text, None);
    let ids = doc.ids();
    let id = ObjectId::slugify(name).unique_among(|i| ids.contains(i));
    let mut layer = Layer::artwork(id.clone(), name, s.size())
        .unwrap_or_else(|_| unreachable!("screen sizes are valid canvas sizes"));
    if let Some(cells) = layer.cells_mut() {
        for p in art.bounds().positions() {
            let Some(cell) = art.get(p).filter(|c| !c.is_empty() && !c.is_wide_tail()) else {
                continue;
            };
            let target =
                glyphforge_core::Position::new(p.x.saturating_add(at.0), p.y.saturating_add(at.1));
            let _ = cells.put(target, cell.clone());
        }
    }
    Ok((
        Patch::single(Operation::CreateLayer {
            layer,
            screen: Some(screen.clone()),
            index: None,
        }),
        id,
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes a small RGBA PNG: a red square on transparency.
    pub(crate) fn write_png(path: &Path, w: u32, h: u32) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let data: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                if x >= w / 4 && x < w * 3 / 4 && y >= h / 4 && y < h * 3 / 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                }
            })
            .collect();
        writer.write_image_data(&data).unwrap();
    }

    #[test]
    fn png_becomes_a_pixel_layer() {
        let dir = std::env::temp_dir().join(format!("glyphforge-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.png");
        write_png(&path, 16, 16);
        let image = load_png(&path).unwrap();
        assert_eq!((image.width, image.height), (16, 16));
        let mut doc = Document::new(Size::new(20, 10)).unwrap();
        let screen = doc.first_screen().id.clone();
        let (patch, id) = import_patch(
            &doc,
            &screen,
            &image,
            &ImportOptions {
                mode: RasterMode::Half,
                at: (0, 0),
                fit: Some(Size::new(8, 4)),
                name: "Logo".into(),
            },
        )
        .unwrap();
        patch.apply(&mut doc).unwrap();
        let raster = doc.layer(&id).unwrap().raster().unwrap();
        // 8×4 cells is an 8×8 physical box: 8×8 half-block pixels.
        assert!(raster.pixels.get(4, 4).is_some_and(|p| !p.is_clear()));
        assert!(
            raster
                .pixels
                .get(0, 0)
                .is_some_and(glyphforge_core::raster::Pixel::is_clear)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
