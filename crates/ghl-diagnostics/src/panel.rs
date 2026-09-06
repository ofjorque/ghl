//! Terminal Panel Component for GHL Cockpit Deck.
//!
//! Renders structured cards with rounded borders (`╭─╮`, `│ │`, `╰─╯`) in Unicode
//! or plain ASCII boxes (`+-+`, `| |`, `+-+`), title headers, badges, and strict width clipping.

use crate::caps::RenderCaps;

#[derive(Debug, Clone)]
pub enum PanelItem {
    Line(String),
    KeyValue { key: String, val: String },
    Divider,
}

#[derive(Debug, Clone)]
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

        let header_text_len = title_clean.chars().count() + if !badge_clean.is_empty() { badge_clean.chars().count() + 1 } else { 0 };
        let remaining_dashes = inner_width.saturating_sub(header_text_len + 2);

        let top_border = if let Some(badge) = &self.badge {
            format!(
                "{tl}{hz} {} {}{hz} {} {hz}{tr}",
                self.title,
                hz.to_string().repeat(remaining_dashes),
                badge,
            )
        } else {
            format!(
                "{tl}{hz} {} {}{hz}{tr}",
                self.title,
                hz.to_string().repeat(inner_width.saturating_sub(title_clean.chars().count() + 1))
            )
        };
        out.push_str(&caps.dim(&top_border));
        out.push('\n');

        // 2. Body lines
        for item in &self.items {
            match item {
                PanelItem::Divider => {
                    let div_line = format!("{sep_l}{}{sep_r}", hz.to_string().repeat(width.saturating_sub(2)));
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
                    out.push_str(&format!("{} {} {}\n", caps.dim(&vt.to_string()), padded_line, caps.dim(&vt.to_string())));
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
                    out.push_str(&format!("{} {} {}\n", caps.dim(&vt.to_string()), content, caps.dim(&vt.to_string())));
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
    fn test_strip_ansi() {
        let text = "\x1b[1m\x1b[32mSuccess\x1b[0m";
        assert_eq!(strip_ansi(text), "Success");
        assert_eq!(visual_width(text), 7);
    }
}
