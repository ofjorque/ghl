//! Code intelligence engine: Hover documentation, autocompletion, and navigation.

use tower_lsp::lsp_types::*;
use ghl_runtime::doc::{lookup_doc, all_docs, FunctionDoc};
use ghl_syntax::ast::{Program, StmtKind, ExprKind};
use ghl_syntax::source::SourceIndex;

/// Extract word at a given (0-based line, 0-based character) position.
pub fn ident_at_position(text: &str, line: usize, col: usize) -> Option<String> {
    let line_str = text.lines().nth(line)?;
    let chars: Vec<char> = line_str.chars().collect();
    if chars.is_empty() {
        return None;
    }

    let col = col.min(chars.len());
    let target_idx = if col < chars.len() && (chars[col].is_alphanumeric() || chars[col] == '_') {
        col
    } else if col > 0 && (chars[col - 1].is_alphanumeric() || chars[col - 1] == '_') {
        col - 1
    } else {
        return None;
    };

    let mut start = target_idx;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    let mut end = target_idx;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    let word: String = chars[start..end].iter().collect();
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

/// Format a `FunctionDoc` as rich Markdown for hover tooltips.
pub fn format_function_doc(doc: &FunctionDoc) -> String {
    let mut md = String::new();
    md.push_str(&format!("```ghl\nfn {}\n```\n\n", doc.signature));
    md.push_str(&format!("**{}**\n\n", doc.summary));

    if let Some(formula) = doc.formula {
        md.push_str(&format!("$$\n{}\n$$\n\n", formula));
    }

    if !doc.description.is_empty() {
        md.push_str(&format!("{}\n\n", doc.description));
    }

    if !doc.parameters.is_empty() {
        md.push_str("**Parameters:**\n");
        for (param, desc) in doc.parameters {
            md.push_str(&format!("- `{}`: {}\n", param, desc));
        }
        md.push('\n');
    }

    if !doc.returns.is_empty() {
        md.push_str(&format!("**Returns:** `{}`\n\n", doc.returns));
    }

    if !doc.example.is_empty() {
        md.push_str("**Example:**\n```ghl\n");
        md.push_str(doc.example.trim());
        md.push_str("\n```\n");
    }

    md
}

/// Provide hover tooltip for an identifier at the given position.
pub fn compute_hover(text: &str, line: usize, col: usize, program: Option<&Program>) -> Option<Hover> {
    let ident = ident_at_position(text, line, col)?;

    // 1. Standard library documentation
    if let Some(doc) = lookup_doc(&ident) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format_function_doc(doc),
            }),
            range: None,
        });
    }

    // 2. User-defined symbols in the AST
    if let Some(prog) = program {
        for stmt in &prog.statements {
            match &stmt.kind {
                StmtKind::Fn { name, params, ret_ty, .. } if name == &ident => {
                    let mut sig = format!("fn {}(", name);
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

                    let val = format!("```ghl\n{}\n```\n\n*User-defined function*", sig);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: val,
                        }),
                        range: None,
                    });
                }
                StmtKind::Let { name, is_mut, ty, .. } if name == &ident => {
                    let mut decl = format!("let {}{}", if *is_mut { "mut " } else { "" }, name);
                    if let Some(ty) = ty {
                        decl.push_str(&format!(": {}", ty));
                    }
                    let val = format!("```ghl\n{}\n```\n\n*Local variable*", decl);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: val,
                        }),
                        range: None,
                    });
                }
                StmtKind::Struct(decl) if &decl.name == &ident => {
                    let mut val = format!("```ghl\nstruct {} {{\n", decl.name);
                    for f in &decl.fields {
                        val.push_str(&format!("    {}: {},\n", f.name, f.ty));
                    }
                    val.push_str("}\n```\n\n*User-defined struct*");
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: val,
                        }),
                        range: None,
                    });
                }
                _ => {}
            }
        }
    }

    None
}

const KEYWORDS: &[&str] = &[
    "let", "mut", "const", "fn", "struct", "enum", "trait", "impl", "type",
    "if", "else", "match", "while", "for", "in", "return", "break", "continue",
    "use", "pub", "mod", "as", "extern", "async", "await",
    "dataframe", "mat", "col", "NA", "true", "false",
];

