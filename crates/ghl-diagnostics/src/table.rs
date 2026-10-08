//! Cockpit Deck Table Component.
//!
//! Provides modern box-drawing tables with column type tags, row numbers,
//! proper alignment, smart truncation, and ASCII/Unicode fallback.

use crate::caps::RenderCaps;
use crate::panel::visual_width;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone)]
pub struct TableColumn {
    pub name: String,
    pub type_hint: Option<String>,
    pub alignment: TableAlignment,
}

impl TableColumn {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_hint: None,
            alignment: TableAlignment::Left,
        }
    }

    pub fn with_type(mut self, ty: impl Into<String>) -> Self {
        self.type_hint = Some(ty.into());
        self
    }

    pub fn with_alignment(mut self, align: TableAlignment) -> Self {
        self.alignment = align;
        self
    }
}

#[derive(Debug, Clone)]
pub struct CockpitTable {
    pub title: Option<String>,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<String>>,
    pub show_row_numbers: bool,
    pub max_display_rows: usize,
}

impl Default for CockpitTable {
    fn default() -> Self {
        Self::new()
    }
}

impl CockpitTable {
    pub fn new() -> Self {
        Self {
            title: None,
            columns: Vec::new(),
            rows: Vec::new(),
            show_row_numbers: true,
            max_display_rows: 10,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn add_column(&mut self, col: TableColumn) -> &mut Self {
        self.columns.push(col);
        self
    }

    pub fn add_row(&mut self, row: Vec<String>) -> &mut Self {
        self.rows.push(row);
        self
    }

    pub fn set_show_row_numbers(&mut self, show: bool) -> &mut Self {
        self.show_row_numbers = show;
        self
    }

    pub fn set_max_display_rows(&mut self, max: usize) -> &mut Self {
        self.max_display_rows = max;
        self
    }

    /// Render the table respecting terminal capabilities.
    pub fn render(&self, caps: &RenderCaps) -> String {
        if self.columns.is_empty() {
            return String::new();
        }

        let total_rows = self.rows.len();
        let num_cols = self.columns.len();

        // Check if truncation is needed
        let (display_rows, _is_truncated) =
            if total_rows > self.max_display_rows && self.max_display_rows >= 6 {
                let head_count = self.max_display_rows.saturating_sub(3);
                let tail_count = 3;
                let mut subset: Vec<(usize, &Vec<String>)> = Vec::new();
                for i in 0..head_count {
                    if let Some(r) = self.rows.get(i) {
                        subset.push((i + 1, r));
                    }
                }
                // Ellipsis placeholder row represented by index 0
                subset.push((0, &self.rows[0]));
                for i in (total_rows - tail_count)..total_rows {
                    if let Some(r) = self.rows.get(i) {
                        subset.push((i + 1, r));
                    }
                }
                (subset, true)
            } else {
                let subset: Vec<(usize, &Vec<String>)> = self
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (i + 1, r))
                    .collect();
                (subset, false)
            };

        // Determine widths
        let row_num_width = if self.show_row_numbers {
            total_rows.to_string().len().max(1)
        } else {
            0
        };

        let mut col_widths: Vec<usize> = Vec::with_capacity(num_cols);
        for (col_idx, col) in self.columns.iter().enumerate() {
            let header_text = if let Some(ref ty) = col.type_hint {
                format!("{} <{}>", col.name, ty)
            } else {
                col.name.clone()
            };
            let mut max_w = visual_width(&header_text);

            for (_, row) in &display_rows {
                if let Some(cell) = row.get(col_idx) {
                    max_w = max_w.max(visual_width(cell));
                }
            }
            // Minimum width 4
            col_widths.push(max_w.max(4));
        }

        let (tl, tr, bl, br, hz, vt, tj, bj, lj, rj, xj) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│', '┬', '┴', '├', '┤', '┼')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+', '+', '+', '+')
        };

        let mut out = String::new();

        if let Some(ref title) = self.title {
            out.push_str(&caps.bold(title));
            out.push('\n');
        }

        // Helper to format top border
        let mut top_border = String::new();
        top_border.push(tl);
        if self.show_row_numbers {
            top_border.push_str(&hz.to_string().repeat(row_num_width + 2));
            top_border.push(tj);
        }
        for (i, &w) in col_widths.iter().enumerate() {
            top_border.push_str(&hz.to_string().repeat(w + 2));
            if i + 1 < col_widths.len() {
                top_border.push(tj);
            } else {
                top_border.push(tr);
            }
        }
        out.push_str(&caps.dim(&top_border));
        out.push('\n');

        // Header Row
        out.push_str(&caps.dim(&vt.to_string()));
        if self.show_row_numbers {
            let pad = " ".repeat(row_num_width.saturating_sub(1));
            let header_idx = format!(" #{} ", pad);
            out.push_str(&caps.dim(&header_idx));
            out.push_str(&caps.dim(&vt.to_string()));
        }

        for (i, col) in self.columns.iter().enumerate() {
            let w = col_widths[i];
            let name_styled = caps.bold(&col.name);
            let (raw_header, styled_header) = if let Some(ref ty) = col.type_hint {
                let raw = format!("{} <{}>", col.name, ty);
                let styled = format!("{} {}", name_styled, caps.cyan(&format!("<{}>", ty)));
                (raw, styled)
            } else {
                (col.name.clone(), name_styled)
            };

            let vlen = visual_width(&raw_header);
            let pad = " ".repeat(w.saturating_sub(vlen));
            let cell_str = match col.alignment {
                TableAlignment::Left => format!(" {}{} ", styled_header, pad),
                TableAlignment::Right => format!(" {}{} ", pad, styled_header),
                TableAlignment::Center => {
                    let left_pad = pad.len() / 2;
                    let right_pad = pad.len() - left_pad;
                    format!(
                        " {}{}{} ",
                        " ".repeat(left_pad),
                        styled_header,
                        " ".repeat(right_pad)
                    )
                }
            };
            out.push_str(&cell_str);
            out.push_str(&caps.dim(&vt.to_string()));
        }
        out.push('\n');

