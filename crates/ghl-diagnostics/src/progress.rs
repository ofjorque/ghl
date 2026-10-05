//! First-class interactive progress bars, animated spinners, and visual telemetry for GHL.
//!
//! Provides:
//! - Sub-block high-resolution Unicode progress bars (`█`, `▉`, `▊`, `▋`, `▌`, `▍`, `▎`, `▏` on `░` track)
//! - Animated spinners (`dots`, `circle`, `arc`, `pie`, `haru`, `ascii`)
//! - Visual color themes (`cyan`, `haru`, `emerald`, `magenta`, `gradient`)
//! - In-place single-line terminal rendering (`\r`) with ~30 FPS rate-limiting (throttle)
//! - CI and non-TTY graceful degradation (clean milestone logs instead of log flooding)

use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::caps::RenderCaps;
#[cfg(test)]
use crate::panel::strip_ansi;

/// Curated color themes for progress indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressTheme {
    /// Electric vibrant cyan (#00f5d4 / ANSI 51) - default modern look.
    Cyan,
    /// Warm amber / sunset orange (#ff9f43 / ANSI 214) - GHL signature Haru theme.
    Haru,
    /// Vivid emerald green (#10b981 / ANSI 48) - convergence / bio themes.
    Emerald,
    /// Synthwave neon fuchsia (#f72585 / ANSI 201) - cyberpunk style.
    Magenta,
    /// Dynamic color progression: Blue -> Violet -> Amber -> Emerald at 100%.
    Gradient,
}

impl Default for ProgressTheme {
    fn default() -> Self {
        Self::Cyan
    }
}

impl ProgressTheme {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "haru" | "amber" | "orange" => Self::Haru,
            "emerald" | "green" => Self::Emerald,
            "magenta" | "purple" | "pink" | "synthwave" => Self::Magenta,
            "gradient" | "dynamic" | "rainbow" => Self::Gradient,
            _ => Self::Cyan,
        }
    }

    /// Colorize a text segment according to this theme and the current progress fraction (0.0 to 1.0).
    pub fn colorize(&self, caps: &RenderCaps, fraction: f64, text: &str) -> String {
        if !caps.color_enabled {
            return text.to_string();
        }

        match self {
            Self::Cyan => format!("\x1b[38;5;51m{}\x1b[0m", text),
            Self::Haru => format!("\x1b[38;5;214m{}\x1b[0m", text),
            Self::Emerald => format!("\x1b[38;5;48m{}\x1b[0m", text),
            Self::Magenta => format!("\x1b[38;5;201m{}\x1b[0m", text),
            Self::Gradient => {
                let code = if fraction < 0.30 {
                    "38;5;51" // Cyan / Blue
                } else if fraction < 0.65 {
                    "38;5;171" // Purple / Violet
                } else if fraction < 0.95 {
                    "38;5;214" // Warm Amber
                } else {
                    "38;5;48" // Emerald Green
                };
                format!("\x1b[{}m{}\x1b[0m", code, text)
            }
        }
    }

    pub fn accent_color(&self, caps: &RenderCaps) -> &'static str {
        if !caps.color_enabled {
            return "";
        }
        match self {
            Self::Cyan => "\x1b[38;5;51m",
            Self::Haru => "\x1b[38;5;214m",
            Self::Emerald => "\x1b[38;5;48m",
            Self::Magenta => "\x1b[38;5;201m",
            Self::Gradient => "\x1b[38;5;214m",
        }
    }
}

/// Visual spinner animation styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinnerStyle {
    /// Braille dots: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
    Dots,
    /// Circle quadrants: ◐ ◓ ◑ ◒
    Circle,
    /// Arc sectors: ◜ ◠ ◝ ◞ ◡ ◟
    Arc,
    /// Pie slices: ○ ◔ ◑ ◕ ●
    Pie,
    /// Haru feline kaomoji animation
    Haru,
    /// ASCII fallback: | / - \
    Ascii,
}

