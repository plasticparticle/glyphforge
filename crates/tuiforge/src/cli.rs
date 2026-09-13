//! Command line interface.

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "tuiforge",
    version,
    about = "Visual terminal UI and ANSI/Unicode art editor"
)]
pub struct Cli {
    /// Project file to open (.tuiforge). A path that does not exist yet
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
