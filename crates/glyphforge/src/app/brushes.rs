//! The brush catalogue: semantic brushes (auto-joining lines, patterns,
//! button and UI stamps) and plain glyphs, grouped into palette pages.

use glyphforge_core::Color;
use glyphforge_core::boxdraw::{BorderFamily, Weight};
use glyphforge_core::paint::{LineStyle, Pattern};
use glyphforge_core::raster::{Dither, GradientShape, GradientStyle, PaletteLimit};
use glyphforge_core::stamp::{ButtonStyle, StampKind};

use super::App;
use crate::actions::{Action, PixelAction};

/// What the brush paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrushKind {
    /// The brush glyph, as is.
    Glyph,
    /// Box-drawing lines that join with each other and with existing lines.
    Line(LineStyle),
    /// The brush glyph through a pattern (or the shade ramp).
    Pattern(Pattern),
    /// A UI element; the label is the brush label.
    Stamp(StampKind),
}

/// Something a palette entry selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrushChoice {
    Glyph(&'static str),
    Kind(BrushKind),
}

/// A named semantic brush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// What the palette shows.
    pub sample: &'static str,
    /// Searchable name.
    pub name: &'static str,
    pub kind: BrushKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageItems {
    /// Space-separated glyphs.
    Glyphs(&'static str),
    Presets(&'static [Preset]),
}

/// A palette page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrushPage {
    /// Short name for the palette's page line (at most 10 columns).
    pub name: &'static str,
    /// Searchable description.
    pub title: &'static str,
    pub items: PageItems,
}

impl BrushPage {
    /// The entries of the page: what to show and what it selects.
    pub fn entries(self) -> Vec<(&'static str, BrushChoice)> {
        match self.items {
            PageItems::Glyphs(glyphs) => glyphs
                .split(' ')
                .map(|g| (g, BrushChoice::Glyph(g)))
                .collect(),
            PageItems::Presets(presets) => presets
                .iter()
                .map(|p| (p.sample, BrushChoice::Kind(p.kind)))
                .collect(),
        }
    }
}

const fn line(weight: Weight, rounded: bool) -> BrushKind {
    BrushKind::Line(LineStyle { weight, rounded })
}

const fn button(style: ButtonStyle) -> BrushKind {
    BrushKind::Stamp(StampKind::Button(style))
}

const LINE_PRESETS: &[Preset] = &[
    Preset {
        sample: "┼ single",
        name: "single line",
        kind: line(Weight::Light, false),
    },
    Preset {
        sample: "╭ rounded",
        name: "rounded line",
        kind: line(Weight::Light, true),
    },
    Preset {
        sample: "╋ heavy",
        name: "heavy line",
        kind: line(Weight::Heavy, false),
    },
    Preset {
        sample: "╬ double",
        name: "double line",
        kind: line(Weight::Double, false),
    },
];

const PATTERN_PRESETS: &[Preset] = &[
    Preset {
        sample: "▒ shade",
        name: "shade ramp",
        kind: BrushKind::Pattern(Pattern::Shade),
    },
    Preset {
        sample: "▚ checker",
        name: "checker pattern",
        kind: BrushKind::Pattern(Pattern::Checker),
    },
    Preset {
        sample: "▤ rows",
        name: "row stripes",
        kind: BrushKind::Pattern(Pattern::Rows),
    },
    Preset {
        sample: "▥ cols",
        name: "column stripes",
        kind: BrushKind::Pattern(Pattern::Columns),
    },
    Preset {
        sample: "∷ dots",
        name: "dot grid",
        kind: BrushKind::Pattern(Pattern::Dots),
    },
    Preset {
        sample: "▰ gradient",
        name: "colour gradient",
        kind: BrushKind::Pattern(Pattern::Gradient),
    },
];

const BUTTON_PRESETS: &[Preset] = &[
    Preset {
        sample: "[OK]",
        name: "bracket button",
        kind: button(ButtonStyle::Bracket),
    },
    Preset {
        sample: "<OK>",
        name: "angle button",
        kind: button(ButtonStyle::Angle),
    },
    Preset {
        sample: "«OK»",
        name: "guillemet button",
        kind: button(ButtonStyle::Guillemet),
    },
    Preset {
        sample: "▕OK▏",
        name: "keycap button",
        kind: button(ButtonStyle::Keycap),
    },
    Preset {
        sample: "▐OK▌",
        name: "pill button",
        kind: button(ButtonStyle::Pill),
    },
    Preset {
        sample: "█OK█",
        name: "solid button",
        kind: button(ButtonStyle::Solid),
    },
    Preset {
        sample: "OK▄",
        name: "shadow button",
        kind: button(ButtonStyle::Shadow),
    },
    Preset {
        sample: "_OK_",
        name: "link button",
        kind: button(ButtonStyle::Link),
    },
    Preset {
        sample: "┌OK┐",
        name: "single boxed button",
        kind: button(ButtonStyle::Single),
    },
    Preset {
        sample: "╭OK╮",
        name: "rounded boxed button",
        kind: button(ButtonStyle::Rounded),
    },
    Preset {
        sample: "╔OK╗",
        name: "double boxed button",
        kind: button(ButtonStyle::Double),
    },
    Preset {
        sample: "┏OK┓",
        name: "heavy boxed button",
        kind: button(ButtonStyle::Heavy),
    },
];

const UI_PRESETS: &[Preset] = &[
    Preset {
        sample: "☑",
        name: "checkbox checked",
        kind: BrushKind::Stamp(StampKind::Checkbox(true)),
    },
    Preset {
        sample: "☐",
        name: "checkbox unchecked",
        kind: BrushKind::Stamp(StampKind::Checkbox(false)),
    },
    Preset {
        sample: "◉",
        name: "radio selected",
        kind: BrushKind::Stamp(StampKind::Radio(true)),
    },
    Preset {
        sample: "○",
        name: "radio unselected",
        kind: BrushKind::Stamp(StampKind::Radio(false)),
    },
    Preset {
        sample: "━━●",
        name: "toggle on",
        kind: BrushKind::Stamp(StampKind::Toggle(true)),
    },
    Preset {
        sample: "●━━",
        name: "toggle off",
        kind: BrushKind::Stamp(StampKind::Toggle(false)),
    },
    Preset {
        sample: "██░ bar",
        name: "progress bar",
        kind: BrushKind::Stamp(StampKind::Progress),
    },
    Preset {
        sample: "▲▼ scroll",
        name: "scrollbar",
        kind: BrushKind::Stamp(StampKind::Scrollbar),
    },
    Preset {
        sample: "q key",
        name: "key hint",
        kind: BrushKind::Stamp(StampKind::KeyHint),
    },
    Preset {
        sample: "╭tab╮",
        name: "tab",
        kind: BrushKind::Stamp(StampKind::Tab),
    },
    Preset {
        sample: "┤rule├",
        name: "titled rule",
        kind: BrushKind::Stamp(StampKind::Rule),
    },
    Preset {
        sample: "⠋",
        name: "spinner",
        kind: BrushKind::Stamp(StampKind::Spinner),
    },
];

/// The palette pages: semantic brushes first, then glyphs.
pub const BRUSH_PAGES: &[BrushPage] = &[
    BrushPage {
        name: "Lines ┼",
        title: "auto-joining line brushes",
        items: PageItems::Presets(LINE_PRESETS),
    },
    BrushPage {
        name: "Patterns",
        title: "pattern brushes",
        items: PageItems::Presets(PATTERN_PRESETS),
    },
    BrushPage {
        name: "Buttons",
        title: "button stamps",
        items: PageItems::Presets(BUTTON_PRESETS),
    },
    BrushPage {
        name: "UI stamps",
        title: "ui element stamps",
        items: PageItems::Presets(UI_PRESETS),
    },
    BrushPage {
        name: "Blocks",
        title: "blocks and eighths",
        items: PageItems::Glyphs("█ ▇ ▆ ▅ ▄ ▃ ▂ ▁ ▔ ▀ ▉ ▊ ▋ ▌ ▍ ▎ ▏ ▐ ▕ ░ ▒ ▓"),
    },
    BrushPage {
        name: "Quadrants",
        title: "quadrant blocks",
        items: PageItems::Glyphs("▘ ▝ ▖ ▗ ▀ ▄ ▌ ▐ ▚ ▞ ▛ ▜ ▙ ▟ █"),
    },
    BrushPage {
        name: "Sextants 1",
        title: "sextant blocks",
        items: PageItems::Glyphs("🬀 🬁 🬂 🬃 🬄 🬅 🬆 🬇 🬈 🬉 🬊 🬋 🬌 🬍 🬎 🬏 🬐 🬑 🬒 🬓 🬔 🬕 🬖 🬗 🬘 🬙 🬚 🬛 🬜 🬝"),
    },
    BrushPage {
        name: "Sextants 2",
        title: "sextant blocks",
        items: PageItems::Glyphs("🬞 🬟 🬠 🬡 🬢 🬣 🬤 🬥 🬦 🬧 🬨 🬩 🬪 🬫 🬬 🬭 🬮 🬯 🬰 🬱 🬲 🬳 🬴 🬵 🬶 🬷 🬸 🬹 🬺 🬻"),
    },
    BrushPage {
        name: "Light ─",
        title: "light box lines",
        items: PageItems::Glyphs("─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼ ╭ ╮ ╰ ╯ ╴ ╵ ╶ ╷ ┄ ┆ ┈ ┊ ╌ ╎ ╱ ╲ ╳"),
    },
    BrushPage {
        name: "Heavy ━",
        title: "heavy box lines",
        items: PageItems::Glyphs("━ ┃ ┏ ┓ ┗ ┛ ┣ ┫ ┳ ┻ ╋ ╸ ╹ ╺ ╻ ┅ ┇ ┉ ┋ ╍ ╏"),
    },
    BrushPage {
        name: "Double ═",
        title: "double box lines",
        items: PageItems::Glyphs("═ ║ ╔ ╗ ╚ ╝ ╠ ╣ ╦ ╩ ╬"),
    },
    BrushPage {
        name: "Mix ─═",
        title: "single and double box junctions",
        items: PageItems::Glyphs("╒ ╕ ╘ ╛ ╞ ╡ ╤ ╧ ╪ ╓ ╖ ╙ ╜ ╟ ╢ ╥ ╨ ╫"),
    },
    BrushPage {
        name: "Mix ─━ 1",
        title: "light and heavy box junctions",
        items: PageItems::Glyphs("┍ ┎ ┑ ┒ ┕ ┖ ┙ ┚ ┝ ┞ ┟ ┠ ┡ ┢ ┥ ┦ ┧ ┨ ┩ ┪ ┭ ┮ ┯ ┰ ┱ ┲ ┵ ┶ ┷ ┸"),
    },
    BrushPage {
        name: "Mix ─━ 2",
        title: "light and heavy box junctions",
        items: PageItems::Glyphs("┹ ┺ ┽ ┾ ┿ ╀ ╁ ╂ ╃ ╄ ╅ ╆ ╇ ╈ ╉ ╊ ╼ ╽ ╾ ╿"),
    },
    BrushPage {
        name: "Triangles",
        title: "triangles and diagonals",
        items: PageItems::Glyphs("◢ ◣ ◤ ◥ ▲ △ ▼ ▽ ◀ ◁ ▶ ▷ ► ◄ ▴ ▵ ▾ ▿ ◂ ◃ ▸ ▹ ◸ ◹ ◺ ◿ ⊿ ╱ ╲ ╳"),
    },
    BrushPage {
        name: "Shapes",
        title: "geometric shapes",
        items: PageItems::Glyphs("■ □ ▢ ▣ ▤ ▥ ▦ ▧ ▨ ▩ ▪ ▫ ▬ ▭ ▮ ▯ ● ○ ◉ ◎ ◌ ◍ ◐ ◑ ◒ ◓ ◆ ◇ ◈ ◊"),
    },
    BrushPage {
        name: "Symbols",
        title: "stars bullets and suits",
        items: PageItems::Glyphs("★ ☆ ✦ ✧ ✱ ✲ ✳ ✴ ✶ ✷ ✸ ✹ ❖ • ◦ ∙ · ‣ ⁃ ♥ ♡ ♦ ♢ ♣ ♧ ♠ ♤ ♪ ♫ ☼"),
    },
    BrushPage {
        name: "UI",
        title: "interface symbols",
        items: PageItems::Glyphs("✓ ✔ ✗ ✘ ☐ ☑ ☒ ⚠ ⓘ ⏻ ⌘ ⌥ ⇧ ⌃ ⏎ ⌫ ⌦ ⎋ ⇥ ⇤ ⊞ ⋮ ⋯ … ⌂ ⚙ ⚑ ⧗ ⊕ ⊗"),
    },
    BrushPage {
        name: "Arrows",
        title: "arrows",
        items: PageItems::Glyphs("← ↑ → ↓ ↔ ↕ ↖ ↗ ↘ ↙ ⇐ ⇑ ⇒ ⇓ ⇔ ⇕ ↩ ↪ ↺ ↻ ⟲ ⟳ ➜ ➤ ⮜ ⮞ ⮝ ⮟ ⤴ ⤵"),
    },
    BrushPage {
        name: "Braille",
        title: "braille dots",
        items: PageItems::Glyphs("⠁ ⠂ ⠄ ⡀ ⠈ ⠐ ⠠ ⢀ ⠉ ⠒ ⠤ ⣀ ⠇ ⠸ ⡇ ⢸ ⠛ ⠶ ⣤ ⠿ ⣶ ⣛ ⡏ ⢹ ⣇ ⣸ ⣾ ⣷ ⣿ ⢿"),
    },
    BrushPage {
        name: "ASCII",
        title: "ascii",
        items: PageItems::Glyphs("# * + - = ~ ^ @ % & $ / \\ | _ < > ( ) [ ] { } . , : ; ' \" `"),
    },
    BrushPage {
        name: "Math",
        title: "math",
        items: PageItems::Glyphs("± × ÷ ≈ ≠ ≡ ≤ ≥ ∞ ∑ ∏ √ ∫ ∂ ∆ ∇ ∈ ∉ ∩ ∪ ⊂ ⊃ ∧ ∨ ¬ ° µ π Ω λ"),
    },
];

/// Every preset, for lookups by kind.
pub fn presets() -> impl Iterator<Item = &'static Preset> {
    BRUSH_PAGES.iter().flat_map(|page| match page.items {
        PageItems::Presets(p) => p.iter(),
        PageItems::Glyphs(_) => [].iter(),
    })
}

