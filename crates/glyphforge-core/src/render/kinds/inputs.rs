//! Inputs and controls: text fields, text areas, check boxes, radio
//! buttons, toggles and selects.

use crate::component::Component;
use crate::document::{Canvas, Cell, Rect, Size};
use crate::layout::Edges;
use crate::render::descriptor::{Category, Descriptor, PropKind, PropSpec};
use crate::render::kit::{self, State};
use crate::render::{ComponentRenderer, RenderContext, painter};

/// How a text field shows its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Text,
    Password,
    Search,
}

/// Single-line text fields: `input`, `password`, `search`.
#[derive(Debug)]
pub struct Field(FieldKind);

impl Field {
    pub const INPUT: Self = Self(FieldKind::Text);
    pub const PASSWORD: Self = Self(FieldKind::Password);
    pub const SEARCH: Self = Self(FieldKind::Search);

    fn shown(&self, c: &Component) -> (String, bool) {
        let value = c.prop_str("value").unwrap_or("");
        if value.is_empty() {
            return (c.prop_str("placeholder").unwrap_or("").to_owned(), true);
        }
        let v = match self.0 {
            FieldKind::Password => "•".repeat(value.chars().count()),
            _ => value.to_owned(),
        };
        (v, false)
    }

    fn prefix(&self) -> &'static str {
        match self.0 {
            FieldKind::Search => "⌕ ",
            _ => "",
        }
    }
}

impl ComponentRenderer for Field {
    fn kind(&self) -> &'static str {
        match self.0 {
            FieldKind::Text => "input",
            FieldKind::Password => "password",
            FieldKind::Search => "search",
        }
    }

    fn descriptor(&self) -> Descriptor {
        let (title, summary, keywords): (&str, &str, &[&str]) = match self.0 {
            FieldKind::Text => (
                "Input",
                "Single-line text field",
                &["text field", "entry", "form", "textbox"],
            ),
            FieldKind::Password => (
                "Password",
                "Text field that hides its value",
                &["secret", "login", "form"],
            ),
            FieldKind::Search => (
                "Search",
                "Search field with a magnifier",
                &["find", "filter", "query"],
            ),
        };
        Descriptor::new(self.kind(), title, Category::Input)
            .summary(summary)
            .keywords(keywords)
            .size(Some((24, 1)))
            .prop(PropSpec::new(
                "label",
                PropKind::Text,
                "label before the field",
            ))
            .prop(PropSpec::new("value", PropKind::Text, "current text"))
            .prop(
                PropSpec::new("placeholder", PropKind::Text, "hint shown when empty").initial(
                    match self.0 {
                        FieldKind::Search => "Search…",
                        FieldKind::Password => "Password",
                        FieldKind::Text => "Type here…",
                    },
                ),
            )
            .prop(PropSpec::new(
                "border",
                PropKind::Bool,
                "draw the field as a box (3 rows)",
            ))
            .prop(kit::state_prop())
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        let label = c
            .prop_str("label")
            .map_or(0, |l| painter::text_width(l) + 1);
        let (text, _) = self.shown(c);
        let field = painter::text_width(&text).max(12) + 2 + painter::text_width(self.prefix());
        let rows = if c.prop_bool("border") == Some(true) {
            3
        } else {
            1
        };
        Some(Size::new(label + field, rows))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let state = kit::state(c);
        let mut x = inner.x;
        if let Some(label) = c.prop_str("label") {
            x += kit::put(
                canvas,
                x,
                inner.y,
                &format!("{label} "),
                kit::style(ctx, "foreground", "surface"),
                inner,
            );
        }
        let boxed = c.prop_bool("border") == Some(true) && inner.height >= 3;
        let field = Rect::new(
            x,
            inner.y,
            inner.right().saturating_sub(u32::from(x)) as u16,
            if boxed { 3 } else { 1 },
        );
        let (fg, bg) = match state {
            State::Disabled => ("foreground-muted", "surface"),
            _ => ("foreground", "surface-alt"),
        };
        let base = kit::style(ctx, fg, bg);
        let text_row = if boxed {
            let border = match state {
                State::Focused => "focus",
                State::Error => "error",
                _ => "border",
            };
            painter::fill(canvas, field, &Cell::blank(base));
            painter::draw_border(canvas, field, ctx.theme.border, kit::style(ctx, border, bg));
            field.y + 1
        } else {
            painter::fill(
                canvas,
                Rect::new(field.x, field.y, field.width, 1),
                &Cell::blank(base),
            );
            field.y
        };
        let text_x = field.x + 1;
        let (text, placeholder) = self.shown(c);
        let style = if placeholder {
            kit::dim(kit::style(ctx, "foreground-muted", bg))
        } else {
            base
        };
        let clip = Rect::new(text_x, text_row, field.width.saturating_sub(2), 1);
        let mut cx = text_x
            + kit::put(
                canvas,
                text_x,
                text_row,
                self.prefix(),
                kit::style(ctx, "foreground-muted", bg),
                clip,
            );
        if !placeholder || state != State::Focused {
            cx += kit::put(canvas, cx, text_row, &text, style, clip);
        }
        match state {
            State::Focused => {
                let caret = kit::style(ctx, "focus", bg);
                kit::put(
                    canvas,
                    cx.min(clip.right().saturating_sub(1) as u16),
                    text_row,
                    "▏",
                    caret,
                    clip,
                );
            }
            State::Error if !boxed => {
                let w = field.width;
                kit::put(
                    canvas,
                    field.x + w.saturating_sub(2),
                    text_row,
                    "!",
                    kit::bold(kit::style(ctx, "error", bg)),
                    field,
                );
            }
            _ => {}
        }
    }
}