impl Default for SpinnerStyle {
    fn default() -> Self {
        Self::Dots
    }
}

impl SpinnerStyle {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "circle" | "quadrant" => Self::Circle,
            "arc" => Self::Arc,
            "pie" => Self::Pie,
            "haru" | "cat" => Self::Haru,
            "ascii" => Self::Ascii,
            _ => Self::Dots,
        }
    }

    pub fn glyph(&self, frame: usize, unicode: bool) -> &'static str {
        if !unicode {
            let frames = ["|", "/", "-", "\\"];
            return frames[frame % frames.len()];
        }

        match self {
            Self::Dots => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                frames[frame % frames.len()]
            }
            Self::Circle => {
                let frames = ["◐", "◓", "◑", "◒"];
                frames[frame % frames.len()]
            }
            Self::Arc => {
                let frames = ["◜", "◠", "◝", "◞", "◡", "◟"];
                frames[frame % frames.len()]
            }
            Self::Pie => {
                let frames = ["○", "◔", "◑", "◕", "●"];
                frames[frame % frames.len()]
            }
            Self::Haru => {
                let frames = ["ฅ^•ﻌ•^ฅ", "ฅ(•⩊ •マ", "ฅ(>⩊ <マ", "ฅ(•⩊ •マ"];
                frames[frame % frames.len()]
            }
            Self::Ascii => {
                let frames = ["|", "/", "-", "\\"];
                frames[frame % frames.len()]
            }
        }
    }
}

/// Circular progress disc for determinate progress in tight spaces.
pub fn circle_disc_glyph(fraction: f64, unicode: bool) -> &'static str {
    if !unicode {
        if fraction >= 1.0 {
            "[#]"
        } else if fraction >= 0.5 {
            "[+]"
        } else {
            "[ ]"
        }
    } else if fraction >= 0.95 {
        "●"
    } else if fraction >= 0.70 {
        "◕"
    } else if fraction >= 0.40 {
        "◑"
    } else if fraction >= 0.15 {
        "◔"
    } else {
        "○"
    }
}

/// Global shared state for single-line interactive telemetries.
struct GlobalTelemetryState {
    start_time: Option<Instant>,
    last_render: Option<Instant>,
    last_reported_pct: i64,
    frame: usize,
    active: bool,
}

impl GlobalTelemetryState {
    const fn new() -> Self {
        Self {
            start_time: None,
            last_render: None,
            last_reported_pct: -1,
            frame: 0,
            active: false,
        }
    }

    fn reset(&mut self) {
        let now = Instant::now();
        self.start_time = Some(now);
        self.last_render = Some(now);
        self.last_reported_pct = -1;
        self.frame = 0;
        self.active = true;
    }
}

static TELEMETRY: Mutex<GlobalTelemetryState> = Mutex::new(GlobalTelemetryState::new());
static PROGRESS_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Configure artificial progress delay in milliseconds per update step (0 disables delay).
pub fn set_progress_delay_ms(delay_ms: u64) {
    PROGRESS_DELAY_MS.store(delay_ms, std::sync::atomic::Ordering::Relaxed);
}

/// Retrieve currently configured progress delay in milliseconds.
pub fn get_progress_delay_ms() -> u64 {
    PROGRESS_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Format duration into human-readable MM:SS or HH:MM:SS.
pub fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours:02}:{mins:02}:{secs:02}")
    } else {
        format!("{mins:02}:{secs:02}")
    }
}

