//! The encoder: pixels to cells.
//!
//! For every cell it gathers the cell's pixels (with the drop shadow under
//! transparent ones), resolves transparency against the backdrop, applies
//! the palette limit and dithering, then picks the glyph and the two
//! colours that represent the pixels best.

use super::{PaletteLimit, Pixel, Raster, RasterLayer, RasterMode, ShadeLevel, bayer};
use crate::document::{Canvas, Cell, CellStyle, Color, Grapheme, Position, Size};

type Rgb = (u8, u8, u8);

/// The glyph of a shade level.
pub const fn shade_glyph(level: ShadeLevel) -> &'static str {
    match level {
        ShadeLevel::Light => "░",
        ShadeLevel::Medium => "▒",
        ShadeLevel::Dark => "▓",
    }
}

fn rgb(c: Color) -> Rgb {
    c.to_rgb().unwrap_or((0, 0, 0))
}

/// Perceptually weighted squared distance.
fn dist(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    2 * d(a.0, b.0) + 4 * d(a.1, b.1) + 3 * d(a.2, b.2)
}

/// `fg` over `bg` with `quarters`/4 coverage. Without known RGB values
/// the dominant colour wins.
fn blend(fg: Color, bg: Color, quarters: u8) -> Color {
    match (fg.to_rgb(), bg.to_rgb()) {
        (Some(_), Some(_)) => {
            let mix = |a: u8, b: u8| {
                ((u16::from(a) * u16::from(quarters) + u16::from(b) * u16::from(4 - quarters) + 2)
                    / 4) as u8
            };
            let (f, b) = (rgb(fg), rgb(bg));
            Color::Rgb(mix(f.0, b.0), mix(f.1, b.1), mix(f.2, b.2))
        }
        _ if quarters >= 2 => fg,
        _ => bg,
    }
}

fn palette_colors(limit: PaletteLimit) -> &'static [(Color, Rgb)] {
    use std::sync::OnceLock;
    static ANSI16: OnceLock<Vec<(Color, Rgb)>> = OnceLock::new();
    static ANSI256: OnceLock<Vec<(Color, Rgb)>> = OnceLock::new();
    let build = |n: u16| {
        (0..n)
            .map(|i| {
                let c = Color::Indexed(i as u8);
                (c, rgb(c))
            })
            .collect()
    };
    match limit {
        PaletteLimit::Truecolor => &[],
        PaletteLimit::Ansi16 => ANSI16.get_or_init(|| build(16)),
        PaletteLimit::Ansi256 => ANSI256.get_or_init(|| build(256)),
    }
}

fn nearest(target: Rgb, palette: &[(Color, Rgb)]) -> Color {
    palette
        .iter()
        .min_by_key(|(_, p)| dist(*p, target))
        .map_or(Color::Default, |(c, _)| *c)
}

/// Reduces a colour to the palette; ordered dithering nudges the target
/// by the Bayer threshold at the pixel before choosing.
fn reduce(c: Color, limit: PaletteLimit, ordered: bool, x: u16, y: u16) -> Color {
    let palette = palette_colors(limit);
    if palette.is_empty() || c == Color::Default {
        return c;
    }
    let target = rgb(c);
    if !ordered {
        return nearest(target, palette);
    }
    let spread = match limit {
        PaletteLimit::Ansi16 => 100.0,
        _ => 36.0,
    };
    let offset = (bayer(x, y) - 0.5) * spread;
    let nudge = |v: u8| (f32::from(v) + offset).round().clamp(0.0, 255.0) as u8;
    nearest((nudge(target.0), nudge(target.1), nudge(target.2)), palette)
}