/// Multi-line text area with a border.
#[derive(Debug)]
pub struct TextArea;

impl ComponentRenderer for TextArea {
    fn kind(&self) -> &'static str {
        "textarea"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("textarea", "Text Area", Category::Input)
            .summary("Multi-line text field")
            .keywords(&["multiline", "editor", "notes", "comment"])
            .size(Some((30, 6)))
            .prop(PropSpec::new(
                "value",
                PropKind::Text,
                "text; \\n separates lines",
            ))
            .prop(
                PropSpec::new("placeholder", PropKind::Text, "hint shown when empty")
                    .initial("Write something…"),
            )
            .prop(kit::state_prop())
    }

    fn chrome_inset(&self, _: &Component, _: &RenderContext<'_>) -> Edges {
        Edges::symmetric(1, 2)
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let state = kit::state(c);
        let base = kit::style(ctx, "foreground", "surface-alt");
        painter::fill(canvas, rect, &Cell::blank(base));
        let border = if state == State::Focused {
            "focus"
        } else {
            "border"
        };
        painter::draw_border(
            canvas,
            rect,
            ctx.theme.border,
            kit::style(ctx, border, "surface-alt"),
        );
        let value = c.prop_str("value").unwrap_or("");
        let (lines, style): (Vec<&str>, _) = if value.is_empty() {
            (
                c.prop_str("placeholder").unwrap_or("").lines().collect(),
                kit::dim(kit::style(ctx, "foreground-muted", "surface-alt")),
            )
        } else {
            (value.lines().collect(), base)
        };
        for (i, line) in lines.iter().enumerate().take(usize::from(inner.height)) {
            kit::put(canvas, inner.x, inner.y + i as u16, line, style, inner);
        }
    }
}

/// What a two-state control looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    Box,
    Radio,
    Toggle,
}

/// `checkbox`, `radio` and `toggle`: a mark and a label.
#[derive(Debug)]
pub struct CheckControl(Check);

impl CheckControl {
    pub const CHECKBOX: Self = Self(Check::Box);
    pub const RADIO: Self = Self(Check::Radio);
    pub const TOGGLE: Self = Self(Check::Toggle);

    fn on(c: &Component) -> bool {
        c.prop_bool("checked").or_else(|| c.prop_bool("on")) == Some(true)
    }

    fn mark(&self, on: bool) -> &'static str {
        match (self.0, on) {
            (Check::Box, true) => "[x]",
            (Check::Box, false) => "[ ]",
            (Check::Radio, true) => "(•)",
            (Check::Radio, false) => "( )",
            (Check::Toggle, true) => "━━●",
            (Check::Toggle, false) => "●━━",
        }
    }
}

