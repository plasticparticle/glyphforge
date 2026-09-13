//! User configuration (TOML, XDG located) with defaults for every field.

pub mod paths;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use paths::AppDirs;

/// Errors when loading the configuration file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("config file {path} is not valid TOML: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub canvas: CanvasConfig,
    pub ui: UiConfig,
    pub theme: ThemeConfig,
    pub mouse: MouseConfig,
    pub autosave: AutosaveConfig,
    /// Key chord -> action name, e.g. `"ctrl+q" = "quit"`. Overrides the
    /// built-in bindings entry by entry. Binding a chord to `"none"`
    /// removes it.
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CanvasConfig {
    pub default_width: u16,
    pub default_height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub show_left_panel: bool,
    pub show_right_panel: bool,
    pub vim_navigation: bool,
    /// Preferred colour output: `auto`, `truecolor`, `ansi256`, `ansi16`.
    pub color_mode: ColorMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Auto,
    TrueColor,
    Ansi256,
    Ansi16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    /// `auto` uses the Omarchy theme when running under Omarchy and the
    /// built-in theme otherwise; `omarchy` and `builtin` force one source.
    pub source: ThemeSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSource {
    #[default]
    Auto,
    Omarchy,
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MouseConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutosaveConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            default_width: 80,
            default_height: 24,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_left_panel: true,
            show_right_panel: true,
            vim_navigation: false,
            color_mode: ColorMode::Auto,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            source: ThemeSource::Auto,
        }
    }
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for AutosaveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 60,
        }
    }
}

impl Config {
    /// Parses a TOML document.
    pub fn parse(text: &str, path_for_errors: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|source| ConfigError::Parse {
            path: path_for_errors.into(),
            source,
        })
    }

    /// Loads the file at `path`. A missing file yields the defaults; any
    /// other problem is an error so the caller can report it.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, &path.display().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.display().to_string(),
                source,
            }),
        }
    }

    /// The default configuration rendered as TOML, for documentation and
    /// `--print-default-config`.
    pub fn default_toml() -> String {
        toml::to_string_pretty(&Self::default()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_is_all_defaults() {
        assert_eq!(Config::parse("", "x").unwrap(), Config::default());
    }

    #[test]
    fn partial_sections_keep_other_defaults() {
        let cfg = Config::parse(
            "[canvas]\ndefault_width = 120\n[keys]\n\"ctrl+q\" = \"quit\"\n",
            "x",
        )
        .unwrap();
        assert_eq!(cfg.canvas.default_width, 120);
        assert_eq!(cfg.canvas.default_height, 24);
        assert!(cfg.ui.show_left_panel);
        assert_eq!(cfg.keys.get("ctrl+q").map(String::as_str), Some("quit"));
    }

    #[test]
    fn unknown_fields_are_errors() {
        assert!(matches!(
            Config::parse("[canvas]\nwidth = 1\n", "x"),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn enums_use_lowercase_names() {
        let cfg = Config::parse(
            "[theme]\nsource = \"builtin\"\n[ui]\ncolor_mode = \"ansi256\"\n",
            "x",
        )
        .unwrap();
        assert_eq!(cfg.theme.source, ThemeSource::Builtin);
        assert_eq!(cfg.ui.color_mode, ColorMode::Ansi256);
    }

    #[test]
    fn default_toml_round_trips() {
        let text = Config::default_toml();
        assert_eq!(Config::parse(&text, "x").unwrap(), Config::default());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = Config::load(Path::new("/nonexistent/tuiforge/config.toml")).unwrap();
        assert_eq!(cfg, Config::default());
    }
}
