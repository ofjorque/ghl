//! Documentation comment parsing, semantic tag extraction, and documentation generation.
//!
//! Supports `///` doc comments on functions (`fn`), structs (`struct`), and traits (`trait`),
//! with semantic tags:
//! - `@param <name> <description>`
//! - `@return <description>` / `@returns <description>`
//! - `@formula <math>`
//! - `@example <code>`

use crate::ast::{Program, Span, StmtKind};
use ghl_diagnostics::{CockpitPanel, RenderCaps};

/// The kind of item being documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Function,
    Struct,
    Trait,
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemKind::Function => write!(f, "function"),
            ItemKind::Struct => write!(f, "struct"),
            ItemKind::Trait => write!(f, "trait"),
        }
    }
}

/// Parsed documentation record for a user-defined symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDoc {
    pub name: String,
    pub kind: ItemKind,
    pub signature: String,
    pub summary: String,
    pub description: String,
    pub formula: Option<String>,
    pub parameters: Vec<(String, String)>,
    pub returns: Option<String>,
    pub example: Option<String>,
    pub raw_markdown: String,
    pub has_doc: bool,
    pub span: Span,
}

impl ItemDoc {
    /// Renders the item documentation as a rich Markdown string suitable for LSP hover tooltips.
    pub fn to_hover_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!("```ghl\n{}\n```\n\n", self.signature));

        if !self.summary.is_empty() {
            md.push_str(&format!("**{}**\n\n", self.summary));
        }

        if let Some(ref formula) = self.formula {
            md.push_str(&format!("$$\n{}\n$$\n\n", formula));
        }

        if !self.description.is_empty() {
            md.push_str(&format!("{}\n\n", self.description));
        }

        if !self.parameters.is_empty() {
            md.push_str("**Parameters:**\n");
            for (param, desc) in &self.parameters {
                md.push_str(&format!("- `{}`: {}\n", param, desc));
            }
            md.push('\n');
        }

        if let Some(ref ret) = self.returns {
            md.push_str(&format!("**Returns:** `{}`\n\n", ret));
        }

        if let Some(ref ex) = self.example {
            md.push_str("**Example:**\n```ghl\n");
            md.push_str(ex.trim());
            md.push_str("\n```\n");
        }

        if !self.has_doc {
            md.push_str(&format!("*User-defined {}*\n", self.kind));
        }

        md
    }

    /// Renders documentation as a Cockpit Panel styled with ANSI colors for REPL (`?fun`).
    pub fn render_cockpit(&self, caps: &RenderCaps) -> String {
        let mut panel = CockpitPanel::new(format!("{}: {}()", self.kind, self.name));
        panel.with_badge("USER_DOC");

        if !self.summary.is_empty() {
            panel.add_line(&self.summary);
            panel.add_divider();
        }

        panel.add_kv("Signature", &self.signature);
        if let Some(ref f) = self.formula {
            panel.add_kv("Formula", caps.bold(f));
        }
        if let Some(ref r) = self.returns {
            panel.add_kv("Returns", r);
        }

        if !self.description.is_empty() {
            panel.add_divider();
            panel.add_line(&self.description);
        }

        if !self.parameters.is_empty() {
            panel.add_divider();
            panel.add_line(caps.dim("Parameters:"));
            for (param, desc) in &self.parameters {
                panel.add_kv(format!("  {param}"), desc.as_str());
            }
        }

        if let Some(ref ex) = self.example {
            panel.add_divider();
            panel.add_line(caps.dim("Example:"));
            for line in ex.lines() {
                panel.add_line(format!("  {}", caps.cyan(line)));
            }
        }

        panel.render(caps)
    }

    /// Renders this item as a Markdown reference section.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("### `{}` ({})\n\n", self.name, self.kind));
        out.push_str(&format!("```ghl\n{}\n```\n\n", self.signature));

        if !self.summary.is_empty() {
            out.push_str(&format!("{}\n\n", self.summary));
        }

        if let Some(ref formula) = self.formula {
            out.push_str(&format!("$$\n{}\n$$\n\n", formula));
        }

        if !self.description.is_empty() {
            out.push_str(&format!("{}\n\n", self.description));
        }

        if !self.parameters.is_empty() {
            out.push_str("**Parámetros:**\n\n");
            for (p, desc) in &self.parameters {
                out.push_str(&format!("- `{}`: {}\n", p, desc));
            }
            out.push('\n');
        }

        if let Some(ref r) = self.returns {
            out.push_str(&format!("**Retorna:** `{}`\n\n", r));
        }

        if let Some(ref ex) = self.example {
            out.push_str("**Ejemplo de Código:**\n\n```ghl\n");
            out.push_str(ex.trim());
            out.push_str("\n```\n\n");
        }

        out.push_str("---\n\n");
        out
    }

    /// Renders this item as an HTML card.
    pub fn to_html(&self) -> String {
        let mut html = String::new();
        html.push_str(&format!(
            r#"<article class="doc-item" id="item-{}">
  <div class="item-header">
    <span class="badge badge-{}">{}</span>
    <h3><code>{}</code></h3>
  </div>
  <pre class="signature"><code>{}</code></pre>
"#,
            self.name,
            self.kind,
            self.kind,
            self.name,
            escape_html(&self.signature)
        ));

        if !self.summary.is_empty() {
            html.push_str(&format!(
                r#"  <p class="summary"><strong>{}</strong></p>
"#,
                escape_html(&self.summary)
            ));
        }

        if let Some(ref f) = self.formula {
            html.push_str(&format!(
                r#"  <div class="formula-box"><code>$$ {} $$</code></div>
"#,
                escape_html(f)
            ));
        }

        if !self.description.is_empty() {
            html.push_str(&format!(
                r#"  <div class="description"><p>{}</p></div>
"#,
                escape_html(&self.description)
            ));
        }

        if !self.parameters.is_empty() {
            html.push_str(
                r#"  <h4>Parameters</h4>
  <ul class="params-list">
"#,
            );
            for (param, desc) in &self.parameters {
                html.push_str(&format!(
                    r#"    <li><code>{}</code> — {}</li>
"#,
                    escape_html(param),
                    escape_html(desc)
                ));
            }
            html.push_str("  </ul>\n");
        }

        if let Some(ref r) = self.returns {
            html.push_str(&format!(
                r#"  <p class="returns"><strong>Returns:</strong> <code>{}</code></p>
"#,
                escape_html(r)
            ));
        }

        if let Some(ref ex) = self.example {
            html.push_str(&format!(
                r#"  <h4>Example</h4>
  <pre class="example"><code>{}</code></pre>
"#,
                escape_html(ex.trim())
            ));
        }

        html.push_str("</article>\n");
        html
    }
}