/// Collect all completions (built-ins, keywords, local symbols, dataframe columns).
pub fn compute_completions(program: Option<&Program>) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // 1. Language keywords
    for &kw in KEYWORDS {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Keyword".to_string()),
            ..Default::default()
        });
    }

    // 2. Built-in functions with signatures and doc summaries
    for doc in all_docs() {
        items.push(CompletionItem {
            label: doc.name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(doc.signature.to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}**\n\n{}", doc.summary, doc.description),
            })),
            insert_text: Some(doc.name.to_string()),
            ..Default::default()
        });
    }

    // 3. User-defined symbols and DataFrame columns in AST
    if let Some(prog) = program {
        for stmt in &prog.statements {
            match &stmt.kind {
                StmtKind::Fn { name, params, ret_ty, .. } => {
                    let sig = format!("{}(...)", name);
                    let mut detail = sig.clone();
                    if let Some(r) = ret_ty {
                        detail.push_str(&format!(" -> {}", r));
                    }
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(detail),
                        ..Default::default()
                    });
                    for p in params {
                        items.push(CompletionItem {
                            label: p.name.clone(),
                            kind: Some(CompletionItemKind::VARIABLE),
                            detail: Some(format!("Parameter of {}", name)),
                            ..Default::default()
                        });
                    }
                }
                StmtKind::Let { name, ty, init, .. } => {
                    let detail = ty.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "variable".to_string());
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some(detail),
                        ..Default::default()
                    });

                    // Check if initializing a DataFrame to extract column names
                    extract_dataframe_columns(init, &mut items);
                }
                StmtKind::Struct(decl) => {
                    items.push(CompletionItem {
                        label: decl.name.clone(),
                        kind: Some(CompletionItemKind::STRUCT),
                        detail: Some("Struct declaration".to_string()),
                        ..Default::default()
                    });
                    for f in &decl.fields {
                        items.push(CompletionItem {
                            label: f.name.clone(),
                            kind: Some(CompletionItemKind::FIELD),
                            detail: Some(format!("Field of {}", decl.name)),
                            ..Default::default()
                        });
                    }
                }
                StmtKind::Expr(expr) => {
                    extract_dataframe_columns(expr, &mut items);
                }
                _ => {}
            }
        }
    }

    // Deduplicate by label
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items.dedup_by(|a, b| a.label == b.label);
    items
}

fn extract_dataframe_columns(expr: &ghl_syntax::ast::Expr, items: &mut Vec<CompletionItem>) {
    if let ExprKind::DataFrameLit(columns) = &expr.kind {
        for (col_name, _) in columns {
            items.push(CompletionItem {
                label: col_name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some("DataFrame column".to_string()),
                ..Default::default()
            });
        }
    }
}

/// Find definition location for an identifier under the cursor.
pub fn compute_definition(
    uri: &Url,
    text: &str,
    index: &SourceIndex,
    line: usize,
    col: usize,
    program: Option<&Program>,
) -> Option<Location> {
    let ident = ident_at_position(text, line, col)?;
    let prog = program?;

    for stmt in &prog.statements {
        match &stmt.kind {
            StmtKind::Fn { name, .. } if name == &ident => {
                let ((sl, sc), (el, ec)) = index.span_to_range(&stmt.span);
                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(sl, sc),
                        end: Position::new(el, ec),
                    },
                });
            }
            StmtKind::Let { name, .. } if name == &ident => {
                let ((sl, sc), (el, ec)) = index.span_to_range(&stmt.span);
                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(sl, sc),
                        end: Position::new(el, ec),
                    },
                });
            }
            StmtKind::Struct(decl) if &decl.name == &ident => {
                let ((sl, sc), (el, ec)) = index.span_to_range(&stmt.span);
                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(sl, sc),
                        end: Position::new(el, ec),
                    },
                });
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghl_syntax::parser::parse;

    #[test]
    fn test_ident_at_position() {
        let text = "let mean_val = mean(data);";
        assert_eq!(ident_at_position(text, 0, 4), Some("mean_val".to_string()));
        assert_eq!(ident_at_position(text, 0, 15), Some("mean".to_string()));
        assert_eq!(ident_at_position(text, 0, 21), Some("data".to_string()));
    }

    #[test]
    fn test_hover_stdlib_function() {
        let text = "let m = ols(y ~ x, df);";
        let hover = compute_hover(text, 0, 9, None).expect("hover for ols");
        if let HoverContents::Markup(content) = hover.contents {
            assert!(content.value.contains("ols"));
            assert!(content.value.contains("β̂") || content.value.contains("XᵀX"));
        } else {
            panic!("expected markdown content");
        }
    }

    #[test]
    fn test_hover_user_defined_function() {
        let text = "fn calculate(a: f64) -> f64 { a * 2.0 }\nlet res = calculate(10.0);";
        let program = parse(text).expect("syntax ok");
        let hover = compute_hover(text, 1, 12, Some(&program)).expect("hover for user fn");
        if let HoverContents::Markup(content) = hover.contents {
            assert!(content.value.contains("fn calculate(a: f64) -> f64"));
            assert!(content.value.contains("User-defined function"));
        } else {
            panic!("expected markdown content");
        }
    }

    #[test]
    fn test_completions_include_builtins_and_keywords() {
        let completions = compute_completions(None);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"let"));
        assert!(labels.contains(&"fn"));
        assert!(labels.contains(&"mean"));
        assert!(labels.contains(&"ols"));
        assert!(labels.contains(&"filter"));
    }

    #[test]
    fn test_definition_user_symbol() {
        let uri = Url::parse("file:///test.gh").unwrap();
        let text = "let target_var = 100;\nlet y = target_var + 5;";
        let index = SourceIndex::new(text);
        let program = parse(text).expect("syntax ok");
        let loc = compute_definition(&uri, text, &index, 1, 10, Some(&program)).expect("find definition");
        assert_eq!(loc.range.start.line, 0);
    }
}