/// The preset for a brush kind, if it has one.
pub fn preset_of(kind: BrushKind) -> Option<&'static Preset> {
    presets().find(|p| p.kind == kind)
}

/// Box styles offered for the Box tool, in palette order.
pub const BOX_STYLES: [BorderFamily; 5] = [
    BorderFamily::Single,
    BorderFamily::Rounded,
    BorderFamily::Double,
    BorderFamily::Heavy,
    BorderFamily::Ascii,
];

/// The joining line style that draws a box family, or `None` for families
/// without box-drawing topology (ASCII).
pub const fn line_style_of(family: BorderFamily) -> Option<LineStyle> {
    let (weight, rounded) = match family {
        BorderFamily::Single => (Weight::Light, false),
        BorderFamily::Rounded => (Weight::Light, true),
        BorderFamily::Double => (Weight::Double, false),
        BorderFamily::Heavy => (Weight::Heavy, false),
        BorderFamily::Ascii | BorderFamily::Minimal | BorderFamily::None => return None,
    };
    Some(LineStyle { weight, rounded })
}

/// Search candidates for the brush prompt, with what each selects.
pub fn search_candidates() -> Vec<(String, BrushChoice)> {
    BRUSH_PAGES
        .iter()
        .flat_map(|page| match page.items {
            PageItems::Glyphs(glyphs) => glyphs
                .split(' ')
                .map(|g| (format!("{g}  {}", page.title), BrushChoice::Glyph(g)))
                .collect::<Vec<_>>(),
            PageItems::Presets(presets) => presets
                .iter()
                .map(|p| {
                    (
                        format!("{}  {}", p.sample, p.name),
                        BrushChoice::Kind(p.kind),
                    )
                })
                .collect(),
        })
        .collect()
}

