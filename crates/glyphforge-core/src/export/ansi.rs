//! ANSI art in and out: rendering a canvas as escape sequences at a
//! chosen colour depth (with minimal SGR changes between cells and a clean
//! reset), and reading text with SGR colour codes back into cells.

use std::fmt::Write as _;

use crate::document::{Attributes, Canvas, Cell, CellStyle, Color, Grapheme, Position, Size};

/// How many colours the output may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Depth {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl Depth {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ansi" | "truecolor" | "ansi-truecolor" => Some(Self::TrueColor),
            "ansi256" | "ansi-256" => Some(Self::Ansi256),
            "ansi16" | "ansi-16" => Some(Self::Ansi16),
            _ => None,
        }
    }
}

/// The nearest palette colour of `c` for `depth` (perceptually weighted).
pub fn reduce(c: Color, depth: Depth) -> Color {
    let n: u8 = match (depth, c) {
        (Depth::TrueColor, _) | (_, Color::Default) | (Depth::Ansi256, Color::Indexed(_)) => {
            return c;
        }
        (Depth::Ansi16, Color::Indexed(i)) if i < 16 => return c,
        (Depth::Ansi256, _) => 255,
        (Depth::Ansi16, _) => 15,
    };
    let Some(target) = c.to_rgb() else { return c };
    let d = |a: (u8, u8, u8), b: (u8, u8, u8)| {
        let sq = |x: u8, y: u8| i32::from(x.abs_diff(y)).pow(2);
        2 * sq(a.0, b.0) + 4 * sq(a.1, b.1) + 3 * sq(a.2, b.2)
    };
    (0..=n)
        .map(Color::Indexed)
        .min_by_key(|p| p.to_rgb().map_or(i32::MAX, |rgb| d(rgb, target)))
        .unwrap_or(c)
}

fn color_params(c: Color, background: bool) -> String {
    let base = if background { 40 } else { 30 };
    match c {
        Color::Default => format!("{}", base + 9),
        Color::Indexed(i) if i < 8 => format!("{}", base + u16::from(i)),
        Color::Indexed(i) if i < 16 => format!("{}", base + 60 + u16::from(i) - 8),
        Color::Indexed(i) => format!("{};5;{i}", base + 8),
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
    }
}

/// SGR parameters that change `from` into `to`, or `None` if equal.
fn sgr_delta(from: &CellStyle, to: &CellStyle) -> Option<String> {
    if from == to {
        return None;
    }
    let a = &to.attrs;
    let b = &from.attrs;
    // Turning an attribute off has awkward codes (22 clears bold and dim
    // together); a reset is shorter and always correct.
    let lost = (b.bold && !a.bold)
        || (b.dim && !a.dim)
        || (b.italic && !a.italic)
        || (b.underline && !a.underline)
        || (b.reverse && !a.reverse)
        || (b.strikethrough && !a.strikethrough)
        || (b.blink && !a.blink);
    let (base, mut params): (&CellStyle, Vec<String>) = if lost {
        (&CellStyle::DEFAULT, vec!["0".into()])
    } else {
        (from, Vec::new())
    };
    let ba = &base.attrs;
    for (on, was, code) in [
        (a.bold, ba.bold, "1"),
        (a.dim, ba.dim, "2"),
        (a.italic, ba.italic, "3"),
        (a.underline, ba.underline, "4"),
        (a.blink, ba.blink, "5"),
        (a.reverse, ba.reverse, "7"),
        (a.strikethrough, ba.strikethrough, "9"),
    ] {
        if on && !was {
            params.push(code.into());
        }
    }
    if to.fg != base.fg {
        params.push(color_params(to.fg, false));
    }
    if to.bg != base.bg {
        params.push(color_params(to.bg, true));
    }
    Some(params.join(";"))
}

