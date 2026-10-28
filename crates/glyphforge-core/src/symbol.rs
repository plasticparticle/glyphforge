//! Symbols and libraries.
//!
//! A symbol is a reusable component tree stored in the document. An
//! `instance` component shows a symbol: it expands the master at render
//! time, so editing the master changes every instance. Instances override
//! properties: any property of the instance besides `symbol` and
//! `overrides` applies to the master's root, and `overrides` maps
//! `"<descendant id>.<property>"` to a value for nested components.
//!
//! A library is an ordinary `.glyph` document whose symbols, gradients,
//! glyph palettes and data fixtures another document can import.

use serde::{Deserialize, Serialize};

use crate::component::Component;
use crate::document::Document;
use crate::id::ObjectId;
use crate::patch::{Operation, Patch};
use crate::value::Value;

/// A reusable component tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: ObjectId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The master. Ids inside it are local to the symbol.
    pub component: Component,
}

/// A named set of glyphs offered as a brush page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlyphPalette {
    pub name: String,
    /// Space-separated glyphs.
    pub glyphs: String,
}

/// Properties of an instance that are not overrides of the master root.
const OWN_PROPS: [&str; 2] = ["symbol", "overrides"];

/// The master of `instance`'s symbol with the instance's overrides
/// applied, sized and placed by the instance (the master's own placement
/// is dropped: it fills the instance).
pub fn expand(instance: &Component, symbols: &[Symbol]) -> Option<Component> {
    let id = instance.prop_str("symbol")?;
    let symbol = symbols.iter().find(|s| s.id.as_str() == id)?;
    let mut root = symbol.component.clone();
    root.layout.placement = crate::layout::Placement::Flow;
    root.layout.width = crate::layout::Dimension::Fill;
    root.layout.height = crate::layout::Dimension::Fill;
    root.layout.margin = crate::layout::Edges::ZERO;
    for (k, v) in &instance.props {
        if !OWN_PROPS.contains(&k.as_str()) {
            root.props.insert(k.clone(), v.clone());
        }
    }
    if let Some(Value::Map(overrides)) = instance.prop("overrides") {
        for (path, value) in overrides {
            let Some((target, prop)) = path.rsplit_once('.') else {
                continue;
            };
            if let Some(c) = ObjectId::new(target).ok().and_then(|t| root.find_mut(&t)) {
                c.props.insert(prop.to_owned(), value.clone());
            }
        }
    }
    Some(root)
}

/// The patch that adds `library`'s symbols, gradients, palettes and data
/// fixtures to `doc`. Existing entries with the same id or key are
/// replaced (that is how a library update propagates); a library symbol
/// whose id is taken by something else in `doc` gets a fresh id.
pub fn import_library(doc: &Document, library: &Document) -> Patch {
    let taken = doc.ids();
    let mut ops = Vec::new();
    for s in &library.symbols {
        let mut s = s.clone();
        let clash = taken.contains(&s.id) && !doc.symbols.iter().any(|d| d.id == s.id);
        if clash {
            s.id = s.id.unique_among(|i| taken.contains(i));
        }
        ops.push(Operation::SetSymbol { symbol: s });
    }
    for g in &library.gradients {
        let clash = taken.contains(&g.id) && doc.gradient(&g.id).is_none();
        if !clash {
            ops.push(Operation::SetGradient {
                gradient: g.clone(),
            });
        }
    }
    for p in &library.palettes {
        ops.push(Operation::SetPalette {
            name: p.name.clone(),
            glyphs: p.glyphs.clone(),
        });
    }
    for (key, value) in &library.data {
        ops.push(Operation::SetData {
            key: key.clone(),
            value: value.clone(),
        });
    }
    Patch::new(ops)
}

/// A library document holding `doc`'s reusable parts: theme, symbols,
/// gradients, palettes and fixtures, with one empty screen.
pub fn export_library(doc: &Document) -> Document {
    let mut lib = Document::new(crate::document::Size::new(40, 12))
        .unwrap_or_else(|_| unreachable!("library screens have a fixed, valid size"));
    lib.meta.clone_from(&doc.meta);
    lib.theme.clone_from(&doc.theme);
    lib.symbols.clone_from(&doc.symbols);
    lib.gradients.clone_from(&doc.gradients);
    lib.palettes.clone_from(&doc.palettes);
    lib.data.clone_from(&doc.data);
    lib
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Size;
    use crate::id;
    use crate::layout::Layout;

    fn card() -> Symbol {
        Symbol {
            id: id!("card"),
            name: "Card".into(),
            description: String::new(),
            component: Component::new(id!("frame"), "panel")
                .with_prop("title", "Card")
                .with_layout(Layout::absolute(3, 3, 20, 4))
                .with_children(vec![
                    Component::new(id!("body"), "label").with_prop("text", "Body"),
                ]),
        }
    }

    #[test]
    fn instances_expand_with_overrides() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("body.text".to_owned(), Value::from("Hello"));
        let instance = Component::new(id!("i"), "instance")
            .with_prop("symbol", "card")
            .with_prop("title", "Mine")
            .with_prop("overrides", Value::Map(overrides));
        let root = expand(&instance, &[card()]).unwrap();
        assert_eq!(root.prop_str("title"), Some("Mine"));
        assert_eq!(root.children[0].prop_str("text"), Some("Hello"));
        assert_eq!(root.layout.placement, crate::layout::Placement::Flow);
        assert!(expand(&instance.clone().with_prop("symbol", "nope"), &[card()]).is_none());
    }

    #[test]
    fn libraries_export_and_import() {
        let mut source = Document::new(Size::new(10, 4)).unwrap();
        source.symbols.push(card());
        source.palettes.push(GlyphPalette {
            name: "Stars".into(),
            glyphs: "★ ☆ ✦".into(),
        });
        let lib = export_library(&source);
        let mut target = Document::new(Size::new(10, 4)).unwrap();
        import_library(&target, &lib).apply(&mut target).unwrap();
        assert_eq!(target.symbols, vec![card()]);
        assert_eq!(target.palettes.len(), 1);
        // Importing again replaces instead of duplicating.
        import_library(&target, &lib).apply(&mut target).unwrap();
        assert_eq!(target.symbols.len(), 1);
    }
}
