//! Headless subcommands built on the same application API as the editor.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use glyphforge_core::api::{ApiError, Session};
use glyphforge_core::history::Origin;
use glyphforge_core::patch::Patch;
use glyphforge_core::validate::Severity;
use glyphforge_core::{Document, ObjectId, Size};

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
    #[error(
        "unsupported export format {0:?}; supported: text, ansi, ansi256, ansi16, handoff, ratatui, textual, bubbletea, ink"
    )]
    Format(String),
    #[error("unknown component kind {0:?}; `glyphforge kinds` lists them")]
    Kind(String),
    #[error("unknown template {0:?}; `glyphforge templates` lists them")]
    Template(String),
    #[error("unknown theme {0:?}")]
    Theme(String),
    #[error("{0} already exists")]
    Exists(String),
    #[error(transparent)]
    Document(#[from] glyphforge_core::DocumentError),
    #[error(transparent)]
    Proposal(#[from] glyphforge_core::proposal::ProposalError),
    #[error(transparent)]
    Image(#[from] crate::image::ImageError),
    #[error("unknown pixel mode {0:?}; use half, quadrant, sextant, braille or octant")]
    Mode(String),
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
                write!(
                    out,
                    "{}",
                    glyphforge_core::export::outline::outline(session.document())
                )
                .map_err(write_err(&file))?;
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
            advice,
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
            if advice {
                for a in glyphforge_core::advice::advise(session.document(), session.registry()) {
                    writeln!(out, "{a}").map_err(write_err(&file))?;
                }
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
        } => propose(&file, &patch_path, agent, message, out),
        Command::Export {
            file,
            format,
            screen,
            out: out_path,
        } => {
            let session = Session::open(&file)?;
            let id = screen_id(&session, screen.as_deref())?;
            let text = export_text(&session, &id, &format)?;
            match out_path {
                Some(p) => std::fs::write(&p, &text).map_err(write_err(&p))?,
                None => write!(out, "{text}").map_err(write_err(&file))?,
            }
            Ok(0)
        }
        Command::Import {
            file,
            image,
            screen,
            mode,
            name,
            x,
            y,
            cols,
            rows,
        } => import(
            ImportArgs {
                file,
                image,
                screen,
                mode,
                name,
                x,
                y,
                cols,
                rows,
            },
            out,
        ),
        Command::Library { op } => library(op, out),
        Command::Kinds { kind, json } => kinds(kind.as_deref(), json, out),
        Command::New {
            file,
            width,
            height,
            theme,
            template,
        } => new_document(
            &file,
            Size::new(width, height),
            theme.as_deref(),
            template.as_deref(),
            out,
        ),
        Command::Templates => {
            for (name, description, _) in crate::templates::TEMPLATES {
                writeln!(out, "{name:<22} {description}")
                    .map_err(write_err(Path::new("stdout")))?;
            }
            Ok(0)
        }
    }
}

/// `glyphforge new`: an empty document, or a template, optionally with a
/// built-in theme.
fn new_document(
    file: &Path,
    size: Size,
    theme: Option<&str>,
    template: Option<&str>,
    out: &mut dyn Write,
) -> Result<u8, HeadlessError> {
    if file.exists() {
        return Err(HeadlessError::Exists(file.display().to_string()));
    }
    let mut doc = match template {
        Some(name) => {
            let json = crate::templates::template(name)
                .ok_or_else(|| HeadlessError::Template(name.to_owned()))?;
            glyphforge_core::project::from_json(json).map_err(ApiError::from)?
        }
        None => Document::new(size)?,
    };
    if let Some(id) = theme {
        doc.theme = glyphforge_core::Theme::builtin_by_id(id)
            .ok_or_else(|| HeadlessError::Theme(id.to_owned()))?;
    }
    let size = doc.first_screen().size();
    let mut session = Session::new(doc);
    session.save_as(file)?;
    writeln!(
        out,
        "created {} ({}×{})",
        file.display(),
        size.width,
        size.height
    )
    .map_err(write_err(file))?;
    Ok(0)
}

fn propose(
    file: &Path,
    patch_path: &Path,
    agent: String,
    message: String,
    out: &mut dyn Write,
) -> Result<u8, HeadlessError> {
    let mut session = Session::open(file)?;
    let patch = read_patch(patch_path)?;
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
    let path = glyphforge_core::proposal::submit(file, &proposal)?;
    writeln!(
        out,
        "proposed {count} operation(s) for review: {}",
        path.display()
    )
    .map_err(write_err(file))?;
    Ok(0)
}

/// Arguments of `glyphforge import`.
struct ImportArgs {
    file: std::path::PathBuf,
    image: std::path::PathBuf,
    screen: Option<String>,
    mode: String,
    name: Option<String>,
    x: u16,
    y: u16,
    cols: Option<u16>,
    rows: Option<u16>,
}

/// A screen (or, for `handoff`, the whole design) in an export format.
pub fn export_text(
    session: &Session,
    screen: &ObjectId,
    format: &str,
) -> Result<String, HeadlessError> {
    use glyphforge_core::export::{ansi, codegen, handoff};
    if format == "text" {
        return Ok(format!("{}\n", session.render_text(screen, None)?));
    }
    if format == "handoff" {
        return Ok(handoff::handoff(session.document(), session.registry()));
    }
    if let Some(depth) = ansi::Depth::parse(format) {
        return Ok(ansi::to_ansi(&session.render(screen, None)?, depth));
    }
    let exporter =
        codegen::exporter(format).ok_or_else(|| HeadlessError::Format(format.to_owned()))?;
    let s = session
        .document()
        .screen(screen)
        .ok_or_else(|| ApiError::NoSuchScreen(screen.clone()))?;
    Ok(exporter.generate(session.document(), s))
}

fn import(args: ImportArgs, out: &mut dyn Write) -> Result<u8, HeadlessError> {
    let ImportArgs {
        file,
        image,
        screen,
        mode,
        name,
        x,
        y,
        cols,
        rows,
    } = args;
    let mode = glyphforge_core::raster::RasterMode::parse(&mode)
        .ok_or_else(|| HeadlessError::Mode(mode.clone()))?;
    let mut session = Session::open(&file)?;
    let screen = screen_id(&session, screen.as_deref())?;
    let name = name.unwrap_or_else(|| {
        image
            .file_stem()
            .map_or_else(|| "Image".to_owned(), |s| s.to_string_lossy().into_owned())
    });
    if !image
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        let text = std::fs::read_to_string(&image).map_err(|source| HeadlessError::ReadPatch {
            path: image.display().to_string(),
            source,
        })?;
        let (patch, id) =
            crate::image::text_patch(session.document(), &screen, &text, (x, y), &name)?;
        session.apply_patch(patch, "Import text art", Origin::User)?;
        session.save_as(&file)?;
        writeln!(out, "imported {} as artwork layer {id}", image.display())
            .map_err(write_err(&file))?;
        return Ok(0);
    }
    let pixels = crate::image::load_png(&image)?;
    let options = crate::image::ImportOptions {
        mode,
        at: (x, y),
        fit: size_arg(cols, rows),
        name,
    };
    let (patch, id) = crate::image::import_patch(session.document(), &screen, &pixels, &options)?;
    session.apply_patch(patch, "Import image", Origin::User)?;
    session.save_as(&file)?;
    writeln!(
        out,
        "imported {} ({}×{}) as pixel layer {id}",
        image.display(),
        pixels.width,
        pixels.height
    )
    .map_err(write_err(&file))?;
    Ok(0)
}