impl ComponentRenderer for CheckControl {
    fn kind(&self) -> &'static str {
        match self.0 {
            Check::Box => "checkbox",
            Check::Radio => "radio",
            Check::Toggle => "toggle",
        }
    }

    fn descriptor(&self) -> Descriptor {
        let (title, summary, keywords): (&str, &str, &[&str]) = match self.0 {
            Check::Box => (
                "Checkbox",
                "Two-state check box with a label",
                &["check", "tick", "option", "form"],
            ),
            Check::Radio => (
                "Radio",
                "One choice of a group",
                &["option", "choice", "form"],
            ),
            Check::Toggle => (
                "Toggle",
                "On/off switch",
                &["switch", "on", "off", "setting"],
            ),
        };
        Descriptor::new(self.kind(), title, Category::Input)
            .summary(summary)
            .keywords(keywords)
            .size(None)
            .prop(PropSpec::new("label", PropKind::Text, "text after the mark").initial(title))
            .prop(PropSpec::new("checked", PropKind::Bool, "checked / on").initial(true))
            .prop(kit::state_prop())
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        let label = c
            .prop_str("label")
            .map_or(0, |l| painter::text_width(l) + 1);
        Some(Size::new(3 + label, 1))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let state = kit::state(c);
        let on = Self::on(c);
        let disabled = state == State::Disabled;
        let mark_fg = match (disabled, on) {
            (false, true) => "primary",
            _ => "foreground-muted",
        };
        let mut mark = kit::style(ctx, mark_fg, "surface");
        if state == State::Focused {
            mark = kit::bold(mark);
        }
        let x = inner.x + kit::put(canvas, inner.x, inner.y, self.mark(on), mark, inner);
        if let Some(label) = c.prop_str("label") {
            let mut style = kit::style(
                ctx,
                if disabled {
                    "foreground-muted"
                } else {
                    "foreground"
                },
                "surface",
            );
            if state == State::Focused {
                style = kit::underline(style);
            }
            kit::put(canvas, x + 1, inner.y, label, style, inner);
        }
    }
}

/// Drop-down select: closed shows the value; `open` lists the options.
#[derive(Debug)]
pub struct Select;

impl ComponentRenderer for Select {
    fn kind(&self) -> &'static str {
        "select"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("select", "Select", Category::Input)
            .summary("Drop-down choice of options")
            .keywords(&["dropdown", "combo", "picker", "options", "form"])
            .size(Some((20, 1)))
            .prop(
                PropSpec::new("options", PropKind::List, "the choices").initial(Value::List(vec![
                    "One".into(),
                    "Two".into(),
                    "Three".into(),
                ])),
            )
            .prop(PropSpec::new("value", PropKind::Text, "chosen option").initial("One"))
            .prop(PropSpec::new(
                "open",
                PropKind::Bool,
                "show the option list below",
            ))
            .prop(kit::bind_prop())
            .prop(kit::state_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let options = kit::strings(c, ctx, "options");
        let w = options
            .iter()
            .map(|o| painter::text_width(o))
            .max()
            .unwrap_or(4)
            .max(4)
            + 4;
        let h = if c.prop_bool("open") == Some(true) {
            1 + options.len() as u16
        } else {
            1
        };
        Some(Size::new(w, h))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let state = kit::state(c);
        let value = c.prop_str("value").unwrap_or("");
        let field = kit::style(
            ctx,
            if state == State::Disabled {
                "foreground-muted"
            } else {
                "foreground"
            },
            "surface-alt",
        );
        let w = inner.width;
        let text = kit::fit(&format!(" {value}"), w.saturating_sub(2));
        kit::put(
            canvas,
            inner.x,
            inner.y,
            &format!("{text}▾ "),
            if state == State::Focused {
                kit::bold(field)
            } else {
                field
            },
            inner,
        );
        if c.prop_bool("open") != Some(true) {
            return;
        }
        for (i, option) in kit::strings(c, ctx, "options").iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            let style = if option == value {
                kit::style(ctx, "selection-foreground", "selection")
            } else {
                kit::style(ctx, "foreground", "surface")
            };
            kit::put(
                canvas,
                inner.x,
                y,
                &kit::fit(&format!(" {option}"), w),
                style,
                inner,
            );
        }
    }
}

use crate::value::Value;
