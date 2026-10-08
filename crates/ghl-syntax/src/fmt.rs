//! Canonical Code Formatter for GHL (RFC 12 & Roadmap 07).
//!
//! Provides AST-driven pretty-printing to normalize indentation to 4 spaces,
//! standardize binary operator spacing, and canonicalize dataframes, matrices,
//! pipelines, and control flow blocks while preserving comment trivia (leading,
//! inline, block, inside blocks) and intentional blank lines.

use crate::ast::*;
use crate::parser::parse;
use ghl_diagnostics::Diagnostic;
use std::fmt::Write;

/// A comment trivia token extracted from source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentTrivia {
    /// Full comment text, including `//`, `///`, or `/* ... */`.
    pub text: String,
    /// Start byte offset in original source.
    pub start: usize,
    /// End byte offset in original source.
    pub end: usize,
    /// True if this is a doc comment (`///`).
    pub is_doc: bool,
    /// True if this is a block comment (`/* ... */`).
    pub is_block: bool,
}

/// Represents a byte range in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub start_offset: usize,
    pub end_offset: usize,
}

/// Extracts all line and block comments from source code, ignoring comments inside strings.
pub fn extract_comments(source: &str) -> Vec<CommentTrivia> {
    let mut comments = Vec::new();
    let bytes = source.as_bytes();
    let n = bytes.len();
    let mut i = 0;

    while i < n {
        if bytes[i] == b'"' {
            i += 1;
            while i < n {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            let start = i;
            let is_doc = i + 2 < n && bytes[i + 2] == b'/' && (i + 3 >= n || bytes[i + 3] != b'/');
            i += 2;
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            let end = i;
            let text = source[start..end].trim_end_matches('\r').to_string();
            comments.push(CommentTrivia {
                text,
                start,
                end,
                is_doc,
                is_block: false,
            });
        } else if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let start = i;
            i += 2;
            while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            let end = if i + 1 < n {
                i += 2;
                i
            } else {
                n
            };
            let text = source[start..end].to_string();
            comments.push(CommentTrivia {
                text,
                start,
                end,
                is_doc: false,
                is_block: true,
            });
        } else {
            i += 1;
        }
    }

    comments
}

/// Checks if there is at least one blank line (>= 2 newlines with optional spaces/tabs)
/// preceding `offset` in `source`, stopping at the previous non-whitespace character.
pub fn has_blank_line_before(source: &str, offset: usize) -> bool {
    let prefix = &source[..offset.min(source.len())];
    let mut newline_count = 0;
    for b in prefix.bytes().rev() {
        if b == b'\n' {
            newline_count += 1;
            if newline_count >= 2 {
                return true;
            }
        } else if b != b' ' && b != b'\t' && b != b'\r' {
            break;
        }
    }
    false
}

/// State tracker for comment trivia during formatting.
#[derive(Debug, Clone)]
pub struct CommentState<'a> {
    pub source: &'a str,
    pub comments: Vec<CommentTrivia>,
    pub cursor: usize,
}

impl<'a> CommentState<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            comments: extract_comments(source),
            cursor: 0,
        }
    }

    pub fn has_comment_before(&self, offset: usize) -> bool {
        self.cursor < self.comments.len() && self.comments[self.cursor].start < offset
    }

    pub fn next_comment_start(&self) -> Option<usize> {
        if self.cursor < self.comments.len() {
            Some(self.comments[self.cursor].start)
        } else {
            None
        }
    }

    /// Emits all comments that appear before `before_offset`.
    pub fn emit_leading(
        &mut self,
        out: &mut String,
        before_offset: usize,
        level: usize,
        is_first: bool,
    ) {
        while self.cursor < self.comments.len() {
            let c = &self.comments[self.cursor];
            if c.start >= before_offset {
                break;
            }

            if (!is_first || !out.is_empty()) && has_blank_line_before(self.source, c.start) {
                if !out.ends_with("\n\n") {
                    if out.ends_with('\n') {
                        out.push('\n');
                    } else if !out.is_empty() {
                        out.push_str("\n\n");
                    }
                }
            }

            indent(out, level);
            out.push_str(&c.text);
            out.push('\n');
            self.cursor += 1;
        }
    }

    /// Emits inline/trailing comments that start on the same line right after `after_offset`.
    pub fn emit_inline_comments(&mut self, out: &mut String, mut after_offset: usize) {
        while self.cursor < self.comments.len() {
            let c = &self.comments[self.cursor];
            if c.start >= after_offset && c.start <= self.source.len() {
                let between = &self.source[after_offset.min(self.source.len())..c.start];
                if !between.contains('\n')
                    && between
                        .chars()
                        .all(|ch| ch == ' ' || ch == '\t' || ch == ',' || ch == ';')
                {
                    out.push(' ');
                    out.push_str(&c.text);
                    after_offset = c.end;
                    self.cursor += 1;
                    continue;
                }
            }
            break;
        }
    }

    /// Emits all remaining comments at the end of the file.
    pub fn emit_remaining(&mut self, out: &mut String) {
        while self.cursor < self.comments.len() {
            let c = &self.comments[self.cursor];
            if !out.is_empty() && has_blank_line_before(self.source, c.start) {
                if !out.ends_with("\n\n") {
                    if out.ends_with('\n') {
                        out.push('\n');
                    } else {
                        out.push_str("\n\n");
                    }
                }
            } else if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&c.text);
            out.push('\n');
            self.cursor += 1;
        }
    }
}