        // Middle Divider
        let mut mid_divider = String::new();
        mid_divider.push(lj);
        if self.show_row_numbers {
            mid_divider.push_str(&hz.to_string().repeat(row_num_width + 2));
            mid_divider.push(xj);
        }
        for (i, &w) in col_widths.iter().enumerate() {
            mid_divider.push_str(&hz.to_string().repeat(w + 2));
            if i + 1 < col_widths.len() {
                mid_divider.push(xj);
            } else {
                mid_divider.push(rj);
            }
        }
        out.push_str(&caps.dim(&mid_divider));
        out.push('\n');

        // Data Rows
        for (row_idx, row) in display_rows {
            out.push_str(&caps.dim(&vt.to_string()));

            if row_idx == 0 {
                // Ellipsis row
                let ell = if caps.unicode_enabled { "…" } else { "..." };
                if self.show_row_numbers {
                    let pad = " ".repeat(row_num_width.saturating_sub(visual_width(ell)));
                    out.push_str(&format!(" {}{} ", pad, caps.dim(ell)));
                    out.push_str(&caps.dim(&vt.to_string()));
                }
                for &w in &col_widths {
                    let pad = " ".repeat(w.saturating_sub(visual_width(ell)));
                    out.push_str(&format!(" {}{} ", pad, caps.dim(ell)));
                    out.push_str(&caps.dim(&vt.to_string()));
                }
                out.push('\n');
                continue;
            }

            if self.show_row_numbers {
                let idx_str = row_idx.to_string();
                let pad = " ".repeat(row_num_width.saturating_sub(idx_str.len()));
                out.push_str(&format!(" {}{} ", pad, caps.dim(&idx_str)));
                out.push_str(&caps.dim(&vt.to_string()));
            }

            for (col_i, col) in self.columns.iter().enumerate() {
                let w = col_widths[col_i];
                let cell_val = row.get(col_i).cloned().unwrap_or_default();
                let vlen = visual_width(&cell_val);
                let pad = " ".repeat(w.saturating_sub(vlen));

                // Style NA
                let styled_val = if cell_val == "NA" || cell_val.starts_with("NA:") {
                    caps.yellow(&cell_val)
                } else {
                    cell_val.clone()
                };

                let cell_str = match col.alignment {
                    TableAlignment::Left => format!(" {}{} ", styled_val, pad),
                    TableAlignment::Right => format!(" {}{} ", pad, styled_val),
                    TableAlignment::Center => {
                        let left_pad = pad.len() / 2;
                        let right_pad = pad.len() - left_pad;
                        format!(
                            " {}{}{} ",
                            " ".repeat(left_pad),
                            styled_val,
                            " ".repeat(right_pad)
                        )
                    }
                };
                out.push_str(&cell_str);
                out.push_str(&caps.dim(&vt.to_string()));
            }
            out.push('\n');
        }

        // Bottom Border
        let mut bot_border = String::new();
        bot_border.push(bl);
        if self.show_row_numbers {
            bot_border.push_str(&hz.to_string().repeat(row_num_width + 2));
            bot_border.push(bj);
        }
        for (i, &w) in col_widths.iter().enumerate() {
            bot_border.push_str(&hz.to_string().repeat(w + 2));
            if i + 1 < col_widths.len() {
                bot_border.push(bj);
            } else {
                bot_border.push(br);
            }
        }
        out.push_str(&caps.dim(&bot_border));
        out.push('\n');

        // Dimension footer
        let mult = if caps.unicode_enabled { "×" } else { "x" };
        let footer = format!("[{} rows {} {} columns]", total_rows, mult, num_cols);
        out.push_str(&caps.dim(&footer));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_unicode_render() {
        let caps = RenderCaps::rich_terminal(80);
        let mut table = CockpitTable::new();
        table.add_column(
            TableColumn::new("id")
                .with_type("i64")
                .with_alignment(TableAlignment::Right),
        );
        table.add_column(
            TableColumn::new("city")
                .with_type("str")
                .with_alignment(TableAlignment::Left),
        );
        table.add_column(
            TableColumn::new("temp")
                .with_type("f64")
                .with_alignment(TableAlignment::Right),
        );

        table.add_row(vec!["1".into(), "Tokyo".into(), "18.5".into()]);
        table.add_row(vec!["2".into(), "Santiago".into(), "24.0".into()]);
        table.add_row(vec!["3".into(), "Kyoto".into(), "NA".into()]);

        let rendered = table.render(&caps);
        assert!(rendered.contains("╭"));
        assert!(rendered.contains("╯"));
        assert!(rendered.contains("Tokyo"));
        assert!(rendered.contains("Santiago"));
        assert!(rendered.contains("<f64>"));
        assert!(rendered.contains("[3 rows × 3 columns]"));
    }

    #[test]
    fn test_table_ascii_render() {
        let caps = RenderCaps::ascii_plain(80);
        let mut table = CockpitTable::new();
        table.add_column(TableColumn::new("col_a"));
        table.add_row(vec!["val1".into()]);

        let rendered = table.render(&caps);
        assert!(rendered.contains("+"));
        assert!(rendered.contains("|"));
        assert!(!rendered.contains("╭"));
        assert!(rendered.contains("[1 rows x 1 columns]"));
    }
}