/// The canvas as lines of text with SGR escapes at `depth`. Every line
/// ends with a reset when it changed the style, so the output can be
/// printed, concatenated or cut by line safely.
pub fn to_ansi(canvas: &Canvas, depth: Depth) -> String {
    let mut out = String::new();
    for y in 0..canvas.height() {
        let mut current = CellStyle::DEFAULT;
        let mut line = String::new();
        let mut pending_spaces = 0usize;
        for x in 0..canvas.width() {
            let Some(cell) = canvas.get(Position::new(x, y)) else {
                continue;
            };
            if cell.is_wide_tail() {
                continue;
            }
            let mut style = if cell.is_empty() {
                CellStyle::DEFAULT
            } else {
                cell.style
            };
            style.fg = reduce(style.fg, depth);
            style.bg = reduce(style.bg, depth);
            let glyph = cell.as_glyph().map_or(" ", Grapheme::as_str);
            // Unstyled trailing blanks are dropped; inner ones are kept.
            if glyph == " " && style == CellStyle::DEFAULT {
                pending_spaces += 1;
                continue;
            }
            if let Some(sgr) =
                sgr_delta(&current, &CellStyle::DEFAULT).filter(|_| pending_spaces > 0)
            {
                let _ = write!(line, "\x1b[{sgr}m");
                current = CellStyle::DEFAULT;
            }
            line.push_str(&" ".repeat(pending_spaces));
            pending_spaces = 0;
            if let Some(sgr) = sgr_delta(&current, &style) {
                let _ = write!(line, "\x1b[{sgr}m");
                current = style;
            }
            line.push_str(glyph);
        }
        if current != CellStyle::DEFAULT {
            line.push_str("\x1b[0m");
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Applies SGR parameters to a style.
fn apply_sgr(style: &mut CellStyle, params: &[u16]) {
    let mut i = 0;
    let params = if params.is_empty() { &[0][..] } else { params };
    while i < params.len() {
        let p = params[i];
        let set = |a: &mut Attributes, f: fn(&mut Attributes) -> &mut bool, v: bool| *f(a) = v;
        match p {
            0 => *style = CellStyle::DEFAULT,
            1 => set(&mut style.attrs, |a| &mut a.bold, true),
            2 => set(&mut style.attrs, |a| &mut a.dim, true),
            3 => set(&mut style.attrs, |a| &mut a.italic, true),
            4 => set(&mut style.attrs, |a| &mut a.underline, true),
            5 => set(&mut style.attrs, |a| &mut a.blink, true),
            7 => set(&mut style.attrs, |a| &mut a.reverse, true),
            9 => set(&mut style.attrs, |a| &mut a.strikethrough, true),
            22 => {
                style.attrs.bold = false;
                style.attrs.dim = false;
            }
            23 => style.attrs.italic = false,
            24 => style.attrs.underline = false,
            27 => style.attrs.reverse = false,
            29 => style.attrs.strikethrough = false,
            30..=37 => style.fg = Color::Indexed((p - 30) as u8),
            39 => style.fg = Color::Default,
            40..=47 => style.bg = Color::Indexed((p - 40) as u8),
            49 => style.bg = Color::Default,
            90..=97 => style.fg = Color::Indexed((p - 90 + 8) as u8),
            100..=107 => style.bg = Color::Indexed((p - 100 + 8) as u8),
            38 | 48 => {
                let target = if p == 38 {
                    &mut style.fg
                } else {
                    &mut style.bg
                };
                match params.get(i + 1) {
                    Some(5) => {
                        if let Some(&n) = params.get(i + 2) {
                            *target = Color::Indexed(n.min(255) as u8);
                        }
                        i += 2;
                    }
                    Some(2) => {
                        if let (Some(&r), Some(&g), Some(&b)) =
                            (params.get(i + 2), params.get(i + 3), params.get(i + 4))
                        {
                            *target =
                                Color::Rgb(r.min(255) as u8, g.min(255) as u8, b.min(255) as u8);
                        }
                        i += 4;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// Reads text (plain or with SGR colour escapes) into a canvas of `size`
/// (or as large as the text when `None`). Other escapes are skipped,
/// tabs expand to eight columns, spaces stay transparent unless styled.
pub fn from_ansi(text: &str, size: Option<Size>) -> Canvas {
    let mut rows: Vec<Vec<(Grapheme, CellStyle)>> = vec![Vec::new()];
    let mut style = CellStyle::DEFAULT;
    let mut chars = text.chars().peekable();
    let mut buf = String::new();
    let flush = |buf: &mut String, rows: &mut Vec<Vec<(Grapheme, CellStyle)>>, style: CellStyle| {
        for g in Grapheme::split_text(buf) {
            let row = rows
                .last_mut()
                .unwrap_or_else(|| unreachable!("rows start non-empty"));
            let w = g.width();
            row.push((g, style));
            if w == 2 {
                row.push((Grapheme::space(), CellStyle::DEFAULT));
            }
        }
        buf.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                flush(&mut buf, &mut rows, style);
                if chars.peek() == Some(&'[') {
                    chars.next();
                    let mut seq = String::new();
                    for d in chars.by_ref() {
                        if d.is_ascii_alphabetic() || d == '@' || d == '~' {
                            if d == 'm' {
                                let params: Vec<u16> = seq
                                    .split([';', ':'])
                                    .map(|p| p.parse().unwrap_or(0))
                                    .collect();
                                apply_sgr(&mut style, if seq.is_empty() { &[] } else { &params });
                            }
                            break;
                        }
                        seq.push(d);
                    }
                } else if chars.peek() == Some(&']') {
                    // OSC: skip to BEL or ST.
                    for d in chars.by_ref() {
                        if d == '\x07' || d == '\\' {
                            break;
                        }
                    }
                }
            }
            '\n' => {
                flush(&mut buf, &mut rows, style);
                rows.push(Vec::new());
            }
            '\r' => {}
            '\t' => {
                flush(&mut buf, &mut rows, style);
                let row = rows
                    .last_mut()
                    .unwrap_or_else(|| unreachable!("rows start non-empty"));
                let n = 8 - row.len() % 8;
                for _ in 0..n {
                    row.push((Grapheme::space(), style));
                }
            }
            c => buf.push(c),
        }
    }
    flush(&mut buf, &mut rows, style);
    while rows.last().is_some_and(Vec::is_empty) && rows.len() > 1 {
        rows.pop();
    }
    let natural = Size::new(
        rows.iter()
            .map(Vec::len)
            .max()
            .unwrap_or(1)
            .clamp(1, usize::from(u16::MAX)) as u16,
        rows.len().clamp(1, usize::from(u16::MAX)) as u16,
    );
    let size = size.unwrap_or(natural);
    let Ok(mut canvas) = Canvas::new(size) else {
        return Canvas::new(Size::new(1, 1)).unwrap_or_else(|_| unreachable!("1×1 is valid"));
    };
    for (y, row) in rows.iter().enumerate() {
        let mut x = 0usize;
        while x < row.len() {
            let (g, s) = &row[x];
            let w = usize::from(g.width());
            let blank = g.as_str() == " " && *s == CellStyle::DEFAULT;
            if !blank && x < usize::from(size.width) && y < usize::from(size.height) {
                let _ = canvas.put(
                    Position::new(x as u16, y as u16),
                    Cell::glyph(g.clone(), *s),
                );
            }
            x += w.max(1);
        }
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(g: &str, fg: Color) -> Cell {
        Cell::glyph(
            Grapheme::new(g).unwrap(),
            CellStyle::new(fg, Color::Default),
        )
    }

    #[test]
    fn export_uses_minimal_deltas_and_resets() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        c.put(Position::new(0, 0), cell("a", Color::Indexed(1)))
            .unwrap();
        c.put(Position::new(1, 0), cell("b", Color::Indexed(1)))
            .unwrap();
        c.put(Position::new(2, 0), cell("c", Color::Rgb(0, 0, 255)))
            .unwrap();
        let out = to_ansi(&c, Depth::TrueColor);
        assert_eq!(out, "\x1b[31mab\x1b[38;2;0;0;255mc\x1b[0m\n");
        let out16 = to_ansi(&c, Depth::Ansi16);
        assert_eq!(out16, "\x1b[31mab\x1b[34mc\x1b[0m\n");
        assert_eq!(
            reduce(Color::Rgb(250, 250, 250), Depth::Ansi16),
            Color::Indexed(15)
        );
    }

    #[test]
    fn import_reads_sgr_and_round_trips() {
        let text = "\x1b[1;32mok\x1b[0m fine\n\x1b[38;5;208m漢\x1b[0m";
        let canvas = from_ansi(text, None);
        assert_eq!(canvas.size(), Size::new(7, 2));
        let ok = canvas.get(Position::new(0, 0)).unwrap();
        assert_eq!(ok.style.fg, Color::Indexed(2));
        assert!(ok.style.attrs.bold);
        assert!(
            canvas.get(Position::new(2, 0)).unwrap().is_empty(),
            "plain space is transparent"
        );
        let han = canvas.get(Position::new(0, 1)).unwrap();
        assert_eq!(han.style.fg, Color::Indexed(208));
        // Export and import again: same cells.
        let again = from_ansi(&to_ansi(&canvas, Depth::TrueColor), Some(canvas.size()));
        assert_eq!(again, canvas);
    }
}
