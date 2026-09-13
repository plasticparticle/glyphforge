//! XDG Base Directory resolution.

use std::collections::HashMap;
use std::path::PathBuf;

/// Application directories resolved from the XDG Base Directory
/// specification. Every directory already has the `tuiforge` suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
}

pub const APP_DIR_NAME: &str = "tuiforge";

impl AppDirs {
    /// Resolves the directories from the process environment.
    pub fn from_env() -> Self {
        let vars: HashMap<String, String> = std::env::vars().collect();
        Self::from_vars(&vars)
    }

    /// Resolves the directories from an explicit variable map. Relative
    /// XDG values are ignored, as the specification requires.
    pub fn from_vars(vars: &HashMap<String, String>) -> Self {
        let home = vars
            .get("HOME")
            .map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let pick = |var: &str, default: &[&str]| -> PathBuf {
            match vars.get(var).map(PathBuf::from) {
                Some(p) if p.is_absolute() => p.join(APP_DIR_NAME),
                _ => default
                    .iter()
                    .fold(home.clone(), |acc, seg| acc.join(seg))
                    .join(APP_DIR_NAME),
            }
        };
        Self {
            config: pick("XDG_CONFIG_HOME", &[".config"]),
            data: pick("XDG_DATA_HOME", &[".local", "share"]),
            state: pick("XDG_STATE_HOME", &[".local", "state"]),
            cache: pick("XDG_CACHE_HOME", &[".cache"]),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn log_file(&self) -> PathBuf {
        self.state.join("tuiforge.log")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_relative_to_home() {
        let dirs = AppDirs::from_vars(&vars(&[("HOME", "/home/lars")]));
        assert_eq!(dirs.config, PathBuf::from("/home/lars/.config/tuiforge"));
        assert_eq!(dirs.data, PathBuf::from("/home/lars/.local/share/tuiforge"));
        assert_eq!(
            dirs.state,
            PathBuf::from("/home/lars/.local/state/tuiforge")
        );
        assert_eq!(dirs.cache, PathBuf::from("/home/lars/.cache/tuiforge"));
        assert_eq!(
            dirs.config_file(),
            PathBuf::from("/home/lars/.config/tuiforge/config.toml")
        );
    }

    #[test]
    fn explicit_xdg_variables_win() {
        let dirs = AppDirs::from_vars(&vars(&[
            ("HOME", "/home/lars"),
            ("XDG_CONFIG_HOME", "/etc/xdg-user"),
            ("XDG_STATE_HOME", "/var/state"),
        ]));
        assert_eq!(dirs.config, PathBuf::from("/etc/xdg-user/tuiforge"));
        assert_eq!(dirs.state, PathBuf::from("/var/state/tuiforge"));
        assert_eq!(dirs.data, PathBuf::from("/home/lars/.local/share/tuiforge"));
    }

    #[test]
    fn relative_xdg_values_are_ignored() {
        let dirs = AppDirs::from_vars(&vars(&[
            ("HOME", "/home/lars"),
            ("XDG_CACHE_HOME", "relative/dir"),
        ]));
        assert_eq!(dirs.cache, PathBuf::from("/home/lars/.cache/tuiforge"));
    }
}