/// The shade glyph mix of two palette colours closest to `c`, if it is
/// closer than the nearest single palette colour.
fn best_shade(c: Color, limit: PaletteLimit) -> Option<(Color, Color, ShadeLevel)> {
    let palette = palette_colors(limit);
    let target = rgb(c);
    let mut ranked: Vec<&(Color, Rgb)> = palette.iter().collect();
    ranked.sort_by_key(|(_, p)| dist(*p, target));
    let solid = dist(ranked.first()?.1, target);
    let candidates = &ranked[..ranked.len().min(8)];
    let mut best: Option<(u32, Color, Color, ShadeLevel)> = None;
    for a in candidates {
        for b in candidates {
            if a.0 == b.0 {
                continue;
            }
            for level in ShadeLevel::ALL {
                let mixed = rgb(blend(a.0, b.0, level.quarters()));
                let e = dist(mixed, target);
                if best.is_none_or(|(be, ..)| e < be) {
                    best = Some((e, a.0, b.0, level));
                }
            }
        }
    }
    best.filter(|(e, ..)| *e < solid)
        .map(|(_, a, b, l)| (a, b, l))
}

/// Encodes a raster layer into cells of `size`. Transparent pixels take
/// the background colour of `backdrop` (what the layers beneath render);
/// fully transparent cells stay transparent.
pub fn encode(layer: &RasterLayer, backdrop: &Canvas, size: Size) -> Canvas {
    let Ok(mut out) = Canvas::new(size) else {
        return backdrop.clone();
    };
    let diffused;
    let layer = if layer.dither.diffuses() && layer.palette != PaletteLimit::Truecolor {
        diffused = diffuse(layer);
        &diffused
    } else {
        layer
    };
    let shadow = layer
        .shadow
        .map(|s| s.render(&layer.pixels, layer.mode, layer.dither));
    let (cw, ch) = layer.mode.cell_px();
    let cells = layer.cells();
    let mut px = Vec::with_capacity(usize::from(cw * ch));
    for cy in 0..cells.height.min(size.height) {
        for cx in 0..cells.width.min(size.width) {
            px.clear();
            for dy in 0..ch {
                for dx in 0..cw {
                    let (x, y) = (cx * cw + dx, cy * ch + dy);
                    let own = layer.pixels.get(x, y).unwrap_or_default();
                    let under = shadow
                        .as_ref()
                        .and_then(|s: &Raster| s.get(x, y))
                        .unwrap_or_default();
                    px.push(if own.is_clear() { under } else { own });
                }
            }
            let pos = Position::new(cx, cy);
            let back = backdrop
                .get(pos)
                .filter(|c| !c.is_empty())
                .map_or(Color::Default, |c| c.style.bg);
            if let Some(cell) = encode_cell(layer, &px, back, (cx * cw, cy * ch)) {
                let _ = out.put(pos, cell);
            }
        }
    }
    out
}