/// The page that holds `choice`.
pub fn page_of(choice: BrushChoice) -> Option<usize> {
    BRUSH_PAGES
        .iter()
        .position(|page| page.entries().iter().any(|(_, c)| *c == choice))
}

/// An entry of the palette's brushes block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub label: String,
    pub action: Action,
    pub active: bool,
    /// Background colours for the first characters of the label (gradient
    /// previews).
    pub swatch: Vec<Color>,
}

impl PaletteEntry {
    fn new(label: impl Into<String>, action: Action, active: bool) -> Self {
        Self {
            label: label.into(),
            action,
            active,
            swatch: Vec::new(),
        }
    }
}

/// A page of the palette's brushes block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PalettePage {
    pub name: String,
    pub entries: Vec<PaletteEntry>,
}

/// Gradients listed per palette page.
const GRADIENTS_PER_PAGE: usize = 5;

impl App {
    /// The pages of the brushes block for the active layer: brushes and
    /// glyphs on artwork layers, pixel settings and gradients on pixel
    /// layers.
    pub fn palette_pages(&self) -> Vec<PalettePage> {
        if self.on_raster_layer() {
            return self.pixel_pages();
        }
        BRUSH_PAGES
            .iter()
            .map(|page| PalettePage {
                name: page.name.to_owned(),
                entries: page
                    .entries()
                    .into_iter()
                    .map(|(sample, choice)| {
                        let (action, active) = match choice {
                            BrushChoice::Glyph(g) => (
                                Action::SetBrushGlyph(g.to_owned()),
                                self.paint.kind == BrushKind::Glyph
                                    && self.paint.brush.as_str() == g,
                            ),
                            BrushChoice::Kind(kind) => {
                                (Action::SetBrushKind(kind), self.paint.kind == kind)
                            }
                        };
                        PaletteEntry::new(sample, action, active)
                    })
                    .collect(),
            })
            .collect()
    }

