//! Terminal capability detection and styling for Cockpit Deck.
//!
//! Respects:
//! - `NO_COLOR` standard (https://no-color.org)
//! - `CLICOLOR_FORCE` / `FORCE_COLOR`
//! - `TERM=dumb`
//! - Terminal width detection (COLUMNS env or query, defaults to 80, min 40)
//! - Unicode locale vs ASCII fallback

use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCaps {
    pub is_tty: bool,
    pub color_enabled: bool,
    pub unicode_enabled: bool,
    pub width: usize,
}

impl RenderCaps {
    /// Detect capabilities from the active environment.
    pub fn detect() -> Self {
        let is_tty = Self::detect_tty();
        let color_enabled = Self::detect_color(is_tty);
        let unicode_enabled = Self::detect_unicode();
        let width = Self::detect_width();

        Self {
            is_tty,
            color_enabled,
            unicode_enabled,
            width,
        }
    }

    /// Explicitly construct an ASCII-only, no-color configuration (e.g. for plain logs or testing).
    pub fn ascii_plain(width: usize) -> Self {
        Self {
            is_tty: false,
            color_enabled: false,
            unicode_enabled: false,
            width: width.max(40),
        }
    }

    /// Construct a rich UTF-8 + ANSI color configuration (e.g. for interactive terminals).
    pub fn rich_terminal(width: usize) -> Self {
        Self {
            is_tty: true,
            color_enabled: true,
            unicode_enabled: true,
            width: width.max(40),
        }
    }

    fn detect_tty() -> bool {
        if env::var("GHL_FORCE_TTY").map(|v| v == "1").unwrap_or(false) {
            return true;
        }
        if env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
            return false;
        }
        std::io::IsTerminal::is_terminal(&std::io::stdout())
    }

    fn detect_color(is_tty: bool) -> bool {
        if env::var("NO_COLOR").is_ok() {
            return false;
        }

        if env::var("FORCE_COLOR").map(|v| v != "0").unwrap_or(false)
            || env::var("CLICOLOR_FORCE")
                .map(|v| v == "1")
                .unwrap_or(false)
        {
            return true;
        }

        if env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
            return false;
        }

        is_tty
    }

    fn detect_unicode() -> bool {
        if env::var("GHL_ASCII_ONLY")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            return false;
        }

        for var in &["LC_ALL", "LC_CTYPE", "LANG"] {
            if let Ok(val) = env::var(var) {
                let lower = val.to_lowercase();
                if lower.contains("utf-8") || lower.contains("utf8") {
                    return true;
                }
            }
        }

        #[cfg(unix)]
        {
            true
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    fn detect_width() -> usize {
        if let Some(w) = env::var("COLUMNS")
            .ok()
            .and_then(|c| c.parse::<usize>().ok())
            .filter(|&w| w >= 40)
        {
            return w;
        }

        #[cfg(windows)]
        {
            #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
            #[repr(C)]
            struct COORD {
                x: i16,
                y: i16,
            }
            #[repr(C)]
            struct SMALL_RECT {
                left: i16,
                top: i16,
                right: i16,
                bottom: i16,
            }
            #[repr(C)]
            struct CONSOLE_SCREEN_BUFFER_INFO {
                size: COORD,
                cursor_pos: COORD,
                attributes: u16,
                window: SMALL_RECT,
                max_window_size: COORD,
            }
            unsafe extern "system" {
                fn GetStdHandle(nStdHandle: i32) -> *mut std::ffi::c_void;
                fn GetConsoleScreenBufferInfo(
                    hConsoleOutput: *mut std::ffi::c_void,
                    lpConsoleScreenBufferInfo: *mut CONSOLE_SCREEN_BUFFER_INFO,
                ) -> i32;
            }

            unsafe {
                let handle = GetStdHandle(-11); // STD_OUTPUT_HANDLE = -11
                if !handle.is_null() && handle != (-1isize as *mut std::ffi::c_void) {
                    let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
                    if GetConsoleScreenBufferInfo(handle, &mut info) != 0 {
                        let w = (info.window.right - info.window.left + 1) as usize;
                        if w >= 40 {
                            return w;
                        }
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            #[repr(C)]
            struct winsize {
                ws_row: u16,
                ws_col: u16,
                ws_xpixel: u16,
                ws_ypixel: u16,
            }
            unsafe extern "C" {
                fn ioctl(fd: i32, request: u64, ...) -> i32;
            }
            unsafe {
                let mut ws: winsize = std::mem::zeroed();
                const TIOCGWINSZ: u64 = 0x5413;
                if ioctl(1, TIOCGWINSZ, &mut ws) == 0 && ws.ws_col >= 40 {
                    return ws.ws_col as usize;
                }
            }
        }

        80
    }

    /// Color helper: apply bold styling if color is enabled.
    pub fn bold(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[1m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply dim styling if color is enabled.
    pub fn dim(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[2m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply cyan styling.
    pub fn cyan(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[36m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply green styling.
    pub fn green(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[32m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply yellow styling.
    pub fn yellow(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[33m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply red styling.
    pub fn red(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[31m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply magenta styling.
    pub fn magenta(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[35m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Color helper: apply blue styling.
    pub fn blue(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[34m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Feline helper: Gojo slate-gray styling for strict/dominant output.
    pub fn gojo(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[38;5;248m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    /// Feline helper: Haru warm orange/amber styling for friendly/success output.
    pub fn haru(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[38;5;214m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_plain_caps() {
        let caps = RenderCaps::ascii_plain(80);
        assert!(!caps.is_tty);
        assert!(!caps.color_enabled);
        assert!(!caps.unicode_enabled);
        assert_eq!(caps.width, 80);

        assert_eq!(caps.bold("Hello"), "Hello");
        assert_eq!(caps.green("Pass"), "Pass");
    }

    #[test]
    fn test_rich_terminal_caps() {
        let caps = RenderCaps::rich_terminal(100);
        assert!(caps.is_tty);
        assert!(caps.color_enabled);
        assert!(caps.unicode_enabled);
        assert_eq!(caps.width, 100);

        assert!(caps.bold("Hello").contains("\x1b[1m"));
        assert!(caps.green("Pass").contains("\x1b[32m"));
    }
}