/// Reduces every solid pixel to the palette in reading order, spreading
/// each pixel's error to its not-yet-visited solid neighbours
/// (Floyd–Steinberg 7/3/5/1 sixteenths, or Atkinson's six eighths).
/// Transparent and shade pixels neither take nor pass on error.
#[allow(clippy::many_single_char_names, reason = "pixel and channel indices")]
fn diffuse(layer: &RasterLayer) -> RasterLayer {
    use super::Dither;
    const FLOYD: &[(i32, i32, f32)] = &[
        (1, 0, 7.0 / 16.0),
        (-1, 1, 3.0 / 16.0),
        (0, 1, 5.0 / 16.0),
        (1, 1, 1.0 / 16.0),
    ];
    const ATKINSON: &[(i32, i32, f32)] = &[
        (1, 0, 0.125),
        (2, 0, 0.125),
        (-1, 1, 0.125),
        (0, 1, 0.125),
        (1, 1, 0.125),
        (0, 2, 0.125),
    ];
    let kernel = if layer.dither == Dither::Atkinson {
        ATKINSON
    } else {
        FLOYD
    };
    let palette = palette_colors(layer.palette);
    let (w, h) = (layer.pixels.width(), layer.pixels.height());
    let mut error = vec![[0.0f32; 3]; usize::from(w) * usize::from(h)];
    let mut out = layer.clone();
    out.dither = Dither::None;
    for y in 0..h {
        for x in 0..w {
            let Some(Pixel::Solid(c)) = layer.pixels.get(x, y) else {
                continue;
            };
            if c == Color::Default {
                continue;
            }
            let i = usize::from(y) * usize::from(w) + usize::from(x);
            let (r, g, b) = rgb(c);
            let want = [
                f32::from(r) + error[i][0],
                f32::from(g) + error[i][1],
                f32::from(b) + error[i][2],
            ];
            let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            let chosen = nearest((clamp(want[0]), clamp(want[1]), clamp(want[2])), palette);
            let got = rgb(chosen);
            let diff = [
                want[0] - f32::from(got.0),
                want[1] - f32::from(got.1),
                want[2] - f32::from(got.2),
            ];
            out.pixels.set(x, y, Pixel::Solid(chosen));
            for &(dx, dy, share) in kernel {
                let (nx, ny) = (i32::from(x) + dx, i32::from(y) + dy);
                if nx < 0 || ny < 0 || nx >= i32::from(w) || ny >= i32::from(h) {
                    continue;
                }
                let (nx, ny) = (nx as u16, ny as u16);
                if !matches!(layer.pixels.get(nx, ny), Some(Pixel::Solid(_))) {
                    continue;
                }
                let j = usize::from(ny) * usize::from(w) + usize::from(nx);
                for k in 0..3 {
                    error[j][k] += diff[k] * share;
                }
            }
        }
    }
    out
}

fn glyph_cell(glyph: &str, fg: Color, bg: Color) -> Option<Cell> {
    Grapheme::new(glyph)
        .ok()
        .map(|g| Cell::glyph(g, CellStyle::new(fg, bg)))
}

fn encode_cell(layer: &RasterLayer, px: &[Pixel], back: Color, origin: (u16, u16)) -> Option<Cell> {
    if px.iter().all(|p| p.is_clear()) {
        return None;
    }
    if let Pixel::Shade { fg, bg, level } = px[0] {
        if px.iter().all(|p| *p == px[0]) {
            return glyph_cell(shade_glyph(level), fg, bg.unwrap_or(back));
        }
    }
    let (cw, _) = layer.mode.cell_px();
    let mut cols: Vec<Option<Color>> = px
        .iter()
        .map(|p| match *p {
            Pixel::Clear => None,
            Pixel::Solid(c) => Some(c),
            Pixel::Shade { fg, bg, level } => Some(blend(fg, bg.unwrap_or(back), level.quarters())),
        })
        .collect();
    if layer.palette != PaletteLimit::Truecolor {
        if layer.dither.shade() && cols.iter().all(|c| *c == cols[0]) {
            if let Some((a, b, level)) = cols[0].and_then(|c| best_shade(c, layer.palette)) {
                return glyph_cell(shade_glyph(level), a, b);
            }
        }
        for (i, c) in cols.iter_mut().enumerate() {
            let (x, y) = (origin.0 + i as u16 % cw, origin.1 + i as u16 / cw);
            *c = c.map(|c| reduce(c, layer.palette, layer.dither.ordered(), x, y));
        }
    }
    encode_block(layer.mode, &cols, back)
}

/// A pixel's colour in a cell: its own, or the backdrop (`Back`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    Back,
    Col(Color),
}

impl Ink {
    fn color(self, back: Color) -> Color {
        match self {
            Self::Back => back,
            Self::Col(c) => c,
        }
    }
}

const QUADRANTS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