/// Formats GHL source code into canonical style preserving comments and intentional blank lines.
pub fn format_source(source: &str) -> Result<String, Diagnostic> {
    let program =
        parse(source).map_err(|errs| Diagnostic::compute_error("P0100", errs.join("\n")))?;
    Ok(format_program_with_comments(&program.statements, source))
}

/// Formats a program AST while preserving comment trivia and blank lines from `source`.
pub fn format_program_with_comments(program: &[Stmt], source: &str) -> String {
    let mut state = CommentState::new(source);
    let mut out = format_statements_with_state(program, Some(&mut state), 0);
    state.emit_remaining(&mut out);

    let trimmed = out.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Pretty-prints a parsed AST `Program` (slice of `Stmt`) into canonical GHL without source trivia.
pub fn format_program(program: &[Stmt]) -> String {
    let out = format_statements_with_state(program, None, 0);
    let trimmed = out.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Formats a specific line range [start_line, end_line] (0-indexed) in `source`.
pub fn format_range(
    source: &str,
    start_line: usize,
    end_line: usize,
) -> Result<Option<(TextRange, String)>, Diagnostic> {
    let program =
        parse(source).map_err(|errs| Diagnostic::compute_error("P0100", errs.join("\n")))?;

    if program.statements.is_empty() {
        return Ok(None);
    }

    let index = crate::source::SourceIndex::new(source);
    let mut matching_indices = Vec::new();

    for (idx, stmt) in program.statements.iter().enumerate() {
        let (stmt_start_l, _) = index.offset_to_position(stmt.span.start);
        let (stmt_end_l, _) = index.offset_to_position(stmt.span.end);
        let s_l = stmt_start_l as usize;
        let e_l = stmt_end_l as usize;

        if e_l >= start_line && s_l <= end_line {
            matching_indices.push(idx);
        }
    }

    if matching_indices.is_empty() {
        return Ok(None);
    }

    let first_idx = matching_indices[0];
    let last_idx = *matching_indices.last().unwrap();
    let selected_stmts = &program.statements[first_idx..=last_idx];

    let all_comments = extract_comments(source);
    let mut start_offset = selected_stmts.first().unwrap().span.start;
    for c in all_comments.iter().rev() {
        if c.end <= start_offset {
            let (c_line, _) = index.offset_to_position(c.start);
            if c_line as usize >= start_line {
                start_offset = c.start;
            } else {
                break;
            }
        }
    }

    let last_stmt_end = selected_stmts.last().unwrap().span.end;
    let mut end_offset = last_stmt_end;
    for c in &all_comments {
        if c.start >= last_stmt_end {
            let between = &source[last_stmt_end..c.start];
            if !between.contains('\n') && between.chars().all(|ch| ch == ' ' || ch == '\t') {
                end_offset = c.end;
            }
            break;
        }
    }

    let (start_l, _) = index.offset_to_position(start_offset);
    let start_offset = index.line_start_offset(start_l as usize);
    let (end_l, _) = index.offset_to_position(end_offset);
    let end_offset = index.line_end_offset(end_l as usize);

    let relevant_comments: Vec<CommentTrivia> = all_comments
        .into_iter()
        .filter(|c| c.start >= start_offset && c.end <= end_offset)
        .collect();

    let mut state = CommentState {
        source,
        comments: relevant_comments,
        cursor: 0,
    };

    let formatted = format_statements_with_state(selected_stmts, Some(&mut state), 0);

    Ok(Some((
        TextRange {
            start_offset,
            end_offset,
        },
        formatted,
    )))
}

fn format_statements_with_state(
    stmts: &[Stmt],
    mut state: Option<&mut CommentState>,
    level: usize,
) -> String {
    let mut out = String::new();
    let mut prev_was_fn_or_decl = false;

    for (i, stmt) in stmts.iter().enumerate() {
        let is_fn_or_decl = matches!(
            stmt.kind,
            StmtKind::Fn { .. } | StmtKind::Struct(_) | StmtKind::Trait(_) | StmtKind::Impl(_)
        );

        if let Some(st) = state.as_deref_mut() {
            let has_leading = st.has_comment_before(stmt.span.start);
            let check_offset = if has_leading {
                st.next_comment_start().unwrap_or(stmt.span.start)
            } else {
                stmt.span.start
            };

            if i > 0 {
                if is_fn_or_decl
                    || prev_was_fn_or_decl
                    || has_blank_line_before(st.source, check_offset)
                {
                    if !out.ends_with("\n\n") {
                        if out.ends_with('\n') {
                            out.push('\n');
                        } else {
                            out.push_str("\n\n");
                        }
                    }
                } else if !out.ends_with('\n') {
                    out.push('\n');
                }
            }

            st.emit_leading(&mut out, stmt.span.start, level, i == 0);

            if has_blank_line_before(st.source, stmt.span.start)
                && !out.ends_with("\n\n")
                && !out.is_empty()
            {
                out.push('\n');
            }
        } else if i > 0 {
            if is_fn_or_decl || prev_was_fn_or_decl {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }

        format_stmt_internal(&mut out, stmt, level, state.as_deref_mut());

        if let Some(st) = state.as_deref_mut() {
            st.emit_inline_comments(&mut out, stmt.span.end);
        }

        out.push('\n');
        prev_was_fn_or_decl = is_fn_or_decl;
    }

    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

pub fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    format_stmt_internal(out, stmt, level, None);
}

pub fn format_expr(out: &mut String, expr: &Expr, level: usize) {
    format_expr_internal(out, expr, level, None);
}

fn format_stmt_internal(
    out: &mut String,
    stmt: &Stmt,
    level: usize,
    mut state: Option<&mut CommentState>,
) {
    indent(out, level);
    match &stmt.kind {
        StmtKind::Let {
            name,
            is_mut,
            ty,
            init,
        } => {
            out.push_str("let ");
            if *is_mut {
                out.push_str("mut ");
            }
            out.push_str(name);
            if let Some(t) = ty {
                out.push_str(": ");
                let _ = write!(out, "{}", t);
            }
            out.push_str(" = ");
            format_expr_internal(out, init, level, state.as_deref_mut());
            out.push(';');
        }
        StmtKind::Assign { name, op, value } => {
            out.push_str(name);
            out.push(' ');
            out.push_str(op.symbol());
            out.push(' ');
            format_expr_internal(out, value, level, state.as_deref_mut());
            out.push(';');
        }
        StmtKind::FieldAssign {
            target,
            fields,
            op,
            value,
        } => {
            out.push_str(target);
            for f in fields {
                out.push('.');
                out.push_str(f);
            }
            out.push(' ');
            out.push_str(op.symbol());
            out.push(' ');
            format_expr_internal(out, value, level, state.as_deref_mut());
            out.push(';');
        }
        StmtKind::IndexAssign {
            target,
            indices,
            op,
            value,
        } => {
            out.push_str(target);
            out.push('[');
            for (i, idx) in indices.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                match idx {
                    IndexSpec::Expr(e) => format_expr_internal(out, e, level, state.as_deref_mut()),
                    IndexSpec::All => out.push_str(".."),
                    IndexSpec::Range {
                        start,
                        end,
                        inclusive,
                    } => {
                        if let Some(s) = start {
                            format_expr_internal(out, s, level, state.as_deref_mut());
                        }
                        if *inclusive {
                            out.push_str("..=");
                        } else {
                            out.push_str("..");
                        }
                        if let Some(e) = end {
                            format_expr_internal(out, e, level, state.as_deref_mut());
                        }
                    }
                }
            }
            out.push_str("] ");
            out.push_str(op.symbol());
            out.push(' ');
            format_expr_internal(out, value, level, state.as_deref_mut());
            out.push(';');
        }
        StmtKind::Return(val) => {
            out.push_str("return");
            if let Some(e) = val {
                out.push(' ');
                format_expr_internal(out, e, level, state.as_deref_mut());
            }
            out.push(';');
        }
        StmtKind::Expr(e) => {
            format_expr_internal(out, e, level, state.as_deref_mut());
            out.push(';');
        }
        StmtKind::Use(u) => {
            out.push_str("use ");
            out.push_str(&u.path.join("::"));
            match &u.kind {
                UseKind::Glob => out.push_str("::*"),
                UseKind::Items(items) => {
                    out.push_str("::{");
                    for (idx, it) in items.iter().enumerate() {
                        if idx > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&it.name);
                        if let Some(alias) = &it.alias {
                            out.push_str(" as ");
                            out.push_str(alias);
                        }
                    }
                    out.push('}');
                }
            }
            out.push(';');
        }
        StmtKind::Fn {
            name,
            params,
            ret_ty,
            body,
            export_ffi,
        } => {
            if *export_ffi {
                out.push_str("#[export_ffi]\n");
                indent(out, level);
            }
            out.push_str("fn ");
            out.push_str(name);
            out.push('(');
            for (idx, p) in params.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&p.name);
                if let Some(t) = &p.ty {
                    out.push_str(": ");
                    let _ = write!(out, "{}", t);
                }
            }
            out.push(')');
            if let Some(rt) = ret_ty {
                out.push_str(" -> ");
                let _ = write!(out, "{}", rt);
            }
            out.push(' ');
            format_expr_internal(out, body, level, state.as_deref_mut());
        }
        StmtKind::Struct(s) => {
            out.push_str("struct ");
            out.push_str(&s.name);
            out.push_str(" {\n");
            for (idx, field) in s.fields.iter().enumerate() {
                if let Some(st) = state.as_deref_mut() {
                    st.emit_leading(out, field.span.start, level + 1, idx == 0);
                }
                indent(out, level + 1);
                out.push_str(&field.name);
                out.push_str(": ");
                let _ = write!(out, "{}", field.ty);
                out.push(',');
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, field.span.end);
                }
                out.push('\n');
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, stmt.span.end, level + 1, s.fields.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
        StmtKind::Trait(t) => {
            out.push_str("trait ");
            out.push_str(&t.name);
            out.push_str(" {\n");
            for (idx, item) in t.items.iter().enumerate() {
                match item {
                    TraitItem::Method(m) => {
                        if let Some(st) = state.as_deref_mut() {
                            st.emit_leading(out, m.span.start, level + 1, idx == 0);
                        }
                        indent(out, level + 1);
                        out.push_str("fn ");
                        out.push_str(&m.name);
                        out.push('(');
                        for (p_idx, p) in m.params.iter().enumerate() {
                            if p_idx > 0 {
                                out.push_str(", ");
                            }
                            out.push_str(&p.name);
                            if let Some(ty) = &p.ty {
                                out.push_str(": ");
                                let _ = write!(out, "{}", ty);
                            }
                        }
                        out.push(')');
                        if let Some(rt) = &m.ret_ty {
                            out.push_str(" -> ");
                            let _ = write!(out, "{}", rt);
                        }
                        out.push(';');
                        if let Some(st) = state.as_deref_mut() {
                            st.emit_inline_comments(out, m.span.end);
                        }
                        out.push('\n');
                    }
                    TraitItem::AssociatedType(at) => {
                        indent(out, level + 1);
                        out.push_str("type ");
                        out.push_str(at);
                        out.push_str(";\n");
                    }
                }
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, stmt.span.end, level + 1, t.items.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
        StmtKind::Impl(im) => {
            out.push_str("impl ");
            if let Some(tr) = &im.trait_name {
                out.push_str(tr);
                out.push_str(" for ");
            }
            out.push_str(&im.target_type);
            out.push_str(" {\n");
            for (idx, item) in im.items.iter().enumerate() {
                if idx > 0 {
                    out.push('\n');
                }
                match item {
                    ImplItem::Method {
                        name,
                        params,
                        ret_ty,
                        body,
                        span,
                    } => {
                        if let Some(st) = state.as_deref_mut() {
                            st.emit_leading(out, span.start, level + 1, idx == 0);
                        }
                        indent(out, level + 1);
                        out.push_str("fn ");
                        out.push_str(name);
                        out.push('(');
                        for (p_idx, p) in params.iter().enumerate() {
                            if p_idx > 0 {
                                out.push_str(", ");
                            }
                            out.push_str(&p.name);
                            if let Some(ty) = &p.ty {
                                out.push_str(": ");
                                let _ = write!(out, "{}", ty);
                            }
                        }
                        out.push(')');
                        if let Some(rt) = ret_ty {
                            out.push_str(" -> ");
                            let _ = write!(out, "{}", rt);
                        }
                        out.push(' ');
                        format_expr_internal(out, body, level + 1, state.as_deref_mut());
                        if let Some(st) = state.as_deref_mut() {
                            st.emit_inline_comments(out, span.end);
                        }
                        out.push('\n');
                    }
                    ImplItem::AssociatedType { name, ty, span } => {
                        if let Some(st) = state.as_deref_mut() {
                            st.emit_leading(out, span.start, level + 1, idx == 0);
                        }
                        indent(out, level + 1);
                        out.push_str("type ");
                        out.push_str(name);
                        out.push_str(" = ");
                        let _ = write!(out, "{}", ty);
                        out.push(';');
                        if let Some(st) = state.as_deref_mut() {
                            st.emit_inline_comments(out, span.end);
                        }
                        out.push('\n');
                    }
                }
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, stmt.span.end, level + 1, im.items.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
        StmtKind::Break => {
            out.push_str("break;");
        }
        StmtKind::Continue => {
            out.push_str("continue;");
        }
    }
}

fn format_expr_internal(
    out: &mut String,
    expr: &Expr,
    level: usize,
    mut state: Option<&mut CommentState>,
) {
    match &expr.kind {
        ExprKind::Lit(lit) => match lit {
            Literal::Int(n) => {
                let _ = write!(out, "{}", n);
            }
            Literal::Float(f) => {
                if f.fract() == 0.0 && !f.is_nan() && !f.is_infinite() {
                    let _ = write!(out, "{:.1}", f);
                } else {
                    let _ = write!(out, "{}", f);
                }
            }
            Literal::String(s) => {
                let _ = write!(out, "{:?}", s);
            }
            Literal::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
            }
            Literal::NA(reason) => match reason {
                None => out.push_str("NA"),
                Some(r) => {
                    out.push_str("NA:");
                    out.push_str(r);
                }
            },
        },
        ExprKind::Ident(name) => out.push_str(name),
        ExprKind::Path(segments) => out.push_str(&segments.join("::")),
        ExprKind::Placeholder => out.push('_'),
        ExprKind::UnaryNot(e) => {
            out.push('!');
            format_expr_internal(out, e, level, state.as_deref_mut());
        }
        ExprKind::UnaryNeg(e) => {
            out.push('-');
            format_expr_internal(out, e, level, state.as_deref_mut());
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let parent_prec = binary_op_precedence(*op);
            let need_parens_l = match &lhs.kind {
                ExprKind::Binary { op: child_op, .. } => {
                    binary_op_precedence(*child_op) < parent_prec
                }
                _ => false,
            };
            if need_parens_l {
                out.push('(');
            }
            format_expr_internal(out, lhs, level, state.as_deref_mut());
            if need_parens_l {
                out.push(')');
            }

            out.push(' ');
            out.push_str(binary_op_symbol(*op));
            out.push(' ');

            let need_parens_r = match &rhs.kind {
                ExprKind::Binary { op: child_op, .. } => {
                    let child_prec = binary_op_precedence(*child_op);
                    child_prec < parent_prec
                        || (child_prec == parent_prec
                            && (*op == BinaryOp::Sub || *op == BinaryOp::Div))
                }
                _ => false,
            };
            if need_parens_r {
                out.push('(');
            }
            format_expr_internal(out, rhs, level, state.as_deref_mut());
            if need_parens_r {
                out.push(')');
            }
        }
        ExprKind::Pipe {
            expr: lhs,
            target: rhs,
        } => {
            format_expr_internal(out, lhs, level, state.as_deref_mut());
            out.push_str(" |> ");
            format_expr_internal(out, rhs, level, state.as_deref_mut());
        }
        ExprKind::Call { callee, args } => {
            format_expr_internal(out, callee, level, state.as_deref_mut());
            out.push('(');
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                format_expr_internal(out, arg, level, state.as_deref_mut());
            }
            out.push(')');
        }
        ExprKind::NamedArg { name, value } => {
            out.push_str(name);
            out.push_str(" = ");
            format_expr_internal(out, value, level, state.as_deref_mut());
        }
        ExprKind::Block {
            stmts,
            expr: trailing,
        } => {
            out.push_str("{\n");
            let mut prev_was_fn_or_decl = false;
            for (i, stmt) in stmts.iter().enumerate() {
                let is_fn_or_decl = matches!(
                    stmt.kind,
                    StmtKind::Fn { .. }
                        | StmtKind::Struct(_)
                        | StmtKind::Trait(_)
                        | StmtKind::Impl(_)
                );

                if let Some(st) = state.as_deref_mut() {
                    let has_leading = st.has_comment_before(stmt.span.start);
                    let check_offset = if has_leading {
                        st.next_comment_start().unwrap_or(stmt.span.start)
                    } else {
                        stmt.span.start
                    };
                    if i > 0
                        && (is_fn_or_decl
                            || prev_was_fn_or_decl
                            || has_blank_line_before(st.source, check_offset))
                    {
                        if !out.ends_with("\n\n") {
                            out.push('\n');
                        }
                    }
                    st.emit_leading(out, stmt.span.start, level + 1, i == 0);
                    if has_blank_line_before(st.source, stmt.span.start)
                        && !out.ends_with("\n\n")
                        && !out.is_empty()
                    {
                        out.push('\n');
                    }
                } else if i > 0 && (is_fn_or_decl || prev_was_fn_or_decl) {
                    if !out.ends_with("\n\n") {
                        out.push('\n');
                    }
                }

                format_stmt_internal(out, stmt, level + 1, state.as_deref_mut());
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, stmt.span.end);
                }
                out.push('\n');
                prev_was_fn_or_decl = is_fn_or_decl;
            }

            if let Some(t) = trailing {
                if let Some(st) = state.as_deref_mut() {
                    let has_leading = st.has_comment_before(t.span.start);
                    let check_offset = if has_leading {
                        st.next_comment_start().unwrap_or(t.span.start)
                    } else {
                        t.span.start
                    };
                    if !stmts.is_empty() && has_blank_line_before(st.source, check_offset) {
                        if !out.ends_with("\n\n") {
                            out.push('\n');
                        }
                    }
                    st.emit_leading(out, t.span.start, level + 1, stmts.is_empty());
                    if has_blank_line_before(st.source, t.span.start)
                        && !out.ends_with("\n\n")
                        && !out.is_empty()
                    {
                        out.push('\n');
                    }
                }
                indent(out, level + 1);
                format_expr_internal(out, t, level + 1, state.as_deref_mut());
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, t.span.end);
                }
                out.push('\n');
            }

            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(
                    out,
                    expr.span.end,
                    level + 1,
                    stmts.is_empty() && trailing.is_none(),
                );
            }

            indent(out, level);
            out.push('}');
        }
        ExprKind::Formula {
            op,
            response,
            terms,
            parts,
        } => {
            format_expr_internal(out, response, level, state.as_deref_mut());
            out.push(' ');
            out.push_str(&op.to_string());
            out.push(' ');
            if parts.is_empty() {
                if terms.is_empty() {
                    out.push('1');
                } else {
                    for (idx, term) in terms.iter().enumerate() {
                        if idx > 0 {
                            out.push_str(" + ");
                        }
                        format_expr_internal(out, term, level, state.as_deref_mut());
                    }
                }
            } else {
                for (idx, part) in parts.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(" | ");
                    }
                    format_expr_internal(out, part, level, state.as_deref_mut());
                }
            }
        }
        ExprKind::SemSpec { equations } => {
            out.push_str("sem_spec {\n");
            for (idx, eq) in equations.iter().enumerate() {
                if let Some(st) = state.as_deref_mut() {
                    st.emit_leading(out, eq.span.start, level + 1, idx == 0);
                }
                indent(out, level + 1);
                format_expr_internal(out, eq, level + 1, state.as_deref_mut());
                out.push(';');
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, eq.span.end);
                }
                out.push('\n');
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, expr.span.end, level + 1, equations.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            out.push_str("if ");
            format_expr_internal(out, cond, level, state.as_deref_mut());
            out.push(' ');
            format_expr_internal(out, then_branch, level, state.as_deref_mut());
            if let Some(el) = else_branch {
                out.push_str(" else ");
                format_expr_internal(out, el, level, state.as_deref_mut());
            }
        }
        ExprKind::While { cond, body } => {
            out.push_str("while ");
            format_expr_internal(out, cond, level, state.as_deref_mut());
            out.push(' ');
            format_expr_internal(out, body, level, state.as_deref_mut());
        }
        ExprKind::For {
            var,
            start,
            end,
            body,
        } => {
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            format_expr_internal(out, start, level, state.as_deref_mut());
            out.push_str("..");
            format_expr_internal(out, end, level, state.as_deref_mut());
            out.push(' ');
            format_expr_internal(out, body, level, state.as_deref_mut());
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            format_expr_internal(out, start, level, state.as_deref_mut());
            if *inclusive {
                out.push_str("..=");
            } else {
                out.push_str("..");
            }
            format_expr_internal(out, end, level, state.as_deref_mut());
        }
        ExprKind::VectorLit(items) => {
            out.push('[');
            for (idx, it) in items.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                format_expr_internal(out, it, level, state.as_deref_mut());
            }
            out.push(']');
        }
        ExprKind::Comprehension {
            expr,
            clauses,
            condition,
        } => {
            out.push('[');
            format_expr_internal(out, expr, level, state.as_deref_mut());
            for (idx, clause) in clauses.iter().enumerate() {
                if idx == 0 {
                    out.push_str(" for ");
                } else {
                    out.push_str(", ");
                }
                out.push_str(&clause.var);
                out.push_str(" in ");
                format_expr_internal(out, &clause.iter, level, state.as_deref_mut());
            }
            if let Some(cond) = condition {
                out.push_str(" if ");
                format_expr_internal(out, cond, level, state.as_deref_mut());
            }
            out.push(']');
        }
        ExprKind::DataFrameLit(cols) => {
            out.push_str("dataframe {\n");
            for (idx, (name, col_expr)) in cols.iter().enumerate() {
                if let Some(st) = state.as_deref_mut() {
                    st.emit_leading(out, col_expr.span.start, level + 1, idx == 0);
                }
                indent(out, level + 1);
                out.push_str(name);
                out.push_str(": ");
                format_expr_internal(out, col_expr, level + 1, state.as_deref_mut());
                if idx + 1 < cols.len() {
                    out.push(',');
                }
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, col_expr.span.end);
                }
                out.push('\n');
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, expr.span.end, level + 1, cols.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
        ExprKind::MatrixLit { rows } => {
            out.push_str("mat [\n");
            for (r_idx, row) in rows.iter().enumerate() {
                indent(out, level + 1);
                for (c_idx, val) in row.iter().enumerate() {
                    if c_idx > 0 {
                        out.push_str(", ");
                    }
                    format_expr_internal(out, val, level + 1, state.as_deref_mut());
                }
                if r_idx + 1 < rows.len() {
                    out.push_str(" ;\n");
                } else {
                    out.push('\n');
                }
            }
            indent(out, level);
            out.push(']');
        }
        ExprKind::Lambda { params, body } => {
            out.push('|');
            out.push_str(&params.join(", "));
            out.push_str("| ");
            format_expr_internal(out, body, level, state.as_deref_mut());
        }
        ExprKind::RecordLit(fields) => {
            out.push_str("#[");
            for (idx, (name, f_expr)) in fields.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
                out.push_str(" = ");
                format_expr_internal(out, f_expr, level, state.as_deref_mut());
            }
            out.push(']');
        }
        ExprKind::StructLit { name, fields } => {
            out.push_str(name);
            out.push_str(" {\n");
            for (idx, (fname, fexpr)) in fields.iter().enumerate() {
                if let Some(st) = state.as_deref_mut() {
                    st.emit_leading(out, fexpr.span.start, level + 1, idx == 0);
                }
                indent(out, level + 1);
                out.push_str(fname);
                out.push_str(": ");
                format_expr_internal(out, fexpr, level + 1, state.as_deref_mut());
                if idx + 1 < fields.len() {
                    out.push(',');
                }
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, fexpr.span.end);
                }
                out.push('\n');
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, expr.span.end, level + 1, fields.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
        ExprKind::FieldAccess { target, field } => {
            format_expr_internal(out, target, level, state.as_deref_mut());
            out.push('.');
            out.push_str(field);
        }
        ExprKind::Index { target, indices } => {
            format_expr_internal(out, target, level, state.as_deref_mut());
            out.push('[');
            for (idx, spec) in indices.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                match spec {
                    IndexSpec::Expr(e) => format_expr_internal(out, e, level, state.as_deref_mut()),
                    IndexSpec::All => out.push_str(".."),
                    IndexSpec::Range {
                        start,
                        end,
                        inclusive,
                    } => {
                        if let Some(s) = start {
                            format_expr_internal(out, s, level, state.as_deref_mut());
                        }
                        if *inclusive {
                            out.push_str("..=");
                        } else {
                            out.push_str("..");
                        }
                        if let Some(e) = end {
                            format_expr_internal(out, e, level, state.as_deref_mut());
                        }
                    }
                }
            }
            out.push(']');
        }
        ExprKind::Match {
            expr: match_expr,
            arms,
        } => {
            out.push_str("match ");
            format_expr_internal(out, match_expr, level, state.as_deref_mut());
            out.push_str(" {\n");
            for (idx, arm) in arms.iter().enumerate() {
                if let Some(st) = state.as_deref_mut() {
                    st.emit_leading(out, arm.span.start, level + 1, idx == 0);
                }
                indent(out, level + 1);
                format_pattern(out, &arm.pattern);
                if let Some(guard) = &arm.guard {
                    out.push_str(" if ");
                    format_expr_internal(out, guard, level + 1, state.as_deref_mut());
                }
                out.push_str(" => ");
                format_expr_internal(out, &arm.body, level + 1, state.as_deref_mut());
                out.push(',');
                if let Some(st) = state.as_deref_mut() {
                    st.emit_inline_comments(out, arm.span.end);
                }
                out.push('\n');
            }
            if let Some(st) = state.as_deref_mut() {
                st.emit_leading(out, expr.span.end, level + 1, arms.is_empty());
            }
            indent(out, level);
            out.push('}');
        }
    }
}

