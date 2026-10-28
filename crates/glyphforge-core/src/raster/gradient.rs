//! Gradients with colour stops, stored in the document, and gradient fills
//! rendered as smooth colours, hard bands, sub-cell dithering or shade
//! glyphs.

use serde::{Deserialize, Serialize};

use super::{Pixel, RasterMode, ShadeLevel, bayer};
use crate::document::{Color, Position};
use crate::id::ObjectId;

/// A colour at a position along the gradient, 0..=100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GradientStop {
    pub at: u8,
    pub color: Color,
}

/// A named, reusable gradient.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Gradient {
    pub id: ObjectId,
    pub name: String,
    pub stops: Vec<GradientStop>,
}

/// Errors from the textual stop list.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GradientError {
    #[error("a gradient needs at least two stops")]
    TooFewStops,
    #[error("invalid stop {0:?}; expected \"COLOUR [POSITION]\"")]
    BadStop(String),
}

impl Gradient {
    /// A gradient with stops sorted by position and clamped to 0..=100.
    pub fn new(
        id: ObjectId,
        name: impl Into<String>,
        mut stops: Vec<GradientStop>,
    ) -> Result<Self, GradientError> {
        if stops.len() < 2 {
            return Err(GradientError::TooFewStops);
        }
        for s in &mut stops {
            s.at = s.at.min(100);
        }
        stops.sort_by_key(|s| s.at);
        Ok(Self {
            id,
            name: name.into(),
            stops,
        })
    }

    /// Parses `"#ff5f6d 0, #ffc371 100"`. Stops without a position are
    /// spread evenly.
    pub fn parse_stops(
        text: &str,
        parse_color: impl Fn(&str) -> Option<Color>,
    ) -> Result<Vec<GradientStop>, GradientError> {
        let parts: Vec<&str> = text
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 2 {
            return Err(GradientError::TooFewStops);
        }
        let last = parts.len() - 1;
        parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let mut words = part.split_whitespace();
                let bad = || GradientError::BadStop((*part).to_owned());
                let color = words.next().and_then(&parse_color).ok_or_else(bad)?;
                let at = match words.next() {
                    Some(w) => w.trim_end_matches('%').parse::<u8>().map_err(|_| bad())?,
                    None => (i * 100 / last) as u8,
                };
                if words.next().is_some() {
                    return Err(bad());
                }
                Ok(GradientStop { at, color })
            })
            .collect()
    }

    /// The stops as text, the inverse of [`Gradient::parse_stops`].
    pub fn stops_text(&self) -> String {
        self.stops
            .iter()
            .map(|s| format!("{} {}", s.color, s.at))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The two stops around `t` (0..=1) and the position between them.
    fn segment(&self, t: f32) -> (Color, Color, f32) {
        let pos = t.clamp(0.0, 1.0) * 100.0;
        let first = self.stops[0];
        let last = self.stops[self.stops.len() - 1];
        if pos <= f32::from(first.at) {
            return (first.color, first.color, 0.0);
        }
        for w in self.stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if pos <= f32::from(b.at) {
                let span = f32::from(b.at - a.at).max(f32::EPSILON);
                return (a.color, b.color, (pos - f32::from(a.at)) / span);
            }
        }
        (last.color, last.color, 0.0)
    }

    /// The colour at `t`, blended in RGB.
    pub fn sample(&self, t: f32) -> Color {
        let (a, b, u) = self.segment(t);
        mix(a, b, u)
    }
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    match (from.to_rgb(), to.to_rgb()) {
        (Some(start), Some(end)) => {
            Color::Rgb(start.0, start.1, start.2).mix(Color::Rgb(end.0, end.1, end.2), amount)
        }
        _ if amount < 0.5 => from,
        _ => to,
    }
}

/// Geometry of a gradient fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GradientShape {
    /// Along the line from the start to the end point.
    #[default]
    Linear,
    /// Outwards from the start point; the end point marks the outer edge.
    Radial,
}

/// How the gradient becomes pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GradientStyle {
    /// Every pixel gets its exact colour.
    #[default]
    Smooth,
    /// This many flat bands.
    Hard(u8),
    /// Only the stop colours, mixed by ordered dithering across sub-cell
    /// pixels.
    Dither,
    /// Stop colours mixed per cell with shade glyphs `░▒▓`.
    Shade,
}

