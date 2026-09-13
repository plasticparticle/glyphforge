//! Terminal lifecycle: enter/leave the full-screen mode safely.

pub mod caps;

use std::io::{self, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub use caps::{ColorDepth, TerminalCapabilities};

/// Errors while setting up the terminal.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("cannot enable raw mode: {0}")]
    RawMode(#[source] io::Error),
    #[error("cannot switch terminal modes: {0}")]
    Mode(#[source] io::Error),
    #[error("cannot create terminal backend: {0}")]
    Backend(#[source] io::Error),
}

/// Global record of what we changed, so a panic hook can undo it without
/// access to the guard.
static RAW_MODE: AtomicBool = AtomicBool::new(false);
static ALT_SCREEN: AtomicBool = AtomicBool::new(false);
static MOUSE: AtomicBool = AtomicBool::new(false);
static KEYBOARD_FLAGS: AtomicBool = AtomicBool::new(false);

/// Best-effort restoration of the terminal. Safe to call more than once and
/// from a panic hook.
pub fn restore_terminal() {
    let mut out = io::stdout();
    if KEYBOARD_FLAGS.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    if MOUSE.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, DisableMouseCapture);
    }
    if ALT_SCREEN.swap(false, Ordering::SeqCst) {
        let _ = execute!(
            out,
            DisableBracketedPaste,
            DisableFocusChange,
            LeaveAlternateScreen,
            cursor::Show
        );
    }
    if RAW_MODE.swap(false, Ordering::SeqCst) {
        let _ = disable_raw_mode();
    }
    let _ = out.flush();
}

/// Installs a panic hook that restores the terminal before the default hook
/// prints the panic message, so a bug never leaves the shell in raw mode.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}

/// RAII guard around the full-screen terminal session.
#[derive(Debug)]
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    keyboard_enhancement: bool,
}

impl TerminalGuard {
    /// Enters raw mode, the alternate screen, and enables mouse, paste and
    /// focus reporting. Keyboard enhancement is probed and enabled when
    /// supported.
    pub fn enter(mouse: bool) -> Result<Self, TerminalError> {
        enable_raw_mode().map_err(TerminalError::RawMode)?;
        RAW_MODE.store(true, Ordering::SeqCst);

        let mut out = io::stdout();
        if let Err(e) = execute!(
            out,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableFocusChange
        ) {
            restore_terminal();
            return Err(TerminalError::Mode(e));
        }
        ALT_SCREEN.store(true, Ordering::SeqCst);

        if mouse {
            if let Err(e) = execute!(out, EnableMouseCapture) {
                restore_terminal();
                return Err(TerminalError::Mode(e));
            }
            MOUSE.store(true, Ordering::SeqCst);
        }

        // The probe writes a query and reads the reply; it needs raw mode.
        let keyboard_enhancement = matches!(supports_keyboard_enhancement(), Ok(true));
        if keyboard_enhancement {
            let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
            if execute!(out, PushKeyboardEnhancementFlags(flags)).is_ok() {
                KEYBOARD_FLAGS.store(true, Ordering::SeqCst);
            }
        }

        let backend = CrosstermBackend::new(out);
        let terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                restore_terminal();
                return Err(TerminalError::Backend(e));
            }
        };
        Ok(Self {
            terminal,
            keyboard_enhancement,
        })
    }

    pub const fn keyboard_enhancement(&self) -> bool {
        self.keyboard_enhancement
    }

    pub const fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}