/// Renders a single-line progress bar string with full telemetry and theme styling.
pub fn render_progress_line(
    current: i64,
    total: i64,
    label: &str,
    details: &str,
    theme: ProgressTheme,
    frame: usize,
    elapsed: Duration,
    caps: &RenderCaps,
) -> String {
    let total = total.max(1);
    let clamped_curr = current.clamp(0, total);
    let fraction = (clamped_curr as f64 / total as f64).clamp(0.0, 1.0);
    let pct = fraction * 100.0;

    // Spinner glyph prefix
    let spinner = SpinnerStyle::Dots.glyph(frame, caps.unicode_enabled);
    let styled_spinner = theme.colorize(caps, fraction, spinner);

    // Compute speeds and ETA
    let elapsed_secs = elapsed.as_secs_f64();
    let it_per_sec = if elapsed_secs > 0.001 {
        clamped_curr as f64 / elapsed_secs
    } else {
        0.0
    };

    let time_str = if clamped_curr >= total {
        format!("[{}, {:.1} it/s]", format_duration(elapsed), it_per_sec)
    } else if it_per_sec > 0.01 {
        let remaining_secs = ((total - clamped_curr) as f64 / it_per_sec).max(0.0);
        let eta = Duration::from_secs_f64(remaining_secs);
        format!(
            "[{}<{}, {:.1} it/s]",
            format_duration(elapsed),
            format_duration(eta),
            it_per_sec
        )
    } else {
        format!("[{}<??, 0.0 it/s]", format_duration(elapsed))
    };

    // Width management for the bar itself
    let term_width = caps.width.max(50);
    // Overhead: spinner (2) + label + percentage (7) + counter (clamped/total) + time_str + details
    let counter_str = format!("({clamped_curr}/{total})");
    let pct_str = format!("{pct:>5.1}%");

    // Static text length
    let fixed_len = 2
        + if label.is_empty() { 0 } else { label.len() + 1 }
        + 2 // brackets "[ ]"
        + 1 + pct_str.len()
        + 1 + counter_str.len()
        + 1 + time_str.len()
        + if details.is_empty() { 0 } else { 3 + details.len() };

    let bar_width = if term_width > fixed_len + 10 {
        (term_width - fixed_len).min(30)
    } else {
        10
    };

    let bar_graphic = build_bar_graphic(fraction, bar_width, theme, caps);

    let mut out = String::new();
    out.push_str(&styled_spinner);
    out.push(' ');
    if !label.is_empty() {
        out.push_str(&caps.bold(label));
        out.push(' ');
    }
    out.push('[');
    out.push_str(&bar_graphic);
    out.push(']');
    out.push(' ');

    // Styled percentage
    if caps.color_enabled {
        let pct_color = if fraction >= 1.0 {
            "\x1b[1;32m"
        } else {
            "\x1b[1;33m"
        };
        let _ = write!(out, "{pct_color}{pct_str}\x1b[0m");
    } else {
        out.push_str(&pct_str);
    }

    out.push(' ');
    out.push_str(&caps.dim(&counter_str));
    out.push(' ');

    // Styled telemetry (muted cyan)
    if caps.color_enabled {
        let _ = write!(out, "\x1b[36m{time_str}\x1b[0m");
    } else {
        out.push_str(&time_str);
    }

    if !details.is_empty() {
        out.push_str(" | ");
        if caps.color_enabled {
            let _ = write!(out, "\x1b[38;5;120m{details}\x1b[0m");
        } else {
            out.push_str(details);
        }
    }

    out
}

