//! Agent proposals: patches waiting for review next to a project file.
//!
//! `glyphforge propose design.glyph patch.json` validates the patch against
//! the file and stores it in `design.glyph.proposals/` instead of applying
//! it. The editor lists the proposals, previews each one and applies
//! (accept) or deletes (reject) it; an accepted proposal becomes an agent
//! transaction in the history like any other patch.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::patch::Patch;

/// A patch an agent suggests, with who suggested it and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub agent: String,
    #[serde(default)]
    pub message: String,
    /// Seconds since the Unix epoch.
    #[serde(default)]
    pub created: u64,
    pub patch: Patch,
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("cannot write proposal {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot encode proposal: {0}")]
    Encode(#[from] serde_json::Error),
}

/// The directory proposals for `file` live in: `<file>.proposals`.
pub fn dir_for(file: &Path) -> PathBuf {
    let mut name = file
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(".proposals");
    file.with_file_name(name)
}

/// Stores a proposal; returns its path. Names sort by submission order.
pub fn submit(file: &Path, proposal: &Proposal) -> Result<PathBuf, ProposalError> {
    let dir = dir_for(file);
    let io = |path: &Path| {
        let path = path.display().to_string();
        move |source| ProposalError::Write { path, source }
    };
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let next = list(file)
        .iter()
        .filter_map(|(p, _)| {
            p.file_stem()?
                .to_str()?
                .split('-')
                .next()?
                .parse::<u32>()
                .ok()
        })
        .max()
        .map_or(1, |n| n + 1);
    let slug: String = proposal
        .agent
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let path = dir.join(format!("{next:04}-{}.json", slug.trim_matches('-')));
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(proposal)?;
    std::fs::write(&tmp, json + "\n").map_err(io(&tmp))?;
    std::fs::rename(&tmp, &path).map_err(io(&path))?;
    Ok(path)
}

/// Proposals for `file`, oldest first. Unreadable files are skipped.
pub fn list(file: &Path) -> Vec<(PathBuf, Proposal)> {
    let Ok(entries) = std::fs::read_dir(dir_for(file)) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, Proposal)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let proposal = serde_json::from_str(&text).ok()?;
            Some((p, proposal))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Deletes a proposal file (after accepting or rejecting it).
pub fn remove(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

/// Seconds since the Unix epoch, for `created`.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;
    use crate::patch::Operation;

    #[test]
    fn submit_list_and_remove() {
        let dir = std::env::temp_dir().join(format!("glyphforge-proposals-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("d.glyph");
        assert_eq!(dir_for(&file), dir.join("d.glyph.proposals"));
        assert!(list(&file).is_empty());
        let p = Proposal {
            agent: "Claude Code".into(),
            message: "tighten the sidebar".into(),
            created: now(),
            patch: Patch::single(Operation::Resize {
                target: id!("sidebar"),
                width: 20,
                height: 10,
            }),
        };
        let first = submit(&file, &p).unwrap();
        let second = submit(&file, &p).unwrap();
        assert!(first.ends_with("0001-claude-code.json"), "{first:?}");
        assert!(second.ends_with("0002-claude-code.json"));
        let all = list(&file);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].1, p);
        remove(&first).unwrap();
        assert_eq!(list(&file).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