/// Helper to escape HTML special characters.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Extract lines of documentation comments preceding `stmt_start` in `source`.
pub fn extract_raw_doc_comment(source: &str, stmt_start: usize) -> Option<String> {
    if stmt_start == 0 || stmt_start > source.len() {
        return None;
    }

    let prefix = &source[..stmt_start];
    let lines: Vec<&str> = prefix.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let mut doc_lines: Vec<&str> = Vec::new();
    let mut i = lines.len();

    // Skip empty or purely whitespace lines right before the statement
    while i > 0 {
        let line = lines[i - 1].trim();
        if line.is_empty() {
            i -= 1;
        } else {
            break;
        }
    }

    // Accumulate consecutive lines starting with `///`
    while i > 0 {
        let line = lines[i - 1].trim_start();
        if line.starts_with("///") {
            let stripped = if line.starts_with("/// ") {
                &line[4..]
            } else if line.starts_with("///") {
                &line[3..]
            } else {
                line
            };
            doc_lines.push(stripped);
            i -= 1;
        } else {
            break;
        }
    }

    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    }
}

/// Parse a raw `///` doc comment text into structured fields.
pub fn parse_doc_comment_fields(
    raw: &str,
) -> (
    String,
    String,
    Option<String>,
    Vec<(String, String)>,
    Option<String>,
    Option<String>,
) {
    let mut summary = String::new();
    let mut desc_lines = Vec::new();
    let mut formula = None;
    let mut parameters = Vec::new();
    let mut returns = None;
    let mut example_lines = Vec::new();

    let mut in_example = false;
    let mut in_summary = true;

    for line in raw.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("@example") {
            in_example = true;
            let rem = trimmed.trim_start_matches("@example").trim();
            if !rem.is_empty() {
                example_lines.push(rem);
            }
            continue;
        }

        if trimmed.starts_with('@') {
            in_example = false;
            in_summary = false;

            if trimmed.starts_with("@param") {
                let rem = trimmed.trim_start_matches("@param").trim();
                let mut parts = rem.splitn(2, |c: char| c.is_whitespace() || c == ':');
                let param_name = parts.next().unwrap_or("").trim().to_string();
                let param_desc = parts
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches(':')
                    .trim()
                    .to_string();
                if !param_name.is_empty() {
                    parameters.push((param_name, param_desc));
                }
            } else if trimmed.starts_with("@returns") || trimmed.starts_with("@return") {
                let rem = if trimmed.starts_with("@returns") {
                    trimmed.trim_start_matches("@returns").trim()
                } else {
                    trimmed.trim_start_matches("@return").trim()
                };
                returns = Some(rem.trim_start_matches(':').trim().to_string());
            } else if trimmed.starts_with("@formula") {
                let rem = trimmed.trim_start_matches("@formula").trim();
                formula = Some(rem.to_string());
            }
            continue;
        }

        if in_example {
            example_lines.push(line);
        } else if in_summary {
            if trimmed.is_empty() {
                if !summary.is_empty() {
                    in_summary = false;
                }
            } else {
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(trimmed);
            }
        } else if !trimmed.is_empty() {
            desc_lines.push(trimmed);
        }
    }

    let description = desc_lines.join(" ");
    let example = if example_lines.is_empty() {
        None
    } else {
        Some(example_lines.join("\n"))
    };

    (summary, description, formula, parameters, returns, example)
}

