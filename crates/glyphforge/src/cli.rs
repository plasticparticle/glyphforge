//! Command line interface: the interactive editor by default, headless
//! subcommands for scripts and coding agents.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "glyphforge",
    version,
    about = "Visual design environment for terminal user interfaces (a full-screen TUI)",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Project file to open (.glyph). A path that does not exist yet
    /// becomes the document's save target.
    pub file: Option<PathBuf>,

    /// Canvas width for a new document (overrides the config).
    #[arg(long, value_name = "COLS")]
    pub width: Option<u16>,

    /// Canvas height for a new document (overrides the config).
    #[arg(long, value_name = "ROWS")]
    pub height: Option<u16>,

    /// Use an alternative config file.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Print detected terminal capabilities, paths and Omarchy status, then exit.
    #[arg(long)]
    pub print_caps: bool,

    /// Print the default configuration as TOML, then exit.
    #[arg(long)]
    pub print_default_config: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the semantic structure of a project (screens, layers, components).
    Inspect {
        file: PathBuf,
        /// Print the full project JSON instead of the outline.
        #[arg(long)]
        json: bool,
    },
    /// Render a screen as plain text.
    Render {
        file: PathBuf,
        /// Screen id (default: the first screen).
        #[arg(long)]
        screen: Option<String>,
        /// Preview width (default: the screen's own size).
        #[arg(long, requires = "height")]
        width: Option<u16>,
        #[arg(long, requires = "width")]
        height: Option<u16>,
    },
    /// Validate layout and report problems; exits 1 when errors exist.
    Validate {
        file: PathBuf,
        /// Also validate the first screen at this width (with --height).
        #[arg(long, requires = "height")]
        width: Option<u16>,
        #[arg(long, requires = "width")]
        height: Option<u16>,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Apply a semantic patch (JSON) to a project and save it.
    Apply {
        file: PathBuf,
        patch: PathBuf,
        /// Write the result here instead of overwriting the input.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Apply and print the rendered result without saving.
        #[arg(long)]
        dry_run: bool,
    },
    /// Export a screen; formats: text.
    Export {
        file: PathBuf,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        screen: Option<String>,
        /// Output file (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Create a new empty project file.
    New {
        file: PathBuf,
        #[arg(long, default_value_t = 80)]
        width: u16,
        #[arg(long, default_value_t = 24)]
        height: u16,
    },
}
