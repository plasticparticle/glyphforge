//! TUIForge: a full-screen terminal editor for terminal user interfaces and
//! ANSI/Unicode artwork.

mod actions;
mod app;
mod cli;
mod config;
mod input;
mod omarchy;
mod render;
mod terminal;
mod theme;
mod ui;

use std::fs::OpenOptions;
use std::process::ExitCode;
use std::sync::Mutex;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use tuiforge_core::{Document, Size};

use crate::app::{App, AppInit};
use crate::cli::Cli;
use crate::config::{AppDirs, Config};
use crate::input::Keymap;
use crate::omarchy::OmarchyPaths;
use crate::terminal::{TerminalCapabilities, TerminalGuard};

/// Largest canvas edge we accept; keeps memory bounded (2000x2000 cells).
const MAX_CANVAS_EDGE: u16 = 2000;

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("canvas size {width}x{height} is invalid; both sides must be between 1 and {max}")]
    CanvasSize { width: u16, height: u16, max: u16 },
    #[error("opening project files is not implemented yet (planned for Milestone 10): {0}")]
    OpenNotImplemented(String),
    #[error(transparent)]
    Terminal(#[from] terminal::TerminalError),
    #[error(transparent)]
    Run(#[from] app::RunError),
    #[error(transparent)]
    Document(#[from] tuiforge_core::DocumentError),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let dirs = AppDirs::from_env();

    if cli.print_default_config {
        print!("{}", Config::default_toml());
        return ExitCode::SUCCESS;
    }

    let mut warnings = Vec::new();
    let config_path = cli.config.clone().unwrap_or_else(|| dirs.config_file());
    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            warnings.push(format!("Config ignored: {e}"));
            Config::default()
        }
    };

    let omarchy_paths = OmarchyPaths::from_env();

    if cli.print_caps {
        let caps = TerminalCapabilities::infer(|k| std::env::var(k).ok(), false);
        println!("{caps}");
        println!("(keyboard enhancement is probed only inside the editor)");
        println!("config file:          {}", config_path.display());
        println!("data dir:             {}", dirs.data.display());
        println!("state dir:            {}", dirs.state.display());
        println!("cache dir:            {}", dirs.cache.display());
        println!("omarchy detected:     {}", omarchy_paths.is_omarchy());
        println!(
            "omarchy theme file:   {}",
            omarchy_paths.theme_colors_file().display()
        );
        for w in &warnings {
            println!("warning: {w}");
        }
        return ExitCode::SUCCESS;
    }

    init_logging(&dirs, &mut warnings);

    match start(cli, config, omarchy_paths, warnings) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // The terminal guard has been dropped by now; stderr is safe.
            eprintln!("tuiforge: {e}");
            let mut source = std::error::Error::source(&e);
            while let Some(s) = source {
                eprintln!("  caused by: {s}");
                source = s.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn init_logging(dirs: &AppDirs, warnings: &mut Vec<String>) {
    let filter = EnvFilter::try_from_env("TUIFORGE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let path = dirs.log_file();
    let file = std::fs::create_dir_all(&dirs.state)
        .and_then(|()| OpenOptions::new().create(true).append(true).open(&path));
    match file {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
            tracing::info!("tuiforge {} starting", env!("CARGO_PKG_VERSION"));
        }
        Err(e) => warnings.push(format!(
            "Logging disabled, cannot open {}: {e}",
            path.display()
        )),
    }
}

fn start(
    cli: Cli,
    config: Config,
    omarchy_paths: OmarchyPaths,
    mut warnings: Vec<String>,
) -> Result<(), StartupError> {
    let width = cli.width.unwrap_or(config.canvas.default_width);
    let height = cli.height.unwrap_or(config.canvas.default_height);
    if !(1..=MAX_CANVAS_EDGE).contains(&width) || !(1..=MAX_CANVAS_EDGE).contains(&height) {
        return Err(StartupError::CanvasSize {
            width,
            height,
            max: MAX_CANVAS_EDGE,
        });
    }

    let doc_path = match cli.file {
        Some(p) if p.exists() => {
            return Err(StartupError::OpenNotImplemented(p.display().to_string()));
        }
        other => other,
    };
    let doc = Document::new(Size::new(width, height))?;

    let mut keymap = Keymap::defaults();
    for e in keymap.apply_overrides(&config.keys) {
        warnings.push(format!("Key binding ignored: {e}"));
    }

    terminal::install_panic_hook();
    let mut guard = TerminalGuard::enter(config.mouse.enabled)?;
    let caps = TerminalCapabilities::infer(|k| std::env::var(k).ok(), guard.keyboard_enhancement());
    tracing::info!(?caps, "terminal capabilities");

    let mut app = App::new(AppInit {
        doc,
        doc_path,
        config,
        caps,
        keymap,
        omarchy_paths,
        startup_warnings: warnings,
    });
    let result = app::run(&mut app, &mut guard);
    drop(guard);
    result?;
    Ok(())
}