/// Extract all documented (and undocumented) top-level items from a program and its source text.
pub fn extract_doc_comments(source: &str, program: &Program) -> Vec<ItemDoc> {
    let mut docs = Vec::new();

    for stmt in &program.statements {
        let raw_doc = extract_raw_doc_comment(source, stmt.span.start);
        let has_doc = raw_doc.is_some();
        let raw_text = raw_doc.unwrap_or_default();

        let (summary, description, formula, parameters, returns, example) =
            parse_doc_comment_fields(&raw_text);

        match &stmt.kind {
            StmtKind::Fn {
                name,
                params,
                ret_ty,
                export_ffi,
                ..
            } => {
                let mut sig = String::new();
                if *export_ffi {
                    // Mirror `fmt.rs`'s canonical rendering: `#[export_ffi]` is the
                    // only concrete syntax for this flag, so the signature shown in
                    // hover/docs should look like real, parseable GHL source.
                    sig.push_str("#[export_ffi]\n");
                }
                sig.push_str(&format!("fn {}(", name));
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        sig.push_str(", ");
                    }
                    sig.push_str(&p.name);
                    if let Some(ty) = &p.ty {
                        sig.push_str(&format!(": {}", ty));
                    }
                }
                sig.push(')');
                if let Some(ret) = ret_ty {
                    sig.push_str(&format!(" -> {}", ret));
                }

                docs.push(ItemDoc {
                    name: name.clone(),
                    kind: ItemKind::Function,
                    signature: sig,
                    summary,
                    description,
                    formula,
                    parameters,
                    returns,
                    example,
                    raw_markdown: raw_text,
                    has_doc,
                    span: stmt.span.clone(),
                });
            }
            StmtKind::Struct(decl) => {
                let mut sig = format!("struct {} {{\n", decl.name);
                for f in &decl.fields {
                    sig.push_str(&format!("    {}: {},\n", f.name, f.ty));
                }
                sig.push('}');

                docs.push(ItemDoc {
                    name: decl.name.clone(),
                    kind: ItemKind::Struct,
                    signature: sig,
                    summary,
                    description,
                    formula,
                    parameters,
                    returns,
                    example,
                    raw_markdown: raw_text,
                    has_doc,
                    span: stmt.span.clone(),
                });
            }
            StmtKind::Trait(decl) => {
                let mut sig = format!("trait {} {{\n", decl.name);
                for item in &decl.items {
                    match item {
                        crate::ast::TraitItem::Method(m) => {
                            sig.push_str(&format!("    fn {}(...);\n", m.name));
                        }
                        crate::ast::TraitItem::AssociatedType(t) => {
                            sig.push_str(&format!("    type {};\n", t));
                        }
                    }
                }
                sig.push('}');

                docs.push(ItemDoc {
                    name: decl.name.clone(),
                    kind: ItemKind::Trait,
                    signature: sig,
                    summary,
                    description,
                    formula,
                    parameters,
                    returns,
                    example,
                    raw_markdown: raw_text,
                    has_doc,
                    span: stmt.span.clone(),
                });
            }
            _ => {}
        }
    }

    docs
}