fn library(op: crate::cli::LibraryOp, out: &mut dyn Write) -> Result<u8, HeadlessError> {
    use glyphforge_core::symbol::{export_library, import_library};
    match op {
        crate::cli::LibraryOp::Export { file, out: target } => {
            let session = Session::open(&file)?;
            let lib =
                Session::new(export_library(session.document())).with_path(Some(target.clone()));
            let mut lib = lib;
            lib.save()?;
            let doc = lib.document();
            writeln!(
                out,
                "library {}: {} symbol(s), {} gradient(s), {} palette(s), {} fixture(s)",
                target.display(),
                doc.symbols.len(),
                doc.gradients.len(),
                doc.palettes.len(),
                doc.data.len()
            )
            .map_err(write_err(&target))?;
        }
        crate::cli::LibraryOp::Import { file, library } => {
            let mut session = Session::open(&file)?;
            let lib = Session::open(&library)?;
            let patch = import_library(session.document(), lib.document());
            let n = patch.operations.len();
            session.apply_patch(patch, "Import library", Origin::User)?;
            session.save()?;
            writeln!(out, "imported {n} item(s) from {}", library.display())
                .map_err(write_err(&file))?;
        }
    }
    Ok(0)
}

fn kinds(kind: Option<&str>, json: bool, out: &mut dyn Write) -> Result<u8, HeadlessError> {
    let registry = glyphforge_core::render::Registry::builtin();
    let mut descriptors = registry.descriptors();
    if let Some(kind) = kind {
        descriptors.retain(|d| d.kind == kind);
        if descriptors.is_empty() {
            return Err(HeadlessError::Kind(kind.to_owned()));
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
            theme: None,
            template: None,
        });
        assert_eq!(code, 0);
        assert!(text.contains("created"));
        assert!(matches!(
            run(
                Command::New {
                    file: file.clone(),
                    width: 1,
                    height: 1,
                    theme: None,
                    template: None,
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
            advice: false,
        });
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("ok"));
        let (code, text) = run_str(Command::Validate {
            file: file.clone(),
            width: Some(10),
            height: Some(2),
            strict: false,
            advice: true,
        });
        assert_eq!(code, 0, "warnings alone do not fail: {text}");
        assert!(text.contains("clipped"), "{text}");
        let (code, _) = run_str(Command::Validate {
            file: file.clone(),
            width: Some(10),
            height: Some(2),
            strict: true,
            advice: false,
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
            theme: None,
            template: None,
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

    #[test]
    fn import_adds_a_pixel_layer() {
        let dir = temp_dir("import");
        let file = dir.join("d.glyph");
        run_str(Command::New {
            file: file.clone(),
            width: 20,
            height: 8,
            theme: None,
            template: None,
        });
        let png = dir.join("logo.png");
        crate::image::tests::write_png(&png, 8, 8);
        let (code, text) = run_str(Command::Import {
            file: file.clone(),
            image: png,
            screen: None,
            mode: "braille".into(),
            name: None,
            x: 2,
            y: 1,
            cols: Some(4),
            rows: Some(2),
        });
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("pixel layer logo"), "{text}");
        let session = Session::open(&file).unwrap();
        let rendered = session
            .render_text(&session.document().first_screen().id, None)
            .unwrap();
        assert!(
            rendered.contains("⣤⣤") && rendered.contains("⠛⠛"),
            "{rendered}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn library_export_and_import_round_trip() {
        let dir = temp_dir("library");
        let a = dir.join("a.glyph");
        let b = dir.join("b.glyph");
        let lib = dir.join("lib.glyph");
        for f in [&a, &b] {
            run_str(Command::New {
                file: f.clone(),
                width: 20,
                height: 5,
                theme: None,
                template: None,
            });
        }
        let patch = dir.join("p.json");
        std::fs::write(
            &patch,
            r#"{"operations":[{"op":"set_palette","name":"Stars","glyphs":"★ ☆"},{"op":"set_data","key":"cpu","value":[1,2]}]}"#,
        )
        .unwrap();
        run_str(Command::Apply {
            file: a.clone(),
            patch,
            out: None,
            dry_run: false,
        });
        let (code, text) = run_str(Command::Library {
            op: crate::cli::LibraryOp::Export {
                file: a,
                out: lib.clone(),
            },
        });
        assert_eq!(code, 0);
        assert!(text.contains("1 palette(s), 1 fixture(s)"), "{text}");
        let (code, _) = run_str(Command::Library {
            op: crate::cli::LibraryOp::Import {
                file: b.clone(),
                library: lib,
            },
        });
        assert_eq!(code, 0);
        let doc = Session::open(&b).unwrap();
        assert_eq!(doc.document().palettes.len(), 1);
        assert_eq!(doc.document().data.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn export_formats_and_text_art_import() {
        let dir = temp_dir("formats");
        let file = dir.join("d.glyph");
        run_str(Command::New {
            file: file.clone(),
            width: 20,
            height: 4,
            theme: None,
            template: None,
        });
        let art = dir.join("logo.ans");
        std::fs::write(&art, "\x1b[31mHI\x1b[0m there\n").unwrap();
        let (code, text) = run_str(Command::Import {
            file: file.clone(),
            image: art,
            screen: None,
            mode: "sextant".into(),
            name: None,
            x: 1,
            y: 1,
            cols: None,
            rows: None,
        });
        assert_eq!(code, 0, "{text}");
        let export = |format: &str| {
            run_str(Command::Export {
                file: file.clone(),
                format: format.into(),
                screen: None,
                out: None,
            })
            .1
        };
        assert!(
            export("text")
                .lines()
                .nth(1)
                .unwrap()
                .starts_with(" HI there")
        );
        assert!(export("ansi16").contains("\x1b[31mHI"));
        assert!(export("handoff").contains("kind: artwork"));
        for code in ["ratatui", "textual", "bubbletea", "ink"] {
            assert!(!export(code).is_empty(), "{code}");
        }
        assert!(matches!(
            run(
                Command::Export {
                    file,
                    format: "pdf".into(),
                    screen: None,
                    out: None
                },
                &mut Vec::new()
            ),
            Err(HeadlessError::Format(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
