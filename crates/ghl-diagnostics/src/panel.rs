//! Terminal Panel Component for GHL Cockpit Deck.
//!
//! Renders structured cards with rounded borders (`╭─╮`, `│ │`, `╰─╯`) in Unicode
//! or plain ASCII boxes (`+-+`, `| |`, `+-+`), title headers, badges, and strict width clipping.

use crate::caps::RenderCaps;
use crate::progress::{ProgressTheme, build_bar_graphic, circle_disc_glyph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelItem {
    Line(String),
    KeyValue {
        key: String,
        val: String,
    },
    Divider,
    ProgressBar {
        label: String,
        current: i64,
        total: i64,
        theme: String,
    },
    CircleProgress {
        label: String,
        current: i64,
        total: i64,
        theme: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CockpitPanel {
    pub title: String,
    pub badge: Option<String>,
    pub items: Vec<PanelItem>,
}

impl CockpitPanel {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            badge: None,
            items: Vec::new(),
        }
    }

    pub fn with_badge(&mut self, badge: impl Into<String>) -> &mut Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn add_line(&mut self, line: impl Into<String>) -> &mut Self {
        self.items.push(PanelItem::Line(line.into()));
        self
    }

    pub fn add_kv(&mut self, key: impl Into<String>, val: impl Into<String>) -> &mut Self {
        self.items.push(PanelItem::KeyValue {
            key: key.into(),
            val: val.into(),
        });
        self
    }

    pub fn add_divider(&mut self) -> &mut Self {
        self.items.push(PanelItem::Divider);
        self
    }

    pub fn add_progress(
        &mut self,
        label: impl Into<String>,
        current: i64,
        total: i64,
        theme: impl Into<String>,
    ) -> &mut Self {
        self.items.push(PanelItem::ProgressBar {
            label: label.into(),
            current,
            total,
            theme: theme.into(),
        });
        self
    }

    pub fn add_circle(
        &mut self,
        label: impl Into<String>,
        current: i64,
        total: i64,
        theme: impl Into<String>,
    ) -> &mut Self {
        self.items.push(PanelItem::CircleProgress {
            label: label.into(),
            current,
            total,
            theme: theme.into(),
        });
        self
    }

    /// Construct a structured operation telemetry card for high-volume tabular verbs.
    pub fn operation_telemetry(
        op_name: &str,
        row_count: usize,
        elapsed_secs: f64,
        details: Option<&str>,
    ) -> Self {
        let mut panel = Self::new(format!("Cockpit Telemetry: {op_name}"));
        panel.with_badge("ฅ(•⩊ •マ PROCESSED");
        let throughput = if elapsed_secs > 0.0 {
            (row_count as f64 / elapsed_secs) as u64
        } else {
            0
        };
        panel.add_kv("Processed Rows", format!("{row_count}"));
        panel.add_kv("Elapsed Time", format!("{:.3} ms", elapsed_secs * 1000.0));
        panel.add_kv("Throughput", format!("{throughput} rows/sec"));
        if let Some(extra) = details {
            panel.add_kv("Details", extra.to_string());
        }
        panel
    }

    /// Render the panel respecting `caps.width`, unicode setting, and colors.
    pub fn render(&self, caps: &RenderCaps) -> String {
        let width = caps.width.max(40);
        let inner_width = width.saturating_sub(4); // 2 borders + 2 margins

        let (tl, tr, bl, br, hz, vt, sep_l, sep_r) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│', '├', '┤')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+')
        };

        let mut out = String::new();

        // 1. Top border with title and badge
        let title_clean = strip_ansi(&self.title);
        let badge_clean = self.badge.as_deref().map(strip_ansi).unwrap_or_default();

        let top_border = if let Some(badge) = &self.badge {
            let fixed_len = 8 + title_clean.chars().count() + badge_clean.chars().count();
            let dashes = width.saturating_sub(fixed_len);
            format!(
                "{tl}{hz} {} {} {} {hz}{tr}",
                self.title,
                hz.to_string().repeat(dashes),
                badge,
            )
        } else {
            let fixed_len = 5 + title_clean.chars().count();
            let dashes = width.saturating_sub(fixed_len);
            format!(
                "{tl}{hz} {} {}{tr}",
                self.title,
                hz.to_string().repeat(dashes),
            )
        };
        out.push_str(&caps.dim(&top_border));
        out.push('\n');

        // 2. Body lines
        for item in &self.items {
            match item {
                PanelItem::Divider => {
                    let div_line = format!(
                        "{sep_l}{}{sep_r}",
                        hz.to_string().repeat(width.saturating_sub(2))
                    );
                    out.push_str(&caps.dim(&div_line));
                    out.push('\n');
                }
                PanelItem::Line(line) => {
                    let vlen = visual_width(line);
                    let padded_line = if vlen > inner_width {
                        truncate_with_ellipsis(line, inner_width, caps.unicode_enabled)
                    } else {
                        let pad = " ".repeat(inner_width - vlen);
                        format!("{line}{pad}")
                    };
                    out.push_str(&format!(
                        "{} {} {}\n",
                        caps.dim(&vt.to_string()),
                        padded_line,
                        caps.dim(&vt.to_string())
                    ));
                }
                PanelItem::KeyValue { key, val } => {
                    let k_len = visual_width(key);
                    let v_len = visual_width(val);
                    let total = k_len + v_len + 2; // key + ": " + val

                    let content = if total > inner_width {
                        let allowed_v = inner_width.saturating_sub(k_len + 3);
                        let trunc_v = truncate_with_ellipsis(val, allowed_v, caps.unicode_enabled);
                        format!("{}: {}", caps.bold(key), trunc_v)
                    } else {
                        let pad = " ".repeat(inner_width - total);
                        format!("{}: {}{pad}", caps.bold(key), val)
                    };
                    out.push_str(&format!(
                        "{} {} {}\n",
                        caps.dim(&vt.to_string()),
                        content,
                        caps.dim(&vt.to_string())
                    ));
                }
                PanelItem::ProgressBar {
                    label,
                    current,
                    total,
                    theme,
                } => {
                    let tot = (*total).max(1);
                    let cur = (*current).clamp(0, tot);
                    let fraction = (cur as f64 / tot as f64).clamp(0.0, 1.0);
                    let pct_str = format!("{:>5.1}%", fraction * 100.0);
                    let ratio_str = format!("({cur}/{tot})");
                    let theme_parsed = ProgressTheme::parse(theme);

                    let l_len = if label.is_empty() {
                        0
                    } else {
                        visual_width(label) + 1
                    };
                    let meta_len = pct_str.len() + 1 + ratio_str.len() + 2;
                    let available = inner_width.saturating_sub(l_len + meta_len + 2);
                    let bar_w = available.clamp(6, 24);

                    let graphic = build_bar_graphic(fraction, bar_w, theme_parsed, caps);

                    let mut content = String::new();
                    if !label.is_empty() {
                        content.push_str(&caps.bold(label));
                        content.push(' ');
                    }
                    content.push('[');
                    content.push_str(&graphic);
                    content.push(']');
                    content.push(' ');
                    if caps.color_enabled {
                        let pct_color = if fraction >= 1.0 {
                            "\x1b[1;32m"
                        } else {
                            "\x1b[1;33m"
                        };
                        content.push_str(&format!("{pct_color}{pct_str}\x1b[0m"));
                    } else {
                        content.push_str(&pct_str);
                    }
                    content.push(' ');
                    content.push_str(&caps.dim(&ratio_str));

                    let vlen = visual_width(&content);
                    let padded_line = if vlen > inner_width {
                        truncate_with_ellipsis(&content, inner_width, caps.unicode_enabled)
                    } else {
                        let pad = " ".repeat(inner_width - vlen);
                        format!("{content}{pad}")
                    };
                    out.push_str(&format!(
                        "{} {} {}\n",
                        caps.dim(&vt.to_string()),
                        padded_line,
                        caps.dim(&vt.to_string())
                    ));
                }
                PanelItem::CircleProgress {
                    label,
                    current,
                    total,
                    theme,
                } => {
                    let tot = (*total).max(1);
                    let cur = (*current).clamp(0, tot);
                    let fraction = (cur as f64 / tot as f64).clamp(0.0, 1.0);
                    let theme_parsed = ProgressTheme::parse(theme);
                    let glyph = circle_disc_glyph(fraction, caps.unicode_enabled);
                    let colored_glyph = theme_parsed.colorize(caps, fraction, glyph);

                    let pct_str = format!("{:>5.1}%", fraction * 100.0);
                    let ratio_str = format!("({cur}/{tot})");

                    let mut content = String::new();
                    if !label.is_empty() {
                        content.push_str(&caps.bold(label));
                        content.push(' ');
                    }
                    content.push_str(&colored_glyph);
                    content.push(' ');
                    if caps.color_enabled {
                        let pct_color = if fraction >= 1.0 {
                            "\x1b[1;32m"
                        } else {
                            "\x1b[1;33m"
                        };
                        content.push_str(&format!("{pct_color}{pct_str}\x1b[0m"));
                    } else {
                        content.push_str(&pct_str);
                    }
                    content.push(' ');
                    content.push_str(&caps.dim(&ratio_str));

                    let vlen = visual_width(&content);
                    let padded_line = if vlen > inner_width {
                        truncate_with_ellipsis(&content, inner_width, caps.unicode_enabled)
                    } else {
                        let pad = " ".repeat(inner_width - vlen);
                        format!("{content}{pad}")
                    };
                    out.push_str(&format!(
                        "{} {} {}\n",
                        caps.dim(&vt.to_string()),
                        padded_line,
                        caps.dim(&vt.to_string())
                    ));
                }
            }
        }

        // 3. Bottom border
        let bot_border = format!("{bl}{}{br}", hz.to_string().repeat(width.saturating_sub(2)));
        out.push_str(&caps.dim(&bot_border));
        out.push('\n');

        out
    }
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;

    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// Calculate visual character count ignoring ANSI escape sequences.