/// Generate a complete Markdown document for a collection of items.
pub fn generate_project_docs_markdown(title: &str, items: &[ItemDoc]) -> String {
    let mut md = String::new();
    md.push_str(&format!("# Referencia de API — {}\n\n", title));
    md.push_str("> Generado automáticamente por `ghl doc` (Gojo & Haru Documentation Deck).\n\n");

    let documented_count = items.iter().filter(|i| i.has_doc).count();
    let total_count = items.len();
    let pct = if total_count > 0 {
        (documented_count as f64 / total_count as f64) * 100.0
    } else {
        100.0
    };

    md.push_str(&format!(
        "**Estadísticas de Cobertura:** {}/{} ítems documentados ({:.1}%)\n\n",
        documented_count, total_count, pct
    ));

    md.push_str("## Índice de Símbolos\n\n");
    for item in items {
        md.push_str(&format!(
            "- [`{}()`](#{}) ({})\n",
            item.name, item.name, item.kind
        ));
    }
    md.push_str("\n---\n\n");

    md.push_str("## Declaraciones\n\n");
    for item in items {
        md.push_str(&item.to_markdown());
    }

    md
}

/// Generate a complete standalone HTML document with embedded CSS.
pub fn generate_project_docs_html(title: &str, items: &[ItemDoc]) -> String {
    let documented_count = items.iter().filter(|i| i.has_doc).count();
    let total_count = items.len();
    let pct = if total_count > 0 {
        (documented_count as f64 / total_count as f64) * 100.0
    } else {
        100.0
    };

    let mut cards = String::new();
    for item in items {
        cards.push_str(&item.to_html());
    }

    let mut sidebar_items = String::new();
    for item in items {
        sidebar_items.push_str(&format!(
            "        <li><a href=\"#item-{}\"><code>{}</code> <span class=\"badge-sm badge-{}\">{}</span></a></li>\n",
            item.name, item.name, item.kind, item.kind
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title} — GHL Documentation</title>
  <style>
    :root {{
      --bg: #0f172a;
      --card-bg: #1e293b;
      --border: #334155;
      --text: #f8fafc;
      --text-muted: #94a3b8;
      --primary: #38bdf8;
      --primary-dim: #0284c7;
      --accent: #a855f7;
      --badge-fn: #38bdf8;
      --badge-struct: #34d399;
      --badge-trait: #f59e0b;
      --code-bg: #090d16;
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
      background: var(--bg);
      color: var(--text);
      line-height: 1.6;
      display: flex;
      min-height: 100vh;
    }}
    nav.sidebar {{
      width: 280px;
      background: #090d16;
      border-right: 1px solid var(--border);
      padding: 1.5rem 1rem;
      position: sticky;
      top: 0;
      height: 100vh;
      overflow-y: auto;
    }}
    nav.sidebar h2 {{
      font-size: 1.1rem;
      color: var(--primary);
      margin-bottom: 0.5rem;
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }}
    nav.sidebar ul {{ list-style: none; margin-top: 1rem; }}
    nav.sidebar li {{ margin-bottom: 0.4rem; }}
    nav.sidebar a {{
      color: var(--text-muted);
      text-decoration: none;
      font-size: 0.9rem;
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 0.3rem 0.5rem;
      border-radius: 4px;
    }}
    nav.sidebar a:hover {{ background: var(--border); color: var(--text); }}
    main.content {{
      flex: 1;
      padding: 2.5rem 3.5rem;
      max-width: 1000px;
      overflow-y: auto;
    }}
    header.doc-header {{
      margin-bottom: 2rem;
      border-bottom: 1px solid var(--border);
      padding-bottom: 1.5rem;
    }}
    header.doc-header h1 {{ font-size: 2.2rem; color: #fff; margin-bottom: 0.5rem; }}
    .stats-bar {{
      display: inline-block;
      background: #1e293b;
      padding: 0.4rem 0.8rem;
      border-radius: 6px;
      border: 1px solid var(--border);
      font-size: 0.85rem;
      color: var(--primary);
    }}
    article.doc-item {{
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 1.5rem;
      margin-bottom: 2rem;
    }}
    .item-header {{
      display: flex;
      align-items: center;
      gap: 0.8rem;
      margin-bottom: 0.8rem;
    }}
    .badge {{
      text-transform: uppercase;
      font-size: 0.7rem;
      font-weight: bold;
      padding: 0.2rem 0.5rem;
      border-radius: 4px;
    }}
    .badge-sm {{ font-size: 0.65rem; padding: 0.1rem 0.3rem; border-radius: 3px; }}
    .badge-function {{ background: rgba(56, 189, 248, 0.2); color: var(--badge-fn); }}
    .badge-struct {{ background: rgba(52, 211, 153, 0.2); color: var(--badge-struct); }}
    .badge-trait {{ background: rgba(245, 158, 11, 0.2); color: var(--badge-trait); }}
    pre.signature {{
      background: var(--code-bg);
      padding: 0.8rem 1rem;
      border-radius: 6px;
      border: 1px solid var(--border);
      color: #38bdf8;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 0.95rem;
      margin-bottom: 1rem;
      overflow-x: auto;
    }}
    .summary {{ font-size: 1.05rem; margin-bottom: 0.8rem; }}
    .formula-box {{
      background: rgba(168, 85, 247, 0.1);
      border-left: 3px solid var(--accent);
      padding: 0.6rem 1rem;
      margin-bottom: 1rem;
      font-family: monospace;
    }}
    ul.params-list {{ margin: 0.5rem 0 1rem 1.5rem; }}
    ul.params-list li {{ margin-bottom: 0.3rem; }}
    pre.example {{
      background: var(--code-bg);
      padding: 0.8rem 1rem;
      border-radius: 6px;
      border: 1px solid var(--border);
      color: #e2e8f0;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 0.9rem;
      margin-top: 0.5rem;
      overflow-x: auto;
    }}
    h4 {{ font-size: 0.95rem; color: var(--text-muted); margin-top: 1rem; text-transform: uppercase; letter-spacing: 0.05em; }}
  </style>
</head>
<body>
  <nav class="sidebar">
    <h2>ฅ(•⩊ •マ Haru Docs</h2>
    <p style="font-size: 0.8rem; color: var(--text-muted); margin-bottom: 1rem;">GHL Documentation Deck</p>
    <ul>
{sidebar_items}    </ul>
  </nav>
  <main class="content">
    <header class="doc-header">
      <h1>{title}</h1>
      <div class="stats-bar">
        Coverage: {documented_count}/{total_count} items documented ({pct:.1}%)
      </div>
    </header>
    <section class="doc-list">
{cards}    </section>
  </main>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_parse_doc_comment_tags() {
        let raw = r#"Sample arithmetic mean with Kleene NA propagation.

Computes the sample mean of a numeric vector.

@param x Numeric vector
@return f64 scalar or NA
@formula x̄ = (1/n) Σ x_i
@example
let v = [1.0, 2.0, 3.0];
mean(v);
"#;
        let (summary, desc, formula, params, ret, example) = parse_doc_comment_fields(raw);
        assert_eq!(
            summary,
            "Sample arithmetic mean with Kleene NA propagation."
        );
        assert_eq!(desc, "Computes the sample mean of a numeric vector.");
        assert_eq!(formula.as_deref(), Some("x̄ = (1/n) Σ x_i"));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "x");
        assert_eq!(params[0].1, "Numeric vector");
        assert_eq!(ret.as_deref(), Some("f64 scalar or NA"));
        assert!(example.unwrap().contains("let v = [1.0, 2.0, 3.0];"));
    }

    #[test]
    fn test_extract_doc_comments_from_program() {
        let code = r#"
/// Robust regression estimator using IRLS.
///
/// @param formula Model formula specification
/// @param data Input dataframe
/// @return Fitted ModelFit object
fn fit_custom(formula: Formula, data: DataFrame) -> ModelFit {
    42
}

struct Point {
    x: f64,
    y: f64,
}
"#;
        let prog = parse(code).expect("syntax valid");
        let docs = extract_doc_comments(code, &prog);
        assert_eq!(docs.len(), 2);

        let fn_doc = &docs[0];
        assert_eq!(fn_doc.name, "fit_custom");
        assert_eq!(fn_doc.kind, ItemKind::Function);
        assert!(fn_doc.has_doc);
        assert_eq!(fn_doc.summary, "Robust regression estimator using IRLS.");
        assert_eq!(fn_doc.parameters.len(), 2);
        assert_eq!(fn_doc.parameters[0].0, "formula");
        assert_eq!(fn_doc.returns.as_deref(), Some("Fitted ModelFit object"));

        let struct_doc = &docs[1];
        assert_eq!(struct_doc.name, "Point");
        assert_eq!(struct_doc.kind, ItemKind::Struct);
        assert!(!struct_doc.has_doc); // No doc comment provided
    }

    /// Regression test: the rendered signature for an `#[export_ffi]`
    /// function must show the real, parseable attribute syntax - it used to
    /// print a bare "export " prefix that isn't valid GHL and doesn't even
    /// mention FFI, disagreeing with both the actual source and `fmt.rs`'s
    /// canonical rendering of the same flag.
    #[test]
    fn test_extract_doc_comments_export_ffi_signature() {
        let code = "#[export_ffi]\nfn calc(a: int, b: int) -> int {\n    a + b\n}";
        let prog = parse(code).expect("syntax valid");
        let docs = extract_doc_comments(code, &prog);

        let fn_doc = docs
            .iter()
            .find(|d| d.name == "calc")
            .expect("calc must be documented");
        assert!(fn_doc.signature.contains("#[export_ffi]"));
        assert!(fn_doc.signature.contains("fn calc(a: int, b: int) -> int"));
    }

    #[test]
    fn test_generate_markdown_and_html() {
        let code = r#"
/// Compute euclidean distance between two points.
///
/// @param p1 First point
/// @param p2 Second point
/// @return f64 distance
/// @formula d = √((x2-x1)² + (y2-y1)²)
fn distance(p1: Point, p2: Point) -> f64 {
    0.0
}
"#;
        let prog = parse(code).expect("syntax valid");
        let docs = extract_doc_comments(code, &prog);
        let md = generate_project_docs_markdown("Geometry Package", &docs);
        assert!(md.contains("# Referencia de API — Geometry Package"));
        assert!(md.contains("`distance`"));
        assert!(md.contains("d = √((x2-x1)² + (y2-y1)²)"));

        let html = generate_project_docs_html("Geometry Package", &docs);
        assert!(html.contains("Geometry Package — GHL Documentation"));
        assert!(html.contains("Haru Docs"));
        assert!(html.contains("item-distance"));
    }
}