/// Helper to render the filled and empty portions of a progress bar.
pub fn build_bar_graphic(
    fraction: f64,
    width: usize,
    theme: ProgressTheme,
    caps: &RenderCaps,
) -> String {
    let width = width.max(4);
    if caps.unicode_enabled {
        let total_subblocks = (fraction * (width as f64 * 8.0)).round() as usize;
        let full_blocks = total_subblocks / 8;
        let remainder = total_subblocks % 8;

        let full_part = "█".repeat(full_blocks.min(width));
        let partial_char = match remainder {
            1 => "▏",
            2 => "▎",
            3 => "▍",
            4 => "▌",
            5 => "▋",
            6 => "▊",
            7 => "▉",
            _ => "",
        };

        let filled_count = full_blocks + if remainder > 0 { 1 } else { 0 };
        let empty_count = width.saturating_sub(filled_count);
        let empty_part = "░".repeat(empty_count);

        let colored_filled = theme.colorize(caps, fraction, &format!("{full_part}{partial_char}"));
        let dimmed_empty = if caps.color_enabled {
            format!("\x1b[38;5;238m{empty_part}\x1b[0m")
        } else {
            caps.dim(&empty_part)
        };

        format!("{colored_filled}{dimmed_empty}")
    } else {
        // Plain ASCII fallback: [=====>-----]
        let filled_len = ((fraction * width as f64).round() as usize).min(width);
        let arrow = if filled_len > 0 && filled_len < width {
            ">"
        } else if filled_len == width {
            "="
        } else {
            ""
        };
        let equals_len = filled_len.saturating_sub(arrow.len());
        let equals_part = "=".repeat(equals_len);
        let empty_len = width.saturating_sub(filled_len);
        let empty_part = "-".repeat(empty_len);

        let colored_filled = theme.colorize(caps, fraction, &format!("{equals_part}{arrow}"));
        format!("{colored_filled}{empty_part}")
    }
}

/// Renders a single-line indeterminate spinner with elapsed time and speed.
pub fn render_spinner_line(
    label: &str,
    details: &str,
    style: SpinnerStyle,
    theme: ProgressTheme,
    frame: usize,
    elapsed: Duration,
    caps: &RenderCaps,
) -> String {
    let glyph = style.glyph(frame, caps.unicode_enabled);
    let styled_glyph = theme.colorize(caps, 0.5, glyph);
    let time_str = format!("[{}]", format_duration(elapsed));

    let mut out = String::new();
    out.push_str(&styled_glyph);
    out.push(' ');
    if !label.is_empty() {
        out.push_str(&caps.bold(label));
        out.push(' ');
    }

    if caps.color_enabled {
        let _ = write!(out, "\x1b[36m{time_str}\x1b[0m");
    } else {
        out.push_str(&time_str);
    }

    if !details.is_empty() {
        out.push_str(" | ");
        if caps.color_enabled {
            let _ = write!(out, "\x1b[38;5;120m{details}\x1b[0m");
        } else {
            out.push_str(details);
        }
    }

    out
}

/// Primary public function to update an in-place progress bar from script/runtime.
///
/// Throttled to ~30 FPS (every 33ms) so tight loops run at native speed.
pub fn update_progress_bar(
    current: i64,
    total: i64,
    label: &str,
    details: &str,
    theme: ProgressTheme,
    caps: &RenderCaps,
) {
    let mut st = match TELEMETRY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let now = Instant::now();
    if !st.active || st.start_time.is_none() {
        st.reset();
    }
    let start_time = st.start_time.unwrap_or(now);
    let last_render = st.last_render.unwrap_or(now);

    let delay = get_progress_delay_ms();
    let is_done = current >= total;
    let throttle_dur = Duration::from_millis(33); // ~30 FPS throttle

    // Throttle checks (bypass when explicit progress delay is active)
    if delay == 0 && !is_done && now.duration_since(last_render) < throttle_dur {
        return;
    }

    st.last_render = Some(now);
    st.frame = st.frame.wrapping_add(1);
    let frame = st.frame;
    let elapsed = now.duration_since(start_time);

    let line = render_progress_line(
        current,
        total,
        label,
        details,
        theme,
        frame,
        elapsed,
        caps,
    );

    if caps.is_tty {
        // In-place carriage return with line erase
        print!("\r\x1b[2K{line}");
        let _ = std::io::stdout().flush();
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
    } else {
        // Non-TTY / CI: report only on 10% milestone transitions or completion
        let pct = if total > 0 {
            (current * 100) / total
        } else {
            100
        };
        let milestone = pct / 10;
        if milestone != st.last_reported_pct || is_done {
            st.last_reported_pct = milestone;
            println!("{line}");
        }
    }
}

