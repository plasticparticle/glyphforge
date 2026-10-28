//! Feedback: modals, dialogs, notifications, toasts, tooltips, empty
//! states, badges, tags, key hints and spinners.

use crate::boxdraw::BorderFamily;
use crate::component::Component;
use crate::document::{Canvas, Cell, Rect, Size};
use crate::layout::Edges;
use crate::render::descriptor::{Category, Descriptor, PropKind, PropSpec};
use crate::render::kit;
use crate::render::{ComponentRenderer, RenderContext, painter};
use crate::value::Value;

/// A framed surface with a title and a drop shadow; `dialog` adds a
/// message and buttons.
#[derive(Debug)]
pub struct Modal {
    dialog: bool,
}

impl Modal {
    pub const MODAL: Self = Self { dialog: false };
    pub const DIALOG: Self = Self { dialog: true };
}

impl ComponentRenderer for Modal {
    fn kind(&self) -> &'static str {
        if self.dialog { "dialog" } else { "modal" }
    }

    fn descriptor(&self) -> Descriptor {
        let d = if self.dialog {
            Descriptor::new("dialog", "Dialog", Category::Feedback)
                .summary("Message with buttons in a framed window")
                .keywords(&["confirm", "alert", "prompt", "popup"])
                .size(Some((40, 7)))
                .prop(PropSpec::new("title", PropKind::Text, "caption").initial("Delete file?"))
                .prop(
                    PropSpec::new("message", PropKind::Text, "body text")
                        .initial("This cannot be undone."),
                )
                .prop(
                    PropSpec::new(
                        "buttons",
                        PropKind::List,
                        "button labels, the last is primary",
                    )
                    .initial(Value::List(vec!["Cancel".into(), "Delete".into()])),
                )
        } else {
            Descriptor::new("modal", "Modal", Category::Feedback)
                .summary("Framed window over the screen, a container")
                .keywords(&["window", "overlay", "popup", "sheet"])
                .container()
                .size(Some((40, 10)))
                .prop(PropSpec::new("title", PropKind::Text, "caption").initial("Settings"))
        };
        d.prop(PropSpec::new("shadow", PropKind::Bool, "drop shadow").default(true))
            .prop(PropSpec::new("border", PropKind::Border, "frame family").default("double"))
    }

    fn chrome_inset(&self, _: &Component, _: &RenderContext<'_>) -> Edges {
        Edges {
            top: 1,
            right: 3,
            bottom: 2,
            left: 2,
        }
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let shadow = c.prop_bool("shadow") != Some(false);
        let frame = if shadow && rect.width > 2 && rect.height > 2 {
            Rect::new(rect.x, rect.y, rect.width - 1, rect.height - 1)
        } else {
            rect
        };
        if shadow {
            let dark = Cell::blank(kit::style(ctx, "background", "background"));
            painter::fill(
                canvas,
                Rect::new(frame.x + 1, frame.y + frame.height, frame.width, 1),
                &dark,
            );
            painter::fill(
                canvas,
                Rect::new(frame.x + frame.width, frame.y + 1, 1, frame.height),
                &dark,
            );
        }
        let base = kit::style(ctx, "foreground", "surface");
        painter::fill(canvas, frame, &Cell::blank(base));
        let family = c
            .prop_str("border")
            .and_then(BorderFamily::parse)
            .unwrap_or(BorderFamily::Double);
        painter::draw_border(canvas, frame, family, kit::style(ctx, "border", "surface"));
        if let Some(title) = c.prop_str("title") {
            let t = format!(
                " {} ",
                painter::truncate(title, frame.width.saturating_sub(4))
            );
            kit::put(canvas, frame.x + 2, frame.y, &t, kit::bold(base), frame);
        }
        if !self.dialog {
            return;
        }
        if let Some(message) = c.prop_str("message") {
            for (i, line) in message.lines().enumerate() {
                kit::put(canvas, inner.x, inner.y + i as u16, line, base, inner);
            }
        }
        let buttons = kit::strings(c, ctx, "buttons");
        let y = frame.bottom().saturating_sub(2) as u16;
        let mut x = frame.right().saturating_sub(2) as u16;
        for (i, label) in buttons.iter().enumerate().rev() {
            let text = format!("[ {label} ]");
            x = x.saturating_sub(painter::text_width(&text) + 1);
            let style = if i + 1 == buttons.len() {
                kit::style(ctx, "selection-foreground", "primary")
            } else {
                kit::style(ctx, "foreground", "surface-alt")
            };
            kit::put(canvas, x, y, &text, style, frame);
        }
    }
}

/// A boxed message with a level icon: `notification`.
#[derive(Debug)]
pub struct Notification;

