//! Omarchy Linux integration.
//!
//! Only public, stable Omarchy interfaces are used, as observed in the
//! Omarchy repository (`bin/omarchy-theme-set`, `bin/omarchy-theme-current`,
//! `bin/omarchy-hook`):
//!
//! - `~/.config/omarchy/current/theme.name` holds the active theme's name;
//! - `~/.config/omarchy/current/theme/colors.toml` holds its colours;
//! - `$OMARCHY_PATH` (default `~/.local/share/omarchy`) is the install.
//!
//! Nothing here writes to Omarchy files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glyphforge_core::boxdraw::BorderFamily;
use glyphforge_core::theme::Spacing;
use glyphforge_core::{Color, ObjectId, Theme};

/// Errors while reading the Omarchy theme.
#[derive(Debug, thiserror::Error)]
pub enum OmarchyError {
    #[error("Omarchy theme file {path} not found")]
    Missing { path: PathBuf },
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not valid TOML: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{path} lacks required colour {key:?}")]
    MissingColor { path: PathBuf, key: &'static str },
    #[error("{path}: colour {key:?} has invalid value {value:?}")]
    InvalidColor {
        path: PathBuf,
        key: String,
        value: String,
    },
}

/// Locations of the Omarchy user state, derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmarchyPaths {
    /// `$OMARCHY_PATH` or `$XDG_DATA_HOME/omarchy`.
    pub install_dir: PathBuf,
    /// `~/.config/omarchy` (Omarchy hard-codes this, it does not honour
    /// `XDG_CONFIG_HOME`).
    pub config_dir: PathBuf,
}

impl OmarchyPaths {
    pub fn from_env() -> Self {
        let vars: HashMap<String, String> = std::env::vars().collect();
        Self::from_vars(&vars)
    }

    pub fn from_vars(vars: &HashMap<String, String>) -> Self {
        let home = vars
            .get("HOME")
            .map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let install_dir = vars
            .get("OMARCHY_PATH")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                vars.get("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join("omarchy"))
            })
            .unwrap_or_else(|| home.join(".local/share/omarchy"));
        Self {
            install_dir,
            config_dir: home.join(".config/omarchy"),
        }
    }

    pub fn theme_name_file(&self) -> PathBuf {
        self.config_dir.join("current/theme.name")
    }

    pub fn theme_colors_file(&self) -> PathBuf {
        self.config_dir.join("current/theme/colors.toml")
    }

    /// Whether this looks like an Omarchy system: the install directory or
    /// the current-theme marker exists.
    pub fn is_omarchy(&self) -> bool {
        self.install_dir.is_dir() || self.theme_name_file().is_file()
    }
}

/// The parsed `colors.toml` of an Omarchy theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmarchyColors {
    pub accent: Color,
    pub cursor: Color,
    pub foreground: Color,
    pub background: Color,
    pub selection_foreground: Color,
    pub selection_background: Color,
    pub palette: [Color; 16],
}

impl OmarchyColors {
    /// Parses the `colors.toml` format: flat `key = "#rrggbb"` pairs.
    /// `foreground` and `background` are required; everything else has a
    /// fallback so a partially specified theme still loads.
    pub fn parse(text: &str, path: &Path) -> Result<Self, OmarchyError> {
        let table: HashMap<String, toml::Value> =
            toml::from_str(text).map_err(|source| OmarchyError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        let get = |key: &str| -> Result<Option<Color>, OmarchyError> {
            match table.get(key) {
                None => Ok(None),
                Some(v) => {
                    let s = v.as_str().unwrap_or_default();
                    Color::from_hex(s)
                        .map(Some)
                        .ok_or_else(|| OmarchyError::InvalidColor {
                            path: path.to_path_buf(),
                            key: key.to_owned(),
                            value: s.to_owned(),
                        })
                }
            }
        };
        let required = |key: &'static str| -> Result<Color, OmarchyError> {
            get(key)?.ok_or(OmarchyError::MissingColor {
                path: path.to_path_buf(),
                key,
            })
        };
        let foreground = required("foreground")?;
        let background = required("background")?;
        let mut palette = [Color::Default; 16];
        for (i, slot) in palette.iter_mut().enumerate() {
            *slot = get(&format!("color{i}"))?.unwrap_or(Color::Indexed(i as u8));
        }
        let accent = get("accent")?.unwrap_or(palette[4]);
        Ok(Self {
            accent,
            cursor: get("cursor")?.unwrap_or(foreground),
            foreground,
            background,
            selection_foreground: get("selection_foreground")?.unwrap_or(background),
            selection_background: get("selection_background")?.unwrap_or(accent),
            palette,
        })
    }