/// Block glyph for `mask` (bit i = pixel i in row-major order is ink).
fn block_glyph(mode: RasterMode, mask: u8) -> Option<String> {
    Some(match mode {
        RasterMode::Half => [" ", "▀", "▄", "█"][usize::from(mask & 3)].to_owned(),
        RasterMode::Quadrant => QUADRANTS[usize::from(mask & 15)].to_owned(),
        RasterMode::Sextant => match mask & 63 {
            0 => " ".to_owned(),
            21 => "▌".to_owned(),
            42 => "▐".to_owned(),
            63 => "█".to_owned(),
            m => {
                let skip = u32::from(m > 21) + u32::from(m > 42);
                char::from_u32(0x1FB00 + u32::from(m) - 1 - skip)?.to_string()
            }
        },
        RasterMode::Octant => super::octant::octant(mask)?.to_string(),
        RasterMode::Braille => return None,
    })
}

fn encode_block(mode: RasterMode, cols: &[Option<Color>], back: Color) -> Option<Cell> {
    let inks: Vec<Ink> = cols.iter().map(|c| c.map_or(Ink::Back, Ink::Col)).collect();
    if mode == RasterMode::Braille {
        return encode_braille(&inks, back);
    }
    let mut distinct: Vec<Ink> = Vec::new();
    for ink in &inks {
        if !distinct.contains(ink) {
            distinct.push(*ink);
        }
    }
    let ink_rgb = |i: Ink| rgb(i.color(back));
    let (mut a, mut b) = match distinct.as_slice() {
        [Ink::Back] => return None,
        [only] => (*only, Ink::Back),
        [x, y] => (*x, *y),
        _ => {
            let mut best = (u32::MAX, distinct[0], distinct[1]);
            for (i, x) in distinct.iter().enumerate() {
                for y in &distinct[i + 1..] {
                    let e: u32 = inks
                        .iter()
                        .map(|p| dist(ink_rgb(*p), ink_rgb(*x)).min(dist(ink_rgb(*p), ink_rgb(*y))))
                        .sum();
                    if e < best.0 {
                        best = (e, *x, *y);
                    }
                }
            }
            (best.1, best.2)
        }
    };
    if a == Ink::Back {
        std::mem::swap(&mut a, &mut b);
    }
    let mask = inks.iter().enumerate().fold(0u8, |m, (i, p)| {
        let is_a =
            *p == a || (*p != b && dist(ink_rgb(*p), ink_rgb(a)) <= dist(ink_rgb(*p), ink_rgb(b)));
        if is_a { m | (1 << i) } else { m }
    });
    let glyph = block_glyph(mode, mask)?;
    glyph_cell(&glyph, a.color(back), b.color(back))
}