impl ComponentRenderer for Notification {
    fn kind(&self) -> &'static str {
        "notification"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("notification", "Notification", Category::Feedback)
            .summary("Boxed message with a level icon")
            .keywords(&["alert", "banner", "message", "callout"])
            .size(Some((40, 4)))
            .prop(PropSpec::new("title", PropKind::Text, "first line").initial("Backup finished"))
            .prop(PropSpec::new("message", PropKind::Text, "details").initial("12 files, 3.4 MB"))
            .prop(kit::level_prop().initial("success"))
    }

    fn chrome_inset(&self, _: &Component, _: &RenderContext<'_>) -> Edges {
        Edges {
            top: 1,
            right: 1,
            bottom: 1,
            left: 4,
        }
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        let w = [c.prop_str("title"), c.prop_str("message")]
            .iter()
            .flatten()
            .map(|t| painter::text_width(t))
            .max()
            .unwrap_or(10);
        Some(Size::new(w, 2))
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let level = kit::str_or(c, "level", "info");
        let token = kit::level_token(level);
        let base = kit::style(ctx, "foreground", "surface");
        painter::fill(canvas, rect, &Cell::blank(base));
        painter::draw_border(
            canvas,
            rect,
            ctx.theme.border,
            kit::style(ctx, token, "surface"),
        );
        kit::put(
            canvas,
            rect.x + 2,
            inner.y,
            kit::level_icon(level),
            kit::bold(kit::style(ctx, token, "surface")),
            rect,
        );
        if let Some(title) = c.prop_str("title") {
            kit::put(canvas, inner.x, inner.y, title, kit::bold(base), inner);
        }
        if let Some(message) = c.prop_str("message") {
            kit::put(
                canvas,
                inner.x,
                inner.y + 1,
                message,
                kit::style(ctx, "foreground-muted", "surface"),
                inner,
            );
        }
    }
}

/// One-line pill: `toast` (level colour), `badge` (count or word) and
/// `tag` (a label, optionally closable).
#[derive(Debug)]
pub struct Pill(&'static str);

impl Pill {
    pub const TOAST: Self = Self("toast");
    pub const BADGE: Self = Self("badge");
    pub const TAG: Self = Self("tag");

    fn text(&self, c: &Component) -> String {
        let t = kit::str_or(c, "text", "");
        match self.0 {
            "toast" => format!(" {} {t} ", kit::level_icon(kit::str_or(c, "level", "info"))),
            "tag" if c.prop_bool("closable") == Some(true) => format!(" {t} × "),
            _ => format!(" {t} "),
        }
    }
}

impl ComponentRenderer for Pill {
    fn kind(&self) -> &'static str {
        self.0
    }

    fn descriptor(&self) -> Descriptor {
        let d = match self.0 {
            "toast" => Descriptor::new("toast", "Toast", Category::Feedback)
                .summary("Short-lived one-line message")
                .keywords(&["snackbar", "flash", "message"])
                .prop(PropSpec::new("text", PropKind::Text, "the message").initial("Saved"))
                .prop(kit::level_prop().initial("success")),
            "badge" => Descriptor::new("badge", "Badge", Category::Feedback)
                .summary("Count or status word")
                .keywords(&["count", "counter", "pill", "status", "new"])
                .prop(PropSpec::new("text", PropKind::Text, "count or word").initial("3"))
                .prop(kit::level_prop().initial("error")),
            _ => Descriptor::new("tag", "Tag", Category::Feedback)
                .summary("Label chip")
                .keywords(&["chip", "label", "category", "topic"])
                .prop(PropSpec::new("text", PropKind::Text, "the label").initial("rust"))
                .prop(PropSpec::new("closable", PropKind::Bool, "show ×")),
        };
        d.size(None)
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        Some(Size::new(painter::text_width(&self.text(c)), 1))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let style = match self.0 {
            "tag" => kit::style(ctx, "foreground", "surface-alt"),
            _ => kit::bold(kit::style(
                ctx,
                "selection-foreground",
                kit::level_token(kit::str_or(c, "level", "info")),
            )),
        };
        kit::put(canvas, inner.x, inner.y, &self.text(c), style, inner);
    }
}

/// Small hint box with an arrow pointing at its target.
#[derive(Debug)]
pub struct Tooltip;

impl ComponentRenderer for Tooltip {
    fn kind(&self) -> &'static str {
        "tooltip"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("tooltip", "Tooltip", Category::Feedback)
            .summary("Hint box with an arrow")
            .keywords(&["hint", "help", "popover", "hover"])
            .size(None)
            .prop(PropSpec::new("text", PropKind::Text, "the hint").initial("Copy to clipboard"))
            .prop(
                PropSpec::new(
                    "arrow",
                    PropKind::choice(&["up", "down", "none"]),
                    "where the arrow points",
                )
                .default("up"),
            )
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        let arrow = u16::from(kit::str_or(c, "arrow", "up") != "none");
        Some(Size::new(
            painter::text_width(kit::str_or(c, "text", "")) + 2,
            1 + arrow,
        ))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let arrow = kit::str_or(c, "arrow", "up");
        let style = kit::style(ctx, "background", "foreground");
        let (text_y, arrow_y, glyph) = match arrow {
            "down" => (inner.y, Some(inner.y + 1), "▼"),
            "none" => (inner.y, None, ""),
            _ => (inner.y + 1, Some(inner.y), "▲"),
        };
        kit::put(
            canvas,
            inner.x,
            text_y,
            &format!(" {} ", kit::str_or(c, "text", "")),
            style,
            inner,
        );
        if let Some(y) = arrow_y {
            kit::put(
                canvas,
                inner.x + 2,
                y,
                glyph,
                kit::style(ctx, "foreground", "surface"),
                inner,
            );
        }
    }
}