pub fn visual_width(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

fn truncate_with_ellipsis(s: &str, max_width: usize, unicode: bool) -> String {
    let clean = strip_ansi(s);
    let count = clean.chars().count();
    if count <= max_width {
        return s.to_string();
    }

    let ell = if unicode { "…" } else { "..." };
    let ell_len = ell.chars().count();
    let target = max_width.saturating_sub(ell_len);

    let prefix: String = clean.chars().take(target).collect();
    format!("{prefix}{ell}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_unicode_render() {
        let caps = RenderCaps::rich_terminal(60);
        let mut panel = CockpitPanel::new("NEKO Model Fit");
        panel.with_badge("[OK]");
        panel.add_kv("Formula", "y ~ x1 + x2");
        panel.add_divider();
        panel.add_line("R-squared: 0.998");

        let rendered = panel.render(&caps);
        assert!(rendered.contains("╭"));
        assert!(rendered.contains("╯"));
        assert!(rendered.contains("NEKO Model Fit"));
        assert!(rendered.contains("R-squared: 0.998"));
    }

    #[test]
    fn test_panel_ascii_render() {
        let caps = RenderCaps::ascii_plain(60);
        let mut panel = CockpitPanel::new("NEKO Model Fit");
        panel.add_line("Status: Clean");

        let rendered = panel.render(&caps);
        assert!(rendered.contains("+- NEKO Model Fit"));
        assert!(rendered.contains("| Status: Clean"));
        assert!(!rendered.contains("╭"));
    }

    #[test]
    fn test_panel_lines_have_consistent_width() {
        let width = 70;
        let caps = RenderCaps::rich_terminal(width);
        let mut panel = CockpitPanel::new("GHL Interactive Shell (REPL)");
        panel.with_badge("READY");
        panel.add_line("Gojo & Haru High-Performance Statistical System");
        panel.add_line("Type :help for session commands or :quit to exit.");
        panel.add_divider();
        panel.add_kv("Status", "Operational");

        let rendered = panel.render(&caps);
        for (i, line) in rendered.lines().enumerate() {
            let line_len = visual_width(line);
            assert_eq!(
                line_len, width as usize,
                "Line {i} visual width {line_len} does not match target width {width}: {line:?}"
            );
        }
    }

    #[test]
    fn test_strip_ansi() {
        let text = "\x1b[1m\x1b[32mSuccess\x1b[0m";
        assert_eq!(strip_ansi(text), "Success");
        assert_eq!(visual_width(text), 7);
    }

    #[test]
    fn test_panel_with_progress_and_circle() {
        let width = 60;
        let caps = RenderCaps::rich_terminal(width);
        let mut panel = CockpitPanel::new("MCMC Status");
        panel.add_progress("Convergence", 80, 100, "emerald");
        panel.add_circle("Warmup", 100, 100, "haru");
        panel.add_circle("Sampling", 45, 100, "cyan");

        let rendered = panel.render(&caps);
        for (i, line) in rendered.lines().enumerate() {
            let line_len = visual_width(line);
            assert_eq!(
                line_len, width as usize,
                "Line {i} visual width {line_len} does not match target width {width}: {line:?}"
            );
        }
        assert!(rendered.contains("80.0%"));
        assert!(rendered.contains("●"));
        assert!(rendered.contains("◑"));
    }
}