fn format_pattern(out: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Wildcard => out.push('_'),
        Pattern::Ident(name) => out.push_str(name),
        Pattern::Lit(lit) => match lit {
            Literal::Int(n) => {
                let _ = write!(out, "{}", n);
            }
            Literal::Float(f) => {
                let _ = write!(out, "{}", f);
            }
            Literal::String(s) => {
                let _ = write!(out, "{:?}", s);
            }
            Literal::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
            }
            Literal::NA(_) => out.push_str("NA"),
        },
        Pattern::NA => out.push_str("NA"),
        Pattern::NAReason(r) => {
            out.push_str("NA:");
            out.push_str(r);
        }
    }
}

fn binary_op_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Pow => "^",
        BinaryOp::MatSolve => "\\",
        BinaryOp::DotMul => ".*",
        BinaryOp::DotAdd => ".+",
        BinaryOp::DotSub => ".-",
        BinaryOp::DotDiv => "./",
        BinaryOp::Eq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::LtEq => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::GtEq => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitOr => "|",
    }
}

fn binary_op_precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::BitOr => 0,
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::LtEq
        | BinaryOp::Gt
        | BinaryOp::GtEq => 3,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::DotAdd | BinaryOp::DotSub => 4,
        BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::DotMul
        | BinaryOp::DotDiv
        | BinaryOp::MatSolve => 5,
        BinaryOp::Pow => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_basic_let_and_binary() {
        let code = "let x=1+2*3;";
        let formatted = format_source(code).expect("format ok");
        assert_eq!(formatted, "let x = 1 + 2 * 3;\n");
    }

    #[test]
    fn test_format_function_and_block() {
        let code = "fn add(a: f64, b: f64) -> f64 { a + b }";
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("fn add(a: f64, b: f64) -> f64"));
    }

    #[test]
    fn test_format_dataframe() {
        let code = "let df = dataframe { x: [1, 2], y: [10.0, 20.0] };";
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("dataframe {"));
        assert!(formatted.contains("    x: [1, 2],"));
    }

    #[test]
    fn test_format_export_ffi_round_trips() {
        let code = "#[export_ffi]\nfn calc(a: int, b: int) -> int { a + b }";
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("#[export_ffi]"));

        let reparsed = crate::parser::parse(&formatted).expect("formatted output must still parse");
        match &reparsed.statements[0].kind {
            StmtKind::Fn {
                name, export_ffi, ..
            } => {
                assert_eq!(name, "calc");
                assert!(*export_ffi, "export_ffi flag must survive the round trip");
            }
            other => panic!("expected StmtKind::Fn, got {other:?}"),
        }
    }

    #[test]
    fn test_format_export_ffi_is_idempotent() {
        let code = "#[export_ffi]\nfn calc(a: int, b: int) -> int { a + b }";
        let once = format_source(code).expect("format ok");
        let twice = format_source(&once).expect("re-format ok");
        assert_eq!(once, twice);
    }

    #[test]
    fn test_format_preserves_leading_and_doc_comments() {
        let code = r#"
// Calculation utility module
/// Adds two numbers together.
/// @param a First value
/// @param b Second value
fn add(a: int, b: int) -> int {
    a + b
}
"#;
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("// Calculation utility module"));
        assert!(formatted.contains("/// Adds two numbers together."));
        assert!(formatted.contains("/// @param a First value"));
        assert!(formatted.contains("/// @param b Second value"));
        assert!(formatted.contains("fn add(a: int, b: int) -> int"));
    }

    #[test]
    fn test_format_preserves_inline_comments() {
        let code = "let x = 10; // set initial count\nlet y = x * 2; /* multiplier */\n";
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("let x = 10; // set initial count\n"));
        assert!(formatted.contains("let y = x * 2; /* multiplier */\n"));
    }

    #[test]
    fn test_format_preserves_comments_inside_block() {
        let code = r#"fn compute() -> int {
    // step 1
    let x = 1;
    // step 2
    let y = 2;
    // final step
    x + y
    // end of function note
}"#;
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("    // step 1\n    let x = 1;"));
        assert!(formatted.contains("    // step 2\n    let y = 2;"));
        assert!(formatted.contains("    // final step\n    x + y"));
        assert!(formatted.contains("    // end of function note\n}"));
    }

    #[test]
    fn test_format_preserves_comments_inside_struct() {
        let code = r#"struct Person {
    // Unique user identifier
    id: int, // primary key
    // Display name
    name: string,
}"#;
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("    // Unique user identifier\n    id: int, // primary key"));
        assert!(formatted.contains("    // Display name\n    name: string,"));
    }

    #[test]
    fn test_format_file_with_only_comments() {
        let code = "// First line comment\n// Second line comment\n/* Block comment */\n";
        let formatted = format_source(code).expect("format ok");
        assert_eq!(formatted, code);
    }

    #[test]
    fn test_format_intentional_blank_lines_preservation() {
        let code = "let a = 1;\n\n// Group 2\nlet b = 2;\n\nlet c = 3;\n";
        let formatted = format_source(code).expect("format ok");
        assert_eq!(
            formatted,
            "let a = 1;\n\n// Group 2\nlet b = 2;\n\nlet c = 3;\n"
        );
    }

    #[test]
    fn test_format_excessive_blank_lines_normalized() {
        let code = "let a = 1;\n\n\n\n\nlet b = 2;\n";
        let formatted = format_source(code).expect("format ok");
        assert_eq!(formatted, "let a = 1;\n\nlet b = 2;\n");
    }

    #[test]
    fn test_format_idempotency_with_comments_and_blank_lines() {
        let code = r#"// Header comment

let x = 10; // inline note

// Section B
fn calc(val: int) -> int {
    // inner comment
    val * 2
}
"#;
        let once = format_source(code).expect("format ok");
        let twice = format_source(&once).expect("re-format ok");
        assert_eq!(once, twice, "Formatting must be idempotent");
    }

    #[test]
    fn test_format_range_partial_document() {
        let code = "let a = 1;\nlet b=2+3;\nlet c=4*5;\nlet d = 6;\n";
        // Format lines 1 to 2 (0-indexed: let b=2+3; and let c=4*5;)
        let result = format_range(code, 1, 2).expect("format range ok");
        assert!(result.is_some());
        let (range, snippet) = result.unwrap();
        assert_eq!(snippet, "let b = 2 + 3;\nlet c = 4 * 5;\n");
        assert_eq!(range.start_offset, 11); // line 1 start
    }
}