    /// Index of the shown page, clamped.
    pub fn palette_page(&self) -> usize {
        let count = self.palette_pages().len().max(1);
        let page = if self.on_raster_layer() {
            self.paint.pixel_page
        } else {
            self.paint.page
        };
        page.min(count - 1)
    }

    fn pixel_pages(&self) -> Vec<PalettePage> {
        let Some(raster) = self.raster() else {
            return Vec::new();
        };
        let px = |a: PixelAction| Action::Pixel(a);
        let dither = |label: &str, d: Dither| {
            PaletteEntry::new(label, px(PixelAction::SetDither(d)), raster.dither == d)
        };
        let limit = |label: &str, p: PaletteLimit| {
            PaletteEntry::new(label, px(PixelAction::SetPalette(p)), raster.palette == p)
        };
        let shadow = raster.shadow;
        let on = shadow.is_some();
        let soft = |n: u8| {
            PaletteEntry::new(
                if n == 0 {
                    "blur 0".to_owned()
                } else {
                    n.to_string()
                },
                px(PixelAction::ShadowSoftness(n)),
                shadow.is_some_and(|s| s.softness == n),
            )
        };
        let opacity = |n: u8| {
            PaletteEntry::new(
                format!("{n}%"),
                px(PixelAction::ShadowOpacity(n)),
                shadow.is_some_and(|s| s.opacity == n),
            )
        };
        let style = |label: &str, s: GradientStyle| {
            let active =
                std::mem::discriminant(&self.paint.gradient_style) == std::mem::discriminant(&s);
            PaletteEntry::new(label, px(PixelAction::GradientStyle(s)), active)
        };
        let shape = |label: &str, s: GradientShape| {
            PaletteEntry::new(
                label,
                px(PixelAction::GradientShape(s)),
                self.paint.gradient_shape == s,
            )
        };
        let mut pages = vec![
            PalettePage {
                name: "Dither".to_owned(),
                entries: vec![
                    dither("· none", Dither::None),
                    dither("▦ ordered", Dither::Ordered),
                    dither("░ shade", Dither::Shade),
                    dither("▦░ both", Dither::OrderedShade),
                    limit("■ true", PaletteLimit::Truecolor),
                    limit("256", PaletteLimit::Ansi256),
                    limit("16", PaletteLimit::Ansi16),
                ],
            },
            PalettePage {
                name: "Shadow".to_owned(),
                entries: vec![
                    PaletteEntry::new(
                        if on { "☑ shadow" } else { "☐ shadow" },
                        px(PixelAction::ToggleShadow),
                        on,
                    ),
                    PaletteEntry::new("↘ +1", px(PixelAction::ShadowOffset(1)), false),
                    PaletteEntry::new("↖ -1", px(PixelAction::ShadowOffset(-1)), false),
                    soft(0),
                    soft(1),
                    soft(2),
                    soft(3),
                    opacity(30),
                    opacity(60),
                    opacity(100),
                    PaletteEntry {
                        swatch: vec![shadow.map_or(Color::BLACK, |s| s.color)],
                        ..PaletteEntry::new("  = fg", px(PixelAction::ShadowColorFromBrush), false)
                    },
                ],
            },
            PalettePage {
                name: "Gradient".to_owned(),
                entries: vec![
                    style("━ smooth", GradientStyle::Smooth),
                    style("▦ dither", GradientStyle::Dither),
                    style("▤ hard", GradientStyle::Hard(5)),
                    style("░ shade", GradientStyle::Shade),
                    shape("→ linear", GradientShape::Linear),
                    shape("◎ radial", GradientShape::Radial),
                ],
            },
        ];
        let current = self.current_gradient().map(|g| g.id);
        let gradients = self.gradients();
        let chunks = gradients.chunks(GRADIENTS_PER_PAGE);
        let count = chunks.len();
        for (i, chunk) in chunks.enumerate() {
            pages.push(PalettePage {
                name: if count > 1 {
                    format!("Gradients {}", i + 1)
                } else {
                    "Gradients".to_owned()
                },
                entries: chunk
                    .iter()
                    .map(|g| {
                        let name: String = g.name.chars().take(6).collect();
                        PaletteEntry {
                            swatch: (0..3u8).map(|k| g.sample(f32::from(k) / 2.0)).collect(),
                            ..PaletteEntry::new(
                                format!("   {name}"),
                                px(PixelAction::SelectGradient(g.id.clone())),
                                current.as_ref() == Some(&g.id),
                            )
                        }
                    })
                    .chain(std::iter::once(PaletteEntry::new(
                        "+ edit",
                        px(PixelAction::EditGradient),
                        false,
                    )))
                    .collect(),
            });
        }
        pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyphforge_core::Grapheme;

    #[test]
    fn pages_are_valid_and_cover_box_drawing_and_blocks() {
        let mut all = std::collections::HashSet::new();
        for page in BRUSH_PAGES {
            assert!(
                page.name.chars().count() <= 10,
                "{} name too long",
                page.name
            );
            for (sample, choice) in page.entries() {
                for g in Grapheme::split_text(sample) {
                    assert_eq!(g.width(), 1, "{sample} on {}", page.name);
                }
                if let BrushChoice::Glyph(g) = choice {
                    assert_eq!(Grapheme::new(g).unwrap().width(), 1, "{g}");
                    all.insert(g.chars().next().unwrap());
                }
            }
        }
        for c in (0x2500..=0x259F).filter_map(char::from_u32) {
            assert!(all.contains(&c), "U+{:04X} {c} missing", c as u32);
        }
    }

    #[test]
    fn presets_are_unique_and_found() {
        let kinds: Vec<BrushKind> = presets().map(|p| p.kind).collect();
        let unique: std::collections::HashSet<_> = kinds.iter().collect();
        assert_eq!(kinds.len(), unique.len());
        assert_eq!(kinds.len(), 34);
        let double = line(Weight::Double, false);
        assert_eq!(preset_of(double).map(|p| p.name), Some("double line"));
        assert_eq!(page_of(BrushChoice::Kind(double)), Some(0));
        assert_eq!(
            line_style_of(BorderFamily::Rounded),
            Some(LineStyle {
                weight: Weight::Light,
                rounded: true
            })
        );
        assert_eq!(line_style_of(BorderFamily::Ascii), None);
        assert!(
            search_candidates()
                .iter()
                .any(|(s, _)| s == "╬ double  double line")
        );
    }
}
