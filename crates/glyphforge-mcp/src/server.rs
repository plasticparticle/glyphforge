//! The MCP server: JSON-RPC 2.0 requests in, responses out. Tools work
//! on one project file through the same `Session` API as the editor and
//! the CLI; changes are saved after every successful edit, or stored as a
//! proposal for review in the editor when `propose` is set.

use std::path::PathBuf;

use glyphforge_core::api::Session;
use glyphforge_core::export::{ansi, codegen, handoff, outline};
use glyphforge_core::history::Origin;
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::proposal::{self, Proposal};
use glyphforge_core::{Component, ObjectId, Size};
use serde_json::{Value as Json, json};

/// The protocol revision this server speaks.
pub const PROTOCOL: &str = "2025-06-18";

/// One server, bound to one project file.
#[derive(Debug)]
pub struct Server {
    path: PathBuf,
    agent: String,
}

/// A tool failure: reported to the client as an error result.
type ToolResult = Result<String, String>;

fn tool(name: &str, description: &str, schema: &Json) -> Json {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

fn object(properties: &Json, required: &[&str]) -> Json {
    json!({ "type": "object", "properties": properties, "required": required })
}

impl Server {
    pub fn new(path: PathBuf, agent: impl Into<String>) -> Self {
        Self {
            path,
            agent: agent.into(),
        }
    }

    fn open(&self) -> Result<Session, String> {
        Session::open(&self.path).map_err(|e| e.to_string())
    }

    /// The tool list (names, descriptions, JSON schemas).
    pub fn tools() -> Json {
        let screen =
            json!({ "type": "string", "description": "screen id (default: the first screen)" });
        json!([
            tool(
                "get_document",
                "The design: an outline (screens, layers, components with ids and properties), the full JSON, or the YAML hand-off with solved geometry.",
                &object(
                    &json!({ "format": { "type": "string", "enum": ["outline", "json", "handoff"] } }),
                    &[]
                ),
            ),
            tool(
                "query_components",
                "Components matching a kind and/or text, with screen, layer, properties and layout.",
                &object(
                    &json!({
                        "kind": { "type": "string" },
                        "text": { "type": "string", "description": "matches ids and text properties" }
                    }),
                    &[]
                ),
            ),
            tool(
                "list_kinds",
                "Component kinds with their typed properties, defaults and initial values.",
                &object(&json!({ "kind": { "type": "string" } }), &[]),
            ),
            tool(
                "create_component",
                "Adds a component (with its kind's initial properties) to an interface layer or inside a parent. Returns its id.",
                &object(
                    &json!({
                        "kind": { "type": "string" },
                        "id": { "type": "string" },
                        "parent": { "type": "string" },
                        "layer": { "type": "string" },
                        "props": { "type": "object" },
                        "layout": { "type": "object", "description": "layout object, e.g. {\"width\": \"fixed(20)\", \"placement\": {\"mode\": \"absolute\", \"x\": 2, \"y\": 1}}" },
                        "propose": { "type": "boolean", "description": "leave it for review instead of saving" }
                    }),
                    &["kind"]
                ),
            ),
            tool(
                "apply_patch",
                "Applies a patch ({\"operations\": [...]}, the same operations as `glyphforge apply`) as one undoable agent transaction, or proposes it for review.",
                &object(
                    &json!({
                        "patch": { "type": "object" },
                        "message": { "type": "string" },
                        "propose": { "type": "boolean" }
                    }),
                    &["patch"]
                ),
            ),
            tool(
                "render_preview",
                "The screen as text (or ANSI) at its size or another size, to check a change.",
                &object(
                    &json!({
                        "screen": screen,
                        "width": { "type": "integer" },
                        "height": { "type": "integer" },
                        "format": { "type": "string", "enum": ["text", "ansi", "ansi256", "ansi16"] }
                    }),
                    &[]
                ),
            ),
            tool(
                "validate_layout",
                "Problems (clipped, truncated, overlap, invisible, unreachable, ...) at the screens' sizes or a given size, optionally with design suggestions.",
                &object(
                    &json!({
                        "screen": screen,
                        "width": { "type": "integer" },
                        "height": { "type": "integer" },
                        "advice": { "type": "boolean" }
                    }),
                    &[]
                ),
            ),
            tool(
                "export_document",
                "A screen as text, ANSI art, the YAML hand-off, or starter code for ratatui, textual, bubbletea or ink.",
                &object(
                    &json!({
                        "format": { "type": "string", "enum": ["text", "ansi", "ansi256", "ansi16", "handoff", "ratatui", "textual", "bubbletea", "ink"] },
                        "screen": screen
                    }),
                    &["format"]
                ),
            ),
        ])
    }

    /// Handles one JSON-RPC message; `None` for notifications.
    pub fn handle(&mut self, message: &Json) -> Option<Json> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Json::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Json::Null);
        let id = id?;
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "glyphforge", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Glyphforge designs terminal user interfaces as semantic components. Read with get_document, change with create_component or apply_patch (ids, not coordinates), check with render_preview and validate_layout."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": Self::tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(Json::as_str).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let (text, is_error) = match self.call(name, &args) {
                    Ok(t) => (t, false),
                    Err(e) => (e, true),
                };
                Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
            }
            other => Err(json!({ "code": -32601, "message": format!("unknown method {other}") })),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
        })
    }

    fn call(&mut self, name: &str, args: &Json) -> ToolResult {
        let s = |k: &str| args.get(k).and_then(Json::as_str);
        let n = |k: &str| {
            args.get(k)
                .and_then(Json::as_u64)
                .and_then(|v| u16::try_from(v).ok())
        };
        let size = match (n("width"), n("height")) {
            (Some(w), Some(h)) => Some(Size::new(w, h)),
            _ => None,
        };
        match name {
            "get_document" => {
                let session = self.open()?;
                Ok(match s("format").unwrap_or("outline") {
                    "json" => session.to_json().map_err(|e| e.to_string())?,
                    "handoff" => handoff::handoff(session.document(), session.registry()),
                    _ => outline::outline(session.document()),
                })
            }
            "query_components" => {
                let session = self.open()?;
                let kind = s("kind");
                let text = s("text").map(str::to_lowercase);
                let doc = session.document();
                let found: Vec<Json> = doc
                    .screens()
                    .iter()
                    .flat_map(|screen| screen.layers().iter().map(move |l| (screen, l)))
                    .filter_map(|(screen, layer)| layer.components().map(|roots| (screen, layer, roots)))
                    .flat_map(|(screen, layer, roots)| {
                        roots.iter().flat_map(Component::iter).map(move |c| (screen, layer, c))
                    })
                    .filter(|(_, _, c)| kind.is_none_or(|k| c.kind == k))
                    .filter(|(_, _, c)| {
                        text.as_ref().is_none_or(|t| {
                            c.id.as_str().contains(t.as_str())
                                || c.props.values().any(|v| v.to_string().to_lowercase().contains(t.as_str()))
                        })
                    })
                    .map(|(screen, layer, c)| {
                        json!({
                            "id": c.id, "kind": c.kind, "screen": screen.id, "layer": layer.id,
                            "props": c.props, "layout": c.layout,
                            "children": c.children.iter().map(|k| k.id.to_string()).collect::<Vec<_>>()
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&found).map_err(|e| e.to_string())
            }
            "list_kinds" => {
                let registry = glyphforge_core::render::Registry::builtin();
                let mut all = registry.descriptors();
                if let Some(k) = s("kind") {
                    all.retain(|d| d.kind == k);
                }
                serde_json::to_string_pretty(&all).map_err(|e| e.to_string())
            }
            "create_component" => self.create_component(args),
            "apply_patch" => {
                let patch: Patch =
                    serde_json::from_value(args.get("patch").cloned().unwrap_or_default())
                        .map_err(|e| format!("not a patch: {e}"))?;
                self.change(
                    patch,
                    s("message").unwrap_or(""),
                    args.get("propose").and_then(Json::as_bool) == Some(true),
                )
            }
            "render_preview" => {
                let session = self.open()?;
                let screen = Self::screen(&session, s("screen"))?;
                match s("format").and_then(ansi::Depth::parse) {
                    Some(depth) => Ok(ansi::to_ansi(
                        &session.render(&screen, size).map_err(|e| e.to_string())?,
                        depth,
                    )),
                    None => session
                        .render_text(&screen, size)
                        .map_err(|e| e.to_string()),
                }
            }
            "validate_layout" => {
                let session = self.open()?;
                let mut lines: Vec<String> =
                    session.validate().iter().map(ToString::to_string).collect();
                if let Some(size) = size {
                    let screen = Self::screen(&session, s("screen"))?;
                    let extra = session
                        .validate_at(&screen, size)
                        .map_err(|e| e.to_string())?;
                    lines.extend(extra.iter().map(ToString::to_string));
                }
                if args.get("advice").and_then(Json::as_bool) == Some(true) {
                    lines.extend(
                        glyphforge_core::advice::advise(session.document(), session.registry())
                            .iter()
                            .map(ToString::to_string),
                    );
                }
                Ok(if lines.is_empty() {
                    "ok: no problems found".to_owned()
                } else {
                    lines.join("\n")
                })
            }
            "export_document" => {
                let session = self.open()?;
                let screen = Self::screen(&session, s("screen"))?;
                let format = s("format").unwrap_or("text");
                if format == "text" {
                    return session
                        .render_text(&screen, None)
                        .map_err(|e| e.to_string());
                }
                if format == "handoff" {
                    return Ok(handoff::handoff(session.document(), session.registry()));
                }
                if let Some(depth) = ansi::Depth::parse(format) {
                    return Ok(ansi::to_ansi(
                        &session.render(&screen, None).map_err(|e| e.to_string())?,
                        depth,
                    ));
                }
                let exporter =
                    codegen::exporter(format).ok_or_else(|| format!("unknown format {format}"))?;
                let s = session.document().screen(&screen).ok_or("no such screen")?;
                Ok(exporter.generate(session.document(), s))
            }
            other => Err(format!("unknown tool {other}")),
        }
    }

    fn screen(session: &Session, id: Option<&str>) -> Result<ObjectId, String> {
        match id {
            Some(s) => ObjectId::new(s).map_err(|e| e.to_string()),
            None => Ok(session.document().first_screen().id.clone()),
        }
    }

    fn create_component(&mut self, args: &Json) -> ToolResult {
        let session = self.open()?;
        let s = |k: &str| args.get(k).and_then(Json::as_str);
        let kind = s("kind").ok_or("kind is required")?;
        let registry = session.registry();
        let descriptor = registry.descriptor(kind);
        let ids = session.document().ids();
        let id = ObjectId::slugify(s("id").unwrap_or(kind)).unique_among(|i| ids.contains(i));
        let mut c = Component::new(id.clone(), kind);
        if let Some(d) = &descriptor {
            for (k, v) in d.initial_props() {
                c = c.with_prop(k, v);
            }
        }
        if let Some(props) = args.get("props").and_then(Json::as_object) {
            for (k, v) in props {
                let value: glyphforge_core::Value =
                    serde_json::from_value(v.clone()).map_err(|e| format!("{k}: {e}"))?;
                c.props.insert(k.clone(), value);
            }
        }
        if let Some(layout) = args.get("layout") {
            c.layout =
                serde_json::from_value(layout.clone()).map_err(|e| format!("layout: {e}"))?;
        }
        let parent = s("parent")
            .map(ObjectId::new)
            .transpose()
            .map_err(|e| e.to_string())?;
        let layer = match (s("layer"), &parent) {
            (Some(l), _) => Some(ObjectId::new(l).map_err(|e| e.to_string())?),
            (None, Some(_)) => None,
            (None, None) => session
                .document()
                .first_screen()
                .layers()
                .iter()
                .rev()
                .find(|l| l.components().is_some())
                .map(|l| l.id.clone()),
        };
        let patch = Patch::single(Operation::CreateComponent {
            component: c,
            parent,
            layer,
            index: None,
        });
        let message = format!("add {kind} {id}");
        let propose = args.get("propose").and_then(Json::as_bool) == Some(true);
        self.change(patch, &message, propose)
            .map(|r| format!("{r}\nid: {id}"))
    }

    /// Applies and saves `patch` as an agent transaction, or validates it
    /// and stores it as a proposal.
    fn change(&mut self, patch: Patch, message: &str, propose: bool) -> ToolResult {
        let mut session = self.open()?;
        let count = patch.operations.len();
        let origin = Origin::Agent {
            name: self.agent.clone(),
            description: message.to_owned(),
        };
        session
            .apply_patch(patch.clone(), message, origin)
            .map_err(|e| e.to_string())?;
        if propose {
            let p = Proposal {
                agent: self.agent.clone(),
                message: message.to_owned(),
                created: proposal::now(),
                patch,
            };
            let path = proposal::submit(&self.path, &p).map_err(|e| e.to_string())?;
            return Ok(format!(
                "proposed {count} operation(s) for review: {}",
                path.display()
            ));
        }
        session.save().map_err(|e| e.to_string())?;
        Ok(format!(
            "applied {count} operation(s) and saved {}",
            self.path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str) -> (Server, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("glyphforge-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("d.glyph");
        let mut session = Session::new(glyphforge_core::Document::new(Size::new(30, 6)).unwrap());
        session.save_as(&file).unwrap();
        (Server::new(file, "test-agent"), dir)
    }

    fn call(server: &mut Server, name: &str, args: &Json) -> (String, bool) {
        let reply = server
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }))
            .unwrap();
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_owned(),
            result["isError"].as_bool().unwrap(),
        )
    }

    #[test]
    fn handshake_lists_tools() {
        let (mut server, dir) = server("handshake");
        let init = server
            .handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }))
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], PROTOCOL);
        assert!(
            server
                .handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .is_none()
        );
        let list = server
            .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
            .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for expected in [
            "get_document",
            "query_components",
            "create_component",
            "apply_patch",
            "render_preview",
            "validate_layout",
            "export_document",
        ] {
            assert!(names.contains(&expected), "{expected}");
        }
        let unknown = server
            .handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" }))
            .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn create_query_render_validate_and_propose() {
        let (mut server, dir) = server("tools");
        let (text, err) = call(
            &mut server,
            "create_component",
            &json!({ "kind": "button", "id": "save", "props": { "label": "Save" },
                    "layout": { "placement": { "mode": "absolute", "x": 1, "y": 1 } } }),
        );
        assert!(!err, "{text}");
        assert!(text.contains("id: save"));
        let (found, _) = call(
            &mut server,
            "query_components",
            &json!({ "kind": "button" }),
        );
        assert!(found.contains("\"save\""), "{found}");
        let (preview, _) = call(&mut server, "render_preview", &json!({}));
        assert!(preview.contains("[ Save ]"), "{preview}");
        let (problems, _) = call(&mut server, "validate_layout", &json!({ "advice": true }));
        assert!(problems.contains("suggestion"), "{problems}");
        let (code, _) = call(
            &mut server,
            "export_document",
            &json!({ "format": "textual" }),
        );
        assert!(code.contains("Button(\"Save\""), "{code}");
        // A proposal is stored, not applied.
        let (text, err) = call(
            &mut server,
            "apply_patch",
            &json!({ "patch": { "operations": [{ "op": "delete_component", "target": "save" }] },
                    "message": "remove save", "propose": true }),
        );
        assert!(!err && text.contains("proposed"), "{text}");
        let (outline, _) = call(&mut server, "get_document", &json!({}));
        assert!(outline.contains("save"), "still there: {outline}");
        // Errors come back as tool errors.
        let (text, err) = call(
            &mut server,
            "apply_patch",
            &json!({ "patch": { "operations": [{ "op": "delete_component", "target": "ghost" }] } }),
        );
        assert!(err, "{text}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
