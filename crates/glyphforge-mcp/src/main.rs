//! `glyphforge-mcp FILE [--agent NAME]`: a Model Context Protocol server
//! (stdio, newline-delimited JSON-RPC) that lets coding agents read,
//! change, preview, validate and export a Glyphforge design.

mod server;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut file = None;
    let mut agent = "mcp-agent".to_owned();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--agent" => agent = args.next().unwrap_or(agent),
            "-h" | "--help" => {
                println!("usage: glyphforge-mcp FILE.glyph [--agent NAME]");
                return ExitCode::SUCCESS;
            }
            _ => file = Some(PathBuf::from(arg)),
        }
    }
    let Some(file) = file else {
        eprintln!("usage: glyphforge-mcp FILE.glyph [--agent NAME]");
        return ExitCode::from(2);
    };
    if !file.exists() {
        eprintln!(
            "{} does not exist (create it with `glyphforge new`)",
            file.display()
        );
        return ExitCode::from(2);
    }
    let mut server = server::Server::new(file, agent);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(message) => server.handle(&message),
            Err(e) => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") }
            })),
        };
        if let Some(reply) = reply {
            if writeln!(stdout, "{reply}")
                .and_then(|()| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    }
    ExitCode::SUCCESS
}