    /// Maps Omarchy's semantic colours onto Glyphforge's style tokens.
    /// Nothing is hard-coded to one Omarchy theme: every token derives from
    /// the theme's own colours.
    pub fn to_theme(&self, name: &str) -> Theme {
        let muted = match self.palette[8] {
            Color::Rgb(..) => self.palette[8],
            _ => self.background.mix(self.foreground, 0.55),
        };
        let pairs = [
            ("background", self.background),
            ("surface", self.background.mix(self.foreground, 0.06)),
            ("surface-alt", self.background.mix(self.foreground, 0.12)),
            ("border", muted),
            ("border-muted", self.background.mix(self.foreground, 0.25)),
            ("foreground", self.foreground),
            ("foreground-muted", muted),
            ("primary", self.accent),
            ("secondary", self.palette[5]),
            ("success", self.palette[2]),
            ("warning", self.palette[3]),
            ("error", self.palette[1]),
            ("info", self.palette[4]),
            ("selection", self.selection_background),
            ("selection-foreground", self.selection_foreground),
            ("focus", self.cursor),
        ];
        Theme {
            id: ObjectId::slugify(&format!("omarchy-{name}")),
            name: name.to_owned(),
            colors: pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
            border: BorderFamily::Rounded,
            spacing: Spacing::default(),
        }
    }
}