fn encode_braille(inks: &[Ink], back: Color) -> Option<Cell> {
    const DOTS: [u32; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];
    let mut counts: Vec<(Color, usize)> = Vec::new();
    let mut bits = 0u32;
    for (i, ink) in inks.iter().enumerate() {
        if let Ink::Col(c) = ink {
            bits |= DOTS[i];
            match counts.iter_mut().find(|(k, _)| k == c) {
                Some((_, n)) => *n += 1,
                None => counts.push((*c, 1)),
            }
        }
    }
    let fg = counts.iter().max_by_key(|(_, n)| *n).map(|(c, _)| *c)?;
    let glyph = char::from_u32(0x2800 + bits)?.to_string();
    glyph_cell(&glyph, fg, back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{Dither, DropShadow};

    const RED: Color = Color::Rgb(255, 0, 0);
    const BLUE: Color = Color::Rgb(0, 0, 255);

    fn layer(mode: RasterMode, cells: Size) -> RasterLayer {
        RasterLayer::new(cells, mode)
    }

    fn glyphs(canvas: &Canvas) -> Vec<String> {
        crate::render::to_text_lines(canvas)
    }

    fn at(canvas: &Canvas, x: u16, y: u16) -> &Cell {
        canvas.get(Position::new(x, y)).unwrap()
    }

    #[test]
    fn every_mask_has_the_right_block() {
        let mut l = layer(RasterMode::Sextant, Size::new(1, 1));
        let backdrop = Canvas::new(Size::new(1, 1)).unwrap();
        for mask in 1u8..64 {
            for i in 0..6u16 {
                let p = if mask & (1 << i) == 0 {
                    Pixel::Clear
                } else {
                    Pixel::Solid(RED)
                };
                l.pixels.set(i % 2, i / 2, p);
            }
            let out = encode(&l, &backdrop, Size::new(1, 1));
            let cell = at(&out, 0, 0);
            assert_eq!(
                cell.as_glyph().unwrap().as_str(),
                block_glyph(RasterMode::Sextant, mask).unwrap()
            );
            assert_eq!(cell.style.fg, RED);
            assert_eq!(
                cell.style.bg,
                Color::Default,
                "transparent pixels show the backdrop"
            );
        }
        l.pixels = Raster::new(2, 3);
        assert!(
            encode(&l, &backdrop, Size::new(1, 1))
                .get(Position::ORIGIN)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn two_colours_per_cell_and_best_pair_for_more() {
        let mut l = layer(RasterMode::Half, Size::new(2, 1));
        l.pixels.set(0, 0, Pixel::Solid(RED));
        l.pixels.set(0, 1, Pixel::Solid(BLUE));
        l.pixels.set(1, 1, Pixel::Solid(BLUE));
        let backdrop = Canvas::new(Size::new(2, 1)).unwrap();
        let out = encode(&l, &backdrop, Size::new(2, 1));
        assert_eq!(glyphs(&out), vec!["▀▄"]);
        assert_eq!(
            (at(&out, 0, 0).style.fg, at(&out, 0, 0).style.bg),
            (RED, BLUE)
        );

        let mut q = layer(RasterMode::Quadrant, Size::new(1, 1));
        let dark_red = Color::Rgb(230, 0, 0);
        q.pixels.set(0, 0, Pixel::Solid(RED));
        q.pixels.set(1, 0, Pixel::Solid(dark_red));
        q.pixels.set(0, 1, Pixel::Solid(BLUE));
        q.pixels.set(1, 1, Pixel::Solid(BLUE));
        let out = encode(&q, &Canvas::new(Size::new(1, 1)).unwrap(), Size::new(1, 1));
        assert_eq!(glyphs(&out), vec!["▀"], "similar reds merge, blue stays");
    }

    #[test]
    fn transparency_uses_the_backdrop_background() {
        let mut l = layer(RasterMode::Half, Size::new(1, 1));
        l.pixels.set(0, 1, Pixel::Solid(RED));
        let mut backdrop = Canvas::new(Size::new(1, 1)).unwrap();
        backdrop
            .put(
                Position::ORIGIN,
                Cell::blank(CellStyle::new(Color::Default, BLUE)),
            )
            .unwrap();
        let out = encode(&l, &backdrop, Size::new(1, 1));
        let cell = at(&out, 0, 0);
        assert_eq!(cell.as_glyph().unwrap().as_str(), "▄");
        assert_eq!((cell.style.fg, cell.style.bg), (RED, BLUE));
    }

    #[test]
    fn braille_uses_one_colour() {
        let mut l = layer(RasterMode::Braille, Size::new(1, 1));
        l.pixels.set(0, 0, Pixel::Solid(RED));
        l.pixels.set(1, 3, Pixel::Solid(RED));
        l.pixels.set(0, 3, Pixel::Solid(BLUE));
        let out = encode(&l, &Canvas::new(Size::new(1, 1)).unwrap(), Size::new(1, 1));
        assert_eq!(glyphs(&out), vec!["⣁"]);
        assert_eq!(at(&out, 0, 0).style.fg, RED, "majority colour");
    }

    #[test]
    fn shade_pixels_and_palette_dithering() {
        let mut l = layer(RasterMode::Quadrant, Size::new(2, 1));
        let shade = Pixel::Shade {
            fg: RED,
            bg: Some(BLUE),
            level: ShadeLevel::Light,
        };
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            l.pixels.set(x, y, shade);
        }
        let backdrop = Canvas::new(Size::new(2, 1)).unwrap();
        let out = encode(&l, &backdrop, Size::new(2, 1));
        assert_eq!(glyphs(&out), vec!["░"]);

        // An orange that ANSI 16 lacks: shade dithering mixes red and yellow.
        let orange = Pixel::Solid(Color::Rgb(230, 115, 0));
        for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
            l.pixels.set(x, y, orange);
        }
        l.palette = PaletteLimit::Ansi16;
        l.dither = Dither::Shade;
        let out = encode(&l, &backdrop, Size::new(2, 1));
        let cell = at(&out, 1, 0);
        assert!(
            ["░", "▒", "▓"].contains(&cell.as_glyph().unwrap().as_str()),
            "{cell:?}"
        );
        assert!(
            matches!(cell.style.fg, Color::Indexed(_))
                && matches!(cell.style.bg, Color::Indexed(_))
        );

        l.dither = Dither::None;
        let out = encode(&l, &backdrop, Size::new(2, 1));
        assert_eq!(at(&out, 1, 0).as_glyph().unwrap().as_str(), "█");
        assert!(matches!(at(&out, 1, 0).style.fg, Color::Indexed(_)));

        // Ordered dithering of a mid grey in ANSI 16 produces a pattern.
        let mut g = layer(RasterMode::Quadrant, Size::new(4, 4));
        for y in 0..8 {
            for x in 0..8 {
                g.pixels.set(x, y, Pixel::Solid(Color::Rgb(96, 96, 96)));
            }
        }
        g.palette = PaletteLimit::Ansi16;
        g.dither = Dither::Ordered;
        let out = encode(&g, &Canvas::new(Size::new(4, 4)).unwrap(), Size::new(4, 4));
        let patterned = out
            .cells()
            .iter()
            .filter(|c| c.as_glyph().is_some_and(|g| g.as_str() != "█"))
            .count();
        assert!(patterned > 0, "{:?}", glyphs(&out));
    }

    #[test]
    fn shadow_renders_under_transparent_pixels() {
        let mut l = layer(RasterMode::Half, Size::new(3, 2));
        l.pixels.set(0, 0, Pixel::Solid(RED));
        l.pixels.set(0, 1, Pixel::Solid(RED));
        l.shadow = Some(DropShadow {
            dx: 1,
            dy: 2,
            color: BLUE,
            softness: 0,
            opacity: 100,
        });
        let out = encode(&l, &Canvas::new(Size::new(3, 2)).unwrap(), Size::new(3, 2));
        assert_eq!(at(&out, 0, 0).style.fg, RED);
        let shadow = at(&out, 1, 1);
        assert_eq!(shadow.as_glyph().unwrap().as_str(), "█");
        assert_eq!(shadow.style.fg, BLUE);
    }

    #[test]
    fn error_diffusion_mixes_palette_colours_for_in_between_shades() {
        use super::super::Dither;
        // A mid grey over 16 colours: nearest alone gives one colour,
        // diffusion alternates between neighbours of the grey.
        let mut l = layer(RasterMode::Quadrant, Size::new(4, 4));
        l.palette = PaletteLimit::Ansi16;
        for y in 0..8 {
            for x in 0..8 {
                l.pixels.set(x, y, Pixel::Solid(Color::Rgb(100, 100, 100)));
            }
        }
        let back = Canvas::new(Size::new(4, 4)).unwrap();
        let count = |l: &RasterLayer| {
            let mut colours = std::collections::BTreeSet::new();
            let out = encode(l, &back, Size::new(4, 4));
            for p in out.bounds().positions() {
                let c = out.get(p).unwrap();
                colours.insert(format!("{}{}", c.style.fg, c.style.bg));
            }
            colours.len()
        };
        let flat = count(&l);
        for d in [Dither::FloydSteinberg, Dither::Atkinson] {
            l.dither = d;
            assert!(count(&l) > flat, "{d:?} varies the cells");
        }
    }
}