/// Centred placeholder for empty views: icon, title, hint.
#[derive(Debug)]
pub struct EmptyState;

impl ComponentRenderer for EmptyState {
    fn kind(&self) -> &'static str {
        "empty"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("empty", "Empty State", Category::Feedback)
            .summary("Centred icon, title and hint for empty views")
            .keywords(&["placeholder", "nothing", "blank", "zero"])
            .size(Some((36, 5)))
            .prop(PropSpec::new("icon", PropKind::Text, "glyph on top").default("∅"))
            .prop(PropSpec::new("title", PropKind::Text, "what is missing").initial("No results"))
            .prop(
                PropSpec::new("hint", PropKind::Text, "what to do")
                    .initial("Try a different filter"),
            )
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let lines = [
            (
                kit::str_or(c, "icon", "∅"),
                kit::style(ctx, "foreground-muted", "surface"),
            ),
            (
                kit::str_or(c, "title", ""),
                kit::bold(kit::style(ctx, "foreground", "surface")),
            ),
            (
                kit::str_or(c, "hint", ""),
                kit::style(ctx, "foreground-muted", "surface"),
            ),
        ];
        let top = inner.y + inner.height.saturating_sub(3) / 2;
        for (i, (text, style)) in lines.iter().enumerate() {
            let w = painter::text_width(text);
            let x = inner.x + inner.width.saturating_sub(w) / 2;
            kit::put(canvas, x, top + i as u16, text, *style, inner);
        }
    }
}

/// `q quit  ? help`: keys emphasised, actions muted.
#[derive(Debug)]
pub struct KeyHint;

impl ComponentRenderer for KeyHint {
    fn kind(&self) -> &'static str {
        "keyhint"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("keyhint", "Key Hints", Category::Feedback)
            .summary("Keys and what they do")
            .keywords(&["shortcuts", "help line", "legend", "keys"])
            .size(None)
            .prop(
                PropSpec::new("items", PropKind::List, "\"key|action\" entries").initial(
                    Value::List(vec!["q|quit".into(), "?|help".into(), "/|search".into()]),
                ),
            )
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let w: u16 = kit::strings(c, ctx, "items")
            .iter()
            .map(|i| painter::text_width(&i.replace('|', " ")) + 2)
            .sum();
        Some(Size::new(w, 1))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let mut x = inner.x;
        for item in kit::strings(c, ctx, "items") {
            let (key, action) = item.split_once('|').unwrap_or((&item, ""));
            x += kit::put(
                canvas,
                x,
                inner.y,
                key,
                kit::bold(kit::style(ctx, "primary", "surface")),
                inner,
            );
            x += kit::put(
                canvas,
                x,
                inner.y,
                &format!(" {action}  "),
                kit::style(ctx, "foreground-muted", "surface"),
                inner,
            );
        }
    }
}

const SPINNERS: &[(&str, &[&str])] = &[
    ("dots", &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    ("line", &["-", "\\", "|", "/"]),
    ("arc", &["◜", "◠", "◝", "◞", "◡", "◟"]),
    ("blocks", &["▖", "▘", "▝", "▗"]),
];

/// A spinner frame and a label.
#[derive(Debug)]
pub struct Spinner;

impl ComponentRenderer for Spinner {
    fn kind(&self) -> &'static str {
        "spinner"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("spinner", "Spinner", Category::Feedback)
            .summary("Activity indicator with a label")
            .keywords(&["loading", "busy", "progress", "wait"])
            .size(None)
            .prop(PropSpec::new("label", PropKind::Text, "what is happening").initial("Loading…"))
            .prop(
                PropSpec::new(
                    "style",
                    PropKind::choice(&["dots", "line", "arc", "blocks"]),
                    "frames",
                )
                .default("dots"),
            )
            .prop(
                PropSpec::new("frame", PropKind::int(0, 99), "which frame to show")
                    .default(Value::Int(0)),
            )
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        Some(Size::new(
            2 + painter::text_width(kit::str_or(c, "label", "")),
            1,
        ))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let style = kit::str_or(c, "style", "dots");
        let frames = SPINNERS
            .iter()
            .find(|(n, _)| *n == style)
            .map_or(SPINNERS[0].1, |(_, f)| f);
        let frame = frames[usize::try_from(kit::int(c, "frame", 0)).unwrap_or(0) % frames.len()];
        let x = inner.x
            + kit::put(
                canvas,
                inner.x,
                inner.y,
                frame,
                kit::style(ctx, "primary", "surface"),
                inner,
            );
        kit::put(
            canvas,
            x + 1,
            inner.y,
            kit::str_or(c, "label", ""),
            kit::style(ctx, "foreground", "surface"),
            inner,
        );
    }
}
