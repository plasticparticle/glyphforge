//! Headless subcommands built on the same application API as the editor.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use glyphforge_core::api::{ApiError, Session};
use glyphforge_core::history::Origin;
use glyphforge_core::layout::{Dimension, Placement};
use glyphforge_core::patch::Patch;
use glyphforge_core::validate::Severity;
use glyphforge_core::{Component, Document, ObjectId, Size};

use crate::cli::Command;

#[derive(Debug, thiserror::Error)]
pub enum HeadlessError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("invalid id {0:?}: {1}")]
    Id(String, glyphforge_core::id::IdError),
    #[error("cannot read patch {path}: {source}")]
    ReadPatch {
        path: String,
        source: std::io::Error,
    },
    #[error("patch {path} is not valid JSON: {source}")]
    ParsePatch {
        path: String,
        source: serde_json::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("unsupported export format {0:?}; supported: text")]
    Format(String),
    #[error("unknown component kind {0:?}; `glyphforge kinds` lists them")]
    Kind(String),
    #[error("{0} already exists")]
    Exists(String),
    #[error(transparent)]
    Document(#[from] glyphforge_core::DocumentError),
    #[error(transparent)]
    Proposal(#[from] glyphforge_core::proposal::ProposalError),
}

fn parse_id(s: &str) -> Result<ObjectId, HeadlessError> {
    ObjectId::new(s).map_err(|e| HeadlessError::Id(s.to_owned(), e))
}

fn screen_id(session: &Session, screen: Option<&str>) -> Result<ObjectId, HeadlessError> {
    match screen {
        Some(s) => parse_id(s),
        None => Ok(session.document().first_screen().id.clone()),
    }
}

fn size_arg(width: Option<u16>, height: Option<u16>) -> Option<Size> {
    match (width, height) {
        (Some(w), Some(h)) => Some(Size::new(w, h)),
        _ => None,
    }
}

/// Runs a subcommand, writing to `out`. Returns the process exit code.
pub fn run(command: Command, out: &mut dyn Write) -> Result<u8, HeadlessError> {
    match command {
        Command::Inspect { file, json } => {
            let session = Session::open(&file)?;
            if json {
                write!(out, "{}", session.to_json()?).map_err(write_err(&file))?;
            } else {
                write!(out, "{}", outline(session.document())).map_err(write_err(&file))?;
            }
            Ok(0)
        }
        Command::Render {
            file,
            screen,
            width,
            height,
        } => {
            let session = Session::open(&file)?;
            let id = screen_id(&session, screen.as_deref())?;
            let text = session.render_text(&id, size_arg(width, height))?;
            writeln!(out, "{text}").map_err(write_err(&file))?;
            Ok(0)
        }
        Command::Validate {
            file,
            width,
            height,
            strict,
        } => {
            let session = Session::open(&file)?;
            let mut diagnostics = session.validate();
            if let Some(size) = size_arg(width, height) {
                let id = session.document().first_screen().id.clone();
                diagnostics.extend(session.validate_at(&id, size)?);
            }
            for d in &diagnostics {
                writeln!(out, "{d}").map_err(write_err(&file))?;
            }
            let errors = diagnostics
                .iter()
                .filter(|d| strict || d.severity == Severity::Error)
                .count();
            if diagnostics.is_empty() {
                writeln!(out, "ok: no problems found").map_err(write_err(&file))?;
            }
            Ok(u8::from(errors > 0))
        }
        Command::Apply {
            file,
            patch,
            out: out_path,
            dry_run,
        } => {
            let mut session = Session::open(&file)?;
            let patch = read_patch(&patch)?;
            let count = patch.operations.len();
            session.apply_patch(
                patch,
                format!("Apply patch ({count} operations)"),
                Origin::Agent {
                    name: "cli".into(),
                    description: "glyphforge apply".into(),
                },
            )?;
            if dry_run {
                let id = session.document().first_screen().id.clone();
                writeln!(out, "{}", session.render_text(&id, None)?).map_err(write_err(&file))?;
            } else {
                let target = out_path.unwrap_or(file);
                session.save_as(&target)?;
                writeln!(
                    out,
                    "applied {count} operation(s), saved {}",
                    target.display()
                )
                .map_err(write_err(&target))?;
            }
            Ok(0)
        }
        Command::Propose {
            file,
            patch: patch_path,
            agent,
            message,
        } => {
            let mut session = Session::open(&file)?;
            let patch = read_patch(&patch_path)?;
            let count = patch.operations.len();
            // Validate against the file now, so the agent learns about
            // errors here rather than the reviewer later.
            session.apply_patch(patch.clone(), "Check proposal", Origin::User)?;
            let proposal = glyphforge_core::proposal::Proposal {
                agent,
                message,
                created: glyphforge_core::proposal::now(),
                patch,
            };
            let path = glyphforge_core::proposal::submit(&file, &proposal)?;
            writeln!(
                out,
                "proposed {count} operation(s) for review: {}",
                path.display()
            )
            .map_err(write_err(&file))?;
            Ok(0)
        }
        Command::Export {
            file,
            format,
            screen,
            out: out_path,
        } => {
            if format != "text" {
                return Err(HeadlessError::Format(format));
            }
            let session = Session::open(&file)?;
            let id = screen_id(&session, screen.as_deref())?;
            let text = session.render_text(&id, None)?;
            match out_path {
                Some(p) => std::fs::write(&p, format!("{text}\n")).map_err(write_err(&p))?,
                None => writeln!(out, "{text}").map_err(write_err(&file))?,
            }
            Ok(0)
        }
        Command::Kinds { kind, json } => {
            let registry = glyphforge_core::render::Registry::builtin();
            let mut descriptors = registry.descriptors();
            if let Some(kind) = &kind {
                descriptors.retain(|d| d.kind == kind);
                if descriptors.is_empty() {
                    return Err(HeadlessError::Kind(kind.clone()));
                }
            }
            let text = if json {
                serde_json::to_string_pretty(&descriptors).unwrap_or_default()
            } else {
                kinds_text(&descriptors)
            };
            writeln!(out, "{text}").map_err(write_err(Path::new("stdout")))?;
            Ok(0)
        }
        Command::New {
            file,
            width,
            height,
        } => {
            if file.exists() {
                return Err(HeadlessError::Exists(file.display().to_string()));
            }
            let mut session = Session::new(Document::new(Size::new(width, height))?);
            session.save_as(&file)?;
            writeln!(out, "created {} ({width}×{height})", file.display())
                .map_err(write_err(&file))?;
            Ok(0)
        }
    }
}

fn read_patch(path: &Path) -> Result<Patch, HeadlessError> {
    let text = std::fs::read_to_string(path).map_err(|source| HeadlessError::ReadPatch {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| HeadlessError::ParsePatch {
        path: path.display().to_string(),
        source,
    })
}

fn write_err(path: &Path) -> impl Fn(std::io::Error) -> HeadlessError + '_ {
    move |source| HeadlessError::Write {
        path: path.display().to_string(),
        source,
    }
}

/// One block per kind: header line, summary, then one line per property.
fn kinds_text(descriptors: &[glyphforge_core::render::Descriptor]) -> String {
    let mut s = String::new();
    for d in descriptors {
        let _ = writeln!(
            s,
            "{} ({}, {}{})",
            d.kind,
            d.title,
            d.category.name(),
            if d.container { ", container" } else { "" }
        );
        if !d.summary.is_empty() {
            let _ = writeln!(s, "  {}", d.summary);
        }
        for p in &d.props {
            let _ = write!(s, "  {:<14} {:<18} {}", p.name, p.kind.label(), p.help);
            if let Some(v) = &p.default {
                let _ = write!(
                    s,
                    " [default {}]",
                    serde_json::to_string(v).unwrap_or_default()
                );
            }
            s.push('\n');
        }
    }
    s.trim_end().to_owned()
}

/// A compact, agent-readable outline of the document.
pub fn outline(doc: &Document) -> String {
    let mut s = String::new();
    let title = if doc.meta.title.is_empty() {
        "untitled"
    } else {
        &doc.meta.title
    };
    let _ = writeln!(s, "document: {title} (theme: {})", doc.theme.name);
    for g in &doc.gradients {
        let _ = writeln!(s, "gradient {} \"{}\": {}", g.id, g.name, g.stops_text());
    }
    for screen in doc.screens() {
        let _ = writeln!(
            s,
            "screen {} \"{}\" {}×{}",
            screen.id,
            screen.name,
            screen.size().width,
            screen.size().height
        );
        for layer in screen.layers() {
            let flags = format!(
                "{}{}",
                if layer.visible { "" } else { " hidden" },
                if layer.locked { " locked" } else { "" }
            );
            let _ = writeln!(
                s,
                "  layer {} \"{}\" [{}]{flags}",
                layer.id,
                layer.name,
                layer.kind().name()
            );
            if let Some(cells) = layer.cells() {
                let opaque = cells.cells().iter().filter(|c| !c.is_empty()).count();
                let _ = writeln!(s, "    {opaque} painted cells");
            }
            if let Some(raster) = layer.raster() {
                let shadow = raster
                    .shadow
                    .map_or_else(String::new, |sh| format!(", shadow {},{}", sh.dx, sh.dy));
                let _ = writeln!(
                    s,
                    "    {} painted pixels ({} {}×{}, dither {}, {}{shadow})",
                    raster.pixels.painted(),
                    raster.mode.name(),
                    raster.pixels.width(),
                    raster.pixels.height(),
                    raster.dither.name(),
                    raster.palette.name()
                );
            }
            if let Some(roots) = layer.components() {
                for c in roots {
                    outline_component(c, 2, &mut s);
                }
            }
        }
    }
    s
}

fn outline_component(c: &Component, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let mut line = format!("{indent}{} {}", c.id, c.kind);
    match c.layout.placement {
        Placement::Absolute { x, y } => {
            let _ = write!(line, " @{x},{y}");
        }
        Placement::Flow => {}
    }
    let dim = |d: Dimension| match d {
        Dimension::Fill => None,
        other => Some(other.to_string()),
    };
    if let Some(w) = dim(c.layout.width) {
        let _ = write!(line, " width={w}");
    }
    if let Some(h) = dim(c.layout.height) {
        let _ = write!(line, " height={h}");
    }
    for (k, v) in &c.props {
        let _ = write!(line, " {k}={v:?}");
    }
    if !c.responsive.is_empty() {
        let _ = write!(line, " responsive={}", c.responsive.len());
    }
    line.push('\n');
    out.push_str(&line);
    for child in &c.children {
        outline_component(child, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glyphforge-headless-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run_str(cmd: Command) -> (u8, String) {
        let mut out = Vec::new();
        let code = run(cmd, &mut out).unwrap();
        (code, String::from_utf8(out).unwrap())
    }

    #[test]
    fn new_apply_render_validate_pipeline() {
        let dir = temp_dir("pipeline");
        let file = dir.join("d.glyph");
        let (code, text) = run_str(Command::New {
            file: file.clone(),
            width: 30,
            height: 6,
        });
        assert_eq!(code, 0);
        assert!(text.contains("created"));
        assert!(matches!(
            run(
                Command::New {
                    file: file.clone(),
                    width: 1,
                    height: 1
                },
                &mut Vec::new()
            ),
            Err(HeadlessError::Exists(_))
        ));

        let patch = dir.join("p.json");
        std::fs::write(
            &patch,
            r#"{"operations":[
              {"op":"create_component","component":{"id":"metrics","kind":"panel","props":{"title":"Metrics"},
                 "layout":{"placement":{"mode":"absolute","x":0,"y":0},"width":"fixed(14)","height":"fixed(3)"}}},
              {"op":"set_property","target":"metrics","property":"border","value":"double"}
            ]}"#,
        )
        .unwrap();
        let (code, text) = run_str(Command::Apply {
            file: file.clone(),
            patch: patch.clone(),
            out: None,
            dry_run: true,
        });
        assert_eq!(code, 0);
        assert!(text.starts_with("╔ Metrics ═══╗"), "{text}");
        let (code, _) = run_str(Command::Apply {
            file: file.clone(),
            patch,
            out: None,
            dry_run: false,
        });
        assert_eq!(code, 0);

        let (_, text) = run_str(Command::Render {
            file: file.clone(),
            screen: None,
            width: None,
            height: None,
        });
        assert!(text.starts_with("╔ Metrics ═══╗"), "{text}");
        let (_, text) = run_str(Command::Inspect {
            file: file.clone(),
            json: false,
        });
        assert!(
            text.contains(
                "metrics panel @0,0 width=fixed(14) height=fixed(3) border=Str(\"double\")"
            ),
            "{text}"
        );
        let (code, text) = run_str(Command::Validate {
            file: file.clone(),
            width: None,
            height: None,
            strict: false,
        });
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("ok"));
        let (code, text) = run_str(Command::Validate {
            file: file.clone(),
            width: Some(10),
            height: Some(2),
            strict: false,
        });
        assert_eq!(code, 0, "warnings alone do not fail: {text}");
        assert!(text.contains("clipped"), "{text}");
        let (code, _) = run_str(Command::Validate {
            file: file.clone(),
            width: Some(10),
            height: Some(2),
            strict: true,
        });
        assert_eq!(code, 1);
        let exported = dir.join("out.txt");
        run_str(Command::Export {
            file: file.clone(),
            format: "text".into(),
            screen: None,
            out: Some(exported.clone()),
        });
        assert!(std::fs::read_to_string(&exported).unwrap().starts_with("╔"));
        assert!(matches!(
            run(
                Command::Export {
                    file,
                    format: "png".into(),
                    screen: None,
                    out: None
                },
                &mut Vec::new()
            ),
            Err(HeadlessError::Format(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(
            run(
                Command::Inspect {
                    file: "/nonexistent/x.glyph".into(),
                    json: false
                },
                &mut Vec::new()
            )
            .is_err()
        );
    }

    #[test]
    fn kinds_lists_descriptors_as_text_and_json() {
        let (code, text) = run_str(Command::Kinds {
            kind: None,
            json: false,
        });
        assert_eq!(code, 0);
        assert!(text.contains("panel (Panel, Layout, container)"), "{text}");
        assert!(text.contains("  align          left|center|right"));
        let (_, json) = run_str(Command::Kinds {
            kind: Some("button".into()),
            json: true,
        });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["kind"], "button");
        assert_eq!(v[0]["props"][0]["name"], "label");
        assert!(matches!(
            run(
                Command::Kinds {
                    kind: Some("nope".into()),
                    json: false
                },
                &mut Vec::new()
            ),
            Err(HeadlessError::Kind(_))
        ));
    }

    #[test]
    fn propose_validates_and_stores_without_applying() {
        let dir = temp_dir("propose");
        let file = dir.join("d.glyph");
        run_str(Command::New {
            file: file.clone(),
            width: 30,
            height: 6,
        });
        let patch = dir.join("p.json");
        std::fs::write(
            &patch,
            r#"{"operations":[{"op":"create_component","layer":"main-ui","component":{"id":"hello","kind":"label","props":{"text":"Hi"}}}]}"#,
        )
        .unwrap();
        let (code, text) = run_str(Command::Propose {
            file: file.clone(),
            patch: patch.clone(),
            agent: "tester".into(),
            message: "say hi".into(),
        });
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("0001-tester.json"), "{text}");
        let doc = Session::open(&file).unwrap();
        assert!(doc.document().components().next().is_none(), "not applied");
        assert_eq!(glyphforge_core::proposal::list(&file).len(), 1);
        // An invalid patch is refused at once.
        std::fs::write(
            &patch,
            r#"{"operations":[{"op":"delete_component","target":"nope"}]}"#,
        )
        .unwrap();
        assert!(
            run(
                Command::Propose {
                    file: file.clone(),
                    patch,
                    agent: "tester".into(),
                    message: String::new()
                },
                &mut Vec::new()
            )
            .is_err()
        );
        assert_eq!(glyphforge_core::proposal::list(&file).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