/// Converts Omarchy's `theme.name` slug into a display name, like
/// `omarchy-theme-current` does (`tokyo-night` -> `Tokyo Night`).
pub fn display_name(slug: &str) -> String {
    slug.trim()
        .split('-')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Loads the active Omarchy theme as a Glyphforge [`Theme`].
pub fn load_theme(paths: &OmarchyPaths) -> Result<Theme, OmarchyError> {
    let colors_path = paths.theme_colors_file();
    let text = std::fs::read_to_string(&colors_path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            OmarchyError::Missing {
                path: colors_path.clone(),
            }
        } else {
            OmarchyError::Read {
                path: colors_path.clone(),
                source,
            }
        }
    })?;
    let colors = OmarchyColors::parse(&text, &colors_path)?;
    let name = std::fs::read_to_string(paths.theme_name_file())
        .map_or_else(|_| "Omarchy".to_owned(), |s| display_name(&s));
    Ok(colors.to_theme(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKYO_NIGHT: &str = r##"
accent = "#7aa2f7"
cursor = "#c0caf5"
foreground = "#a9b1d6"
background = "#1a1b26"
selection_foreground = "#c0caf5"
selection_background = "#7aa2f7"

color0 = "#32344a"
color1 = "#f7768e"
color2 = "#9ece6a"
color3 = "#e0af68"
color4 = "#7aa2f7"
color5 = "#ad8ee6"
color6 = "#449dab"
color7 = "#787c99"
color8 = "#444b6a"
color9 = "#ff7a93"
color10 = "#b9f27c"
color11 = "#ff9e64"
color12 = "#7da6ff"
color13 = "#bb9af7"
color14 = "#0db9d7"
color15 = "#acb0d0"
"##;

    #[test]
    fn parses_full_colors_toml_into_complete_theme() {
        let c = OmarchyColors::parse(TOKYO_NIGHT, Path::new("colors.toml")).unwrap();
        assert_eq!(c.background, Color::Rgb(0x1a, 0x1b, 0x26));
        assert_eq!(c.palette[1], Color::Rgb(0xf7, 0x76, 0x8e));
        let t = c.to_theme("Tokyo Night");
        assert!(t.missing_tokens().is_empty());
        assert_eq!(t.color("error"), c.palette[1]);
        assert_eq!(t.color("warning"), c.palette[3]);
        assert_eq!(t.color("success"), c.palette[2]);
        assert_eq!(t.color("primary"), c.accent);
        assert_eq!(t.color("foreground-muted"), c.palette[8]);
        assert_ne!(t.color("surface"), t.color("background"));
        assert_eq!(t.name, "Tokyo Night");
        assert_eq!(t.id.as_str(), "omarchy-tokyo-night");
    }

    #[test]
    fn partial_theme_uses_fallbacks() {
        let c = OmarchyColors::parse(
            "foreground = \"#ffffff\"\nbackground = \"#000000\"\n",
            Path::new("x"),
        )
        .unwrap();
        assert_eq!(c.accent, Color::Indexed(4));
        assert_eq!(c.cursor, c.foreground);
        assert_eq!(c.selection_background, c.accent);
        let t = c.to_theme("Minimal");
        assert!(matches!(t.color("foreground-muted"), Color::Rgb(..)));
    }

    #[test]
    fn missing_required_colour_is_an_error() {
        let err = OmarchyColors::parse("foreground = \"#ffffff\"\n", Path::new("x")).unwrap_err();
        assert!(matches!(
            err,
            OmarchyError::MissingColor {
                key: "background",
                ..
            }
        ));
    }

    #[test]
    fn invalid_colour_value_is_an_error() {
        let err = OmarchyColors::parse(
            "foreground = \"white\"\nbackground = \"#000000\"\n",
            Path::new("x"),
        )
        .unwrap_err();
        assert!(matches!(err, OmarchyError::InvalidColor { .. }));
    }

    #[test]
    fn display_name_matches_omarchy_theme_current() {
        assert_eq!(display_name("tokyo-night\n"), "Tokyo Night");
        assert_eq!(display_name("catppuccin-latte"), "Catppuccin Latte");
        assert_eq!(display_name("nord"), "Nord");
    }

    #[test]
    fn paths_prefer_omarchy_path_env() {
        let mut vars = HashMap::new();
        vars.insert("HOME".to_owned(), "/home/u".to_owned());
        let p = OmarchyPaths::from_vars(&vars);
        assert_eq!(p.install_dir, PathBuf::from("/home/u/.local/share/omarchy"));
        assert_eq!(
            p.theme_colors_file(),
            PathBuf::from("/home/u/.config/omarchy/current/theme/colors.toml")
        );
        vars.insert("OMARCHY_PATH".to_owned(), "/opt/omarchy".to_owned());
        assert_eq!(
            OmarchyPaths::from_vars(&vars).install_dir,
            PathBuf::from("/opt/omarchy")
        );
    }

    #[test]
    fn load_theme_from_fixture_directory() {
        let dir = std::env::temp_dir().join(format!("glyphforge-omarchy-{}", std::process::id()));
        let cfg = dir.join(".config/omarchy/current/theme");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(cfg.join("colors.toml"), TOKYO_NIGHT).unwrap();
        std::fs::write(
            dir.join(".config/omarchy/current/theme.name"),
            "tokyo-night\n",
        )
        .unwrap();
        let paths = OmarchyPaths {
            install_dir: dir.join("nope"),
            config_dir: dir.join(".config/omarchy"),
        };
        assert!(paths.is_omarchy());
        let theme = load_theme(&paths).unwrap();
        assert_eq!(theme.name, "Tokyo Night");
        assert_eq!(theme.color("primary"), Color::Rgb(0x7a, 0xa2, 0xf7));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_theme_is_reported() {
        let paths = OmarchyPaths {
            install_dir: "/nonexistent".into(),
            config_dir: "/nonexistent".into(),
        };
        assert!(!paths.is_omarchy());
        assert!(matches!(
            load_theme(&paths),
            Err(OmarchyError::Missing { .. })
        ));
    }
}