/// Primary public function to update an in-place indeterminate spinner.
pub fn update_progress_spinner(
    label: &str,
    details: &str,
    style: SpinnerStyle,
    theme: ProgressTheme,
    caps: &RenderCaps,
) {
    let mut st = match TELEMETRY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let now = Instant::now();
    if !st.active || st.start_time.is_none() {
        st.reset();
    }
    let start_time = st.start_time.unwrap_or(now);
    let last_render = st.last_render.unwrap_or(now);

    let delay = get_progress_delay_ms();
    let throttle_dur = Duration::from_millis(60); // 16 FPS for pleasant spinner rotation
    if delay == 0 && now.duration_since(last_render) < throttle_dur {
        return;
    }

    st.last_render = Some(now);
    st.frame = st.frame.wrapping_add(1);
    let frame = st.frame;
    let elapsed = now.duration_since(start_time);

    let line = render_spinner_line(label, details, style, theme, frame, elapsed, caps);

    if caps.is_tty {
        print!("\r\x1b[2K{line}");
        let _ = std::io::stdout().flush();
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
    }
}

/// Finalizes the active progress bar or spinner with an optional completion message.
pub fn finish_progress(message: &str, caps: &RenderCaps) {
    let mut st = match TELEMETRY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if !st.active {
        if !message.is_empty() {
            println!("{message}");
        }
        return;
    }

    let start_time = st.start_time.unwrap_or_else(Instant::now);
    st.active = false;
    st.start_time = None;
    st.last_render = None;
    let elapsed = Instant::now().duration_since(start_time);
    let elapsed_str = format_duration(elapsed);

    let badge = caps.haru("ฅ(•⩊ •マ COMPLETED");
    let check = if caps.unicode_enabled { "✔" } else { "[OK]" };

    if caps.is_tty {
        print!("\r\x1b[2K");
    }

    if message.is_empty() {
        println!("{check} {badge} [{elapsed_str}]");
    } else {
        println!("{check} {badge} [{elapsed_str}] {message}");
    }
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_render_rich() {
        let caps = RenderCaps::rich_terminal(80);
        let line = render_progress_line(
            50,
            100,
            "Optimizing",
            "loss: 0.012",
            ProgressTheme::Cyan,
            0,
            Duration::from_secs(2),
            &caps,
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("Optimizing"));
        assert!(clean.contains("50.0%"));
        assert!(clean.contains("(50/100)"));
        assert!(clean.contains("loss: 0.012"));
    }

    #[test]
    fn test_progress_bar_render_ascii() {
        let caps = RenderCaps::ascii_plain(80);
        let line = render_progress_line(
            75,
            100,
            "BFGS",
            "",
            ProgressTheme::Haru,
            1,
            Duration::from_secs(1),
            &caps,
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("BFGS"));
        assert!(clean.contains("75.0%"));
        assert!(clean.contains("===="));
    }

    #[test]
    fn test_spinner_styles() {
        let caps = RenderCaps::rich_terminal(80);
        let line = render_spinner_line(
            "Waiting for SQL",
            "streaming",
            SpinnerStyle::Dots,
            ProgressTheme::Emerald,
            3,
            Duration::from_secs(4),
            &caps,
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("Waiting for SQL"));
        assert!(clean.contains("streaming"));
    }

    #[test]
    fn test_circle_disc_glyphs() {
        assert_eq!(circle_disc_glyph(0.05, true), "○");
        assert_eq!(circle_disc_glyph(0.25, true), "◔");
        assert_eq!(circle_disc_glyph(0.50, true), "◑");
        assert_eq!(circle_disc_glyph(0.80, true), "◕");
        assert_eq!(circle_disc_glyph(1.00, true), "●");

        assert_eq!(circle_disc_glyph(1.00, false), "[#]");
    }
}