/// Pixels of a gradient fill over `region`, from `from` to `to` (pixel
/// coordinates). Distances are measured in cell-proportional units so a
/// radial fill is round on screen although sub-cell pixels are not square.
pub fn fill(
    region: &[Position],
    mode: RasterMode,
    gradient: &Gradient,
    shape: GradientShape,
    style: GradientStyle,
    from: (f32, f32),
    to: (f32, f32),
) -> Vec<(Position, Pixel)> {
    let (cw, ch) = mode.cell_px();
    let (cw, ch) = (f32::from(cw), f32::from(ch));
    // One cell is 1 unit wide and 2 units high.
    let phys = |x: f32, y: f32| (x / cw, y * 2.0 / ch);
    let (fx, fy) = phys(from.0 + 0.5, from.1 + 0.5);
    let (tx, ty) = phys(to.0 + 0.5, to.1 + 0.5);
    let (dx, dy) = (tx - fx, ty - fy);
    let len2 = dx * dx + dy * dy;
    let t_at = |px: f32, py: f32| {
        let (x, y) = phys(px, py);
        if len2 <= f32::EPSILON {
            return 0.0;
        }
        match shape {
            GradientShape::Linear => ((x - fx) * dx + (y - fy) * dy) / len2,
            GradientShape::Radial => ((x - fx).powi(2) + (y - fy).powi(2)).sqrt() / len2.sqrt(),
        }
    };
    region
        .iter()
        .map(|&p| {
            let pixel_t = t_at(f32::from(p.x) + 0.5, f32::from(p.y) + 0.5);
            let pixel = match style {
                GradientStyle::Smooth => Pixel::Solid(gradient.sample(pixel_t)),
                GradientStyle::Hard(steps) => {
                    let steps = f32::from(steps.max(2));
                    let band = (pixel_t.clamp(0.0, 1.0) * steps).floor().min(steps - 1.0);
                    Pixel::Solid(gradient.sample(band / (steps - 1.0)))
                }
                GradientStyle::Dither => {
                    let (a, b, u) = gradient.segment(pixel_t);
                    Pixel::Solid(if u > bayer(p.x, p.y) { b } else { a })
                }
                GradientStyle::Shade => {
                    let (cx, cy) = ((f32::from(p.x) / cw).floor(), (f32::from(p.y) / ch).floor());
                    let cell_t = t_at((cx + 0.5) * cw, (cy + 0.5) * ch);
                    let (a, b, u) = gradient.segment(cell_t);
                    match (u * 4.0).round() as u8 {
                        0 => Pixel::Solid(a),
                        q @ 1..=3 => Pixel::Shade {
                            fg: b,
                            bg: Some(a),
                            level: ShadeLevel::from_quarters(q).unwrap_or(ShadeLevel::Medium),
                        },
                        _ => Pixel::Solid(b),
                    }
                }
            };
            (p, pixel)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Color = Color::Rgb(0, 0, 0);
    const B: Color = Color::Rgb(200, 100, 0);

    fn g() -> Gradient {
        Gradient::new(
            ObjectId::slugify("g"),
            "G",
            vec![
                GradientStop { at: 100, color: B },
                GradientStop { at: 0, color: A },
            ],
        )
        .unwrap()
    }

    fn row(width: u16) -> Vec<Position> {
        (0..width).map(|x| Position::new(x, 0)).collect()
    }

    #[test]
    fn stops_parse_sort_and_sample() {
        let parse = |s: &str| s.parse::<Color>().ok();
        let stops = Gradient::parse_stops("#000000, #ff0000 25%, #ffffff", parse).unwrap();
        assert_eq!(
            stops.iter().map(|s| s.at).collect::<Vec<_>>(),
            vec![0, 25, 100]
        );
        assert!(Gradient::parse_stops("#000000", parse).is_err());
        assert!(Gradient::parse_stops("#000000 0, nope 100", parse).is_err());
        let gradient = g();
        assert_eq!(gradient.stops[0].color, A, "sorted");
        assert_eq!(gradient.sample(0.5), Color::Rgb(100, 50, 0));
        assert_eq!(gradient.sample(-1.0), A);
        assert_eq!(gradient.stops_text(), "#000000 0, #c86400 100");
    }

    #[test]
    fn linear_styles() {
        let region = row(8);
        let fill_with = |style| {
            fill(
                &region,
                RasterMode::Quadrant,
                &g(),
                GradientShape::Linear,
                style,
                (0.0, 0.0),
                (7.0, 0.0),
            )
        };
        let smooth = fill_with(GradientStyle::Smooth);
        assert_eq!(smooth[0].1, Pixel::Solid(A));
        assert_eq!(smooth[7].1, Pixel::Solid(B));

        let hard = fill_with(GradientStyle::Hard(2));
        assert!(
            hard.iter()
                .all(|(_, p)| *p == Pixel::Solid(A) || *p == Pixel::Solid(B))
        );
        assert_eq!(hard[3].1, Pixel::Solid(A));
        assert_eq!(hard[4].1, Pixel::Solid(B));

        let dither = fill_with(GradientStyle::Dither);
        assert!(
            dither
                .iter()
                .all(|(_, p)| *p == Pixel::Solid(A) || *p == Pixel::Solid(B))
        );

        let shade = fill_with(GradientStyle::Shade);
        assert!(shade.iter().any(|(_, p)| matches!(p, Pixel::Shade { .. })));
        assert_eq!(shade[0].1, shade[1].1, "one value per cell");
    }

    #[test]
    fn radial_is_round_on_screen() {
        // Sextant pixels are 1/2 cell wide and 2/3 unit high.
        let region = vec![
            Position::new(4, 3),
            Position::new(8, 3),
            Position::new(4, 6),
        ];
        let out = fill(
            &region,
            RasterMode::Sextant,
            &g(),
            GradientShape::Radial,
            GradientStyle::Smooth,
            (4.0, 3.0),
            (8.0, 3.0),
        );
        assert_eq!(out[0].1, Pixel::Solid(A));
        assert_eq!(out[1].1, Pixel::Solid(B), "2 cells right = radius");
        assert_eq!(
            out[2].1,
            Pixel::Solid(B),
            "3 pixels down = 2 units = radius"
        );
    }
}
