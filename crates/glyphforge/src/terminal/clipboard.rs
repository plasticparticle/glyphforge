//! The system clipboard: OSC 52 through the terminal (works over SSH and
//! in every terminal Glyphforge targets), plus `wl-copy` or `xclip` when a
//! local display is available, for terminals that ignore OSC 52.

use std::io::Write;
use std::process::{Command, Stdio};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The OSC 52 sequence that sets the clipboard to `text`.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Copies `text` to the system clipboard. Failures are logged, never
/// fatal: the internal clipboard keeps working either way.
pub fn copy(text: &str) {
    let mut stdout = std::io::stdout();
    if let Err(e) = stdout
        .write_all(osc52(text).as_bytes())
        .and_then(|()| stdout.flush())
    {
        tracing::warn!("OSC 52 clipboard write failed: {e}");
    }
    let tool: Option<(&str, &[&str])> = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Some(("wl-copy", &[]))
    } else if std::env::var_os("DISPLAY").is_some() {
        Some(("xclip", &["-selection", "clipboard"]))
    } else {
        None
    };
    if let Some((program, args)) = tool {
        let spawned = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                // Reap in the background; wl-copy forks and exits quickly.
                std::thread::spawn(move || child.wait());
            }
            Err(e) => tracing::debug!("{program} unavailable: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
