//! Canonical Code Formatter for GHL (RFC 12 & Roadmap 07).
//!
//! Provides AST-driven pretty-printing to normalize indentation to 4 spaces,
//! standardize binary operator spacing, and canonicalize dataframes, matrices,
//! pipelines, and control flow blocks.

use std::fmt::Write;
use ghl_diagnostics::Diagnostic;
use crate::ast::*;
use crate::parser::parse;

/// Formats GHL source code into canonical style.
pub fn format_source(source: &str) -> Result<String, Diagnostic> {
    let program = parse(source).map_err(|errs| {
        Diagnostic::compute_error("P0100", errs.join("\n"))
    })?;
    Ok(format_program(&program.statements))
}

/// Pretty-prints a parsed AST `Program` (slice of `Stmt`) into canonical GHL.
pub fn format_program(program: &[Stmt]) -> String {
    let mut out = String::new();
    let mut prev_was_fn_or_decl = false;

    for (i, stmt) in program.iter().enumerate() {
        let is_fn_or_decl = matches!(
            stmt.kind,
            StmtKind::Fn { .. } | StmtKind::Struct(_) | StmtKind::Trait(_) | StmtKind::Impl(_)
        );

        if i > 0 {
            if is_fn_or_decl || prev_was_fn_or_decl {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }

        format_stmt(&mut out, stmt, 0);
        prev_was_fn_or_decl = is_fn_or_decl;
    }

    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }

    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    match &stmt.kind {
        StmtKind::Let { name, is_mut, ty, init } => {
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
            format_expr(out, init, level);
            out.push(';');
        }
        StmtKind::Assign { name, op, value } => {
            out.push_str(name);
            out.push(' ');
            out.push_str(op.symbol());
            out.push(' ');
            format_expr(out, value, level);
            out.push(';');
        }
        StmtKind::FieldAssign { target, fields, op, value } => {
            out.push_str(target);
            for f in fields {
                out.push('.');
                out.push_str(f);
            }
            out.push(' ');
            out.push_str(op.symbol());
            out.push(' ');
            format_expr(out, value, level);
            out.push(';');
        }
        StmtKind::IndexAssign { target, indices, op, value } => {
            out.push_str(target);
            out.push('[');
            for (i, idx) in indices.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                match idx {
                    IndexSpec::Expr(e) => format_expr(out, e, level),
                    IndexSpec::All => out.push_str(".."),
                    IndexSpec::Range { start, end, inclusive } => {
                        if let Some(s) = start {
                            format_expr(out, s, level);
                        }
                        if *inclusive {
                            out.push_str("..=");
                        } else {
                            out.push_str("..");
                        }
                        if let Some(e) = end {
                            format_expr(out, e, level);
                        }
                    }
                }
            }
            out.push_str("] ");
            out.push_str(op.symbol());
            out.push(' ');
            format_expr(out, value, level);
            out.push(';');
        }
        StmtKind::Return(val) => {
            out.push_str("return");
            if let Some(e) = val {
                out.push(' ');
                format_expr(out, e, level);
            }
            out.push(';');
        }
        StmtKind::Expr(e) => {
            format_expr(out, e, level);
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
        StmtKind::Fn { name, params, ret_ty, body, export_ffi } => {
            if *export_ffi {
                // `export_ffi` has no keyword-sequence syntax of its own - the only
                // grammar the parser accepts for it is the `#[export_ffi]` attribute
                // (or `extern "C"`, which this crate can't distinguish from the flag
                // alone). Emitting anything else here produces text the parser can't
                // read back, silently destroying the function on the next format.
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
            format_expr(out, body, level);
        }
        StmtKind::Struct(s) => {
            out.push_str("struct ");
            out.push_str(&s.name);
            out.push_str(" {\n");
            for field in &s.fields {
                indent(out, level + 1);
                out.push_str(&field.name);
                out.push_str(": ");
                let _ = write!(out, "{}", field.ty);
                out.push_str(",\n");
            }
            indent(out, level);
            out.push('}');
        }
        StmtKind::Trait(t) => {
            out.push_str("trait ");
            out.push_str(&t.name);
            out.push_str(" {\n");
            for item in &t.items {
                match item {
                    TraitItem::Method(m) => {
                        indent(out, level + 1);
                        out.push_str("fn ");
                        out.push_str(&m.name);
                        out.push('(');
                        for (idx, p) in m.params.iter().enumerate() {
                            if idx > 0 {
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
                        out.push_str(";\n");
                    }
                    TraitItem::AssociatedType(at) => {
                        indent(out, level + 1);
                        out.push_str("type ");
                        out.push_str(at);
                        out.push_str(";\n");
                    }
                }
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
                    ImplItem::Method { name, params, ret_ty, body, .. } => {
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
                        format_expr(out, body, level + 1);
                        out.push('\n');
                    }
                    ImplItem::AssociatedType { name, ty, .. } => {
                        indent(out, level + 1);
                        out.push_str("type ");
                        out.push_str(name);
                        out.push_str(" = ");
                        let _ = write!(out, "{}", ty);
                        out.push_str(";\n");
                    }
                }
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

fn format_expr(out: &mut String, expr: &Expr, level: usize) {
    match &expr.kind {
        ExprKind::Lit(lit) => match lit {
            Literal::Int(n) => { let _ = write!(out, "{}", n); }
            Literal::Float(f) => {
                if f.fract() == 0.0 && !f.is_nan() && !f.is_infinite() {
                    let _ = write!(out, "{:.1}", f);
                } else {
                    let _ = write!(out, "{}", f);
                }
            }
            Literal::String(s) => { let _ = write!(out, "{:?}", s); }
            Literal::Bool(b) => { out.push_str(if *b { "true" } else { "false" }); }
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
            format_expr(out, e, level);
        }
        ExprKind::UnaryNeg(e) => {
            out.push('-');
            format_expr(out, e, level);
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let parent_prec = binary_op_precedence(*op);
            let need_parens_l = match &lhs.kind {
                ExprKind::Binary { op: child_op, .. } => binary_op_precedence(*child_op) < parent_prec,
                _ => false,
            };
            if need_parens_l {
                out.push('(');
            }
            format_expr(out, lhs, level);
            if need_parens_l {
                out.push(')');
            }

            out.push(' ');
            out.push_str(binary_op_symbol(*op));
            out.push(' ');

            let need_parens_r = match &rhs.kind {
                ExprKind::Binary { op: child_op, .. } => {
                    let child_prec = binary_op_precedence(*child_op);
                    child_prec < parent_prec || (child_prec == parent_prec && (*op == BinaryOp::Sub || *op == BinaryOp::Div))
                }
                _ => false,
            };
            if need_parens_r {
                out.push('(');
            }
            format_expr(out, rhs, level);
            if need_parens_r {
                out.push(')');
            }
        }
        ExprKind::Pipe { expr: lhs, target: rhs } => {
            format_expr(out, lhs, level);
            out.push_str(" |> ");
            format_expr(out, rhs, level);
        }
        ExprKind::Call { callee, args } => {
            format_expr(out, callee, level);
            out.push('(');
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                format_expr(out, arg, level);
            }
            out.push(')');
        }
        ExprKind::NamedArg { name, value } => {
            out.push_str(name);
            out.push_str(" = ");
            format_expr(out, value, level);
        }
        ExprKind::Block { stmts, expr: trailing } => {
            out.push_str("{\n");
            for stmt in stmts {
                format_stmt(out, stmt, level + 1);
                out.push('\n');
            }
            if let Some(t) = trailing {
                indent(out, level + 1);
                format_expr(out, t, level + 1);
                out.push('\n');
            }
            indent(out, level);
            out.push('}');
        }
        ExprKind::Formula { op, response, terms, parts } => {
            format_expr(out, response, level);
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
                        format_expr(out, term, level);
                    }
                }
            } else {
                for (idx, part) in parts.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(" | ");
                    }
                    format_expr(out, part, level);
                }
            }
        }
        ExprKind::SemSpec { equations } => {
            out.push_str("sem_spec {\n");
            for eq in equations {
                indent(out, level + 1);
                format_expr(out, eq, level + 1);
                out.push_str(";\n");
            }
            indent(out, level);
            out.push('}');
        }
        ExprKind::If { cond, then_branch, else_branch } => {
            out.push_str("if ");
            format_expr(out, cond, level);
            out.push(' ');
            format_expr(out, then_branch, level);
            if let Some(el) = else_branch {
                out.push_str(" else ");
                format_expr(out, el, level);
            }
        }
        ExprKind::While { cond, body } => {
            out.push_str("while ");
            format_expr(out, cond, level);
            out.push(' ');
            format_expr(out, body, level);
        }
        ExprKind::For { var, start, end, body } => {
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            format_expr(out, start, level);
            out.push_str("..");
            format_expr(out, end, level);
            out.push(' ');
            format_expr(out, body, level);
        }
        ExprKind::Range { start, end, inclusive } => {
            format_expr(out, start, level);
            if *inclusive {
                out.push_str("..=");
            } else {
                out.push_str("..");
            }
            format_expr(out, end, level);
        }
        ExprKind::VectorLit(items) => {
            out.push('[');
            for (idx, it) in items.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                format_expr(out, it, level);
            }
            out.push(']');
        }
        ExprKind::Comprehension { expr, clauses, condition } => {
            out.push('[');
            format_expr(out, expr, level);
            for (idx, clause) in clauses.iter().enumerate() {
                if idx == 0 {
                    out.push_str(" for ");
                } else {
                    out.push_str(", ");
                }
                out.push_str(&clause.var);
                out.push_str(" in ");
                format_expr(out, &clause.iter, level);
            }
            if let Some(cond) = condition {
                out.push_str(" if ");
                format_expr(out, cond, level);
            }
            out.push(']');
        }
        ExprKind::DataFrameLit(cols) => {
            out.push_str("dataframe {\n");
            for (idx, (name, expr)) in cols.iter().enumerate() {
                indent(out, level + 1);
                out.push_str(name);
                out.push_str(": ");
                format_expr(out, expr, level + 1);
                if idx + 1 < cols.len() {
                    out.push(',');
                }
                out.push('\n');
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
                    format_expr(out, val, level + 1);
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
            format_expr(out, body, level);
        }
        ExprKind::RecordLit(fields) => {
            out.push_str("#[");
            for (idx, (name, expr)) in fields.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
                out.push_str(" = ");
                format_expr(out, expr, level);
            }
            out.push(']');
        }
        ExprKind::StructLit { name, fields } => {
            out.push_str(name);
            out.push_str(" {\n");
            for (idx, (fname, fexpr)) in fields.iter().enumerate() {
                indent(out, level + 1);
                out.push_str(fname);
                out.push_str(": ");
                format_expr(out, fexpr, level + 1);
                if idx + 1 < fields.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, level);
            out.push('}');
        }
        ExprKind::FieldAccess { target, field } => {
            format_expr(out, target, level);
            out.push('.');
            out.push_str(field);
        }
        ExprKind::Index { target, indices } => {
            format_expr(out, target, level);
            out.push('[');
            for (idx, spec) in indices.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                match spec {
                    IndexSpec::Expr(e) => format_expr(out, e, level),
                    IndexSpec::All => out.push_str(".."),
                    IndexSpec::Range { start, end, inclusive } => {
                        if let Some(s) = start {
                            format_expr(out, s, level);
                        }
                        if *inclusive {
                            out.push_str("..=");
                        } else {
                            out.push_str("..");
                        }
                        if let Some(e) = end {
                            format_expr(out, e, level);
                        }
                    }
                }
            }
            out.push(']');
        }
        ExprKind::Match { expr, arms } => {
            out.push_str("match ");
            format_expr(out, expr, level);
            out.push_str(" {\n");
            for arm in arms {
                indent(out, level + 1);
                format_pattern(out, &arm.pattern);
                if let Some(guard) = &arm.guard {
                    out.push_str(" if ");
                    format_expr(out, guard, level + 1);
                }
                out.push_str(" => ");
                format_expr(out, &arm.body, level + 1);
                out.push_str(",\n");
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
            Literal::Int(n) => { let _ = write!(out, "{}", n); }
            Literal::Float(f) => { let _ = write!(out, "{}", f); }
            Literal::String(s) => { let _ = write!(out, "{:?}", s); }
            Literal::Bool(b) => { out.push_str(if *b { "true" } else { "false" }); }
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
        BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => 3,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::DotAdd | BinaryOp::DotSub => 4,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::DotMul | BinaryOp::DotDiv | BinaryOp::MatSolve => 5,
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

    /// Regression test: `fmt` must emit a form of `export_ffi` that the parser
    /// can actually read back (`#[export_ffi]`), not a bare "export ffi"
    /// keyword sequence the grammar has no rule for - which used to silently
    /// destroy the function on the very next format+parse cycle.
    #[test]
    fn test_format_export_ffi_round_trips() {
        let code = "#[export_ffi]\nfn calc(a: int, b: int) -> int { a + b }";
        let formatted = format_source(code).expect("format ok");
        assert!(formatted.contains("#[export_ffi]"));

        let reparsed = crate::parser::parse(&formatted).expect("formatted output must still parse");
        match &reparsed.statements[0].kind {
            StmtKind::Fn { name, export_ffi, .. } => {
                assert_eq!(name, "calc");
                assert!(*export_ffi, "export_ffi flag must survive the round trip");
            }
            other => panic!("expected StmtKind::Fn, got {other:?}"),
        }
    }

    /// Formatting an already-canonical `#[export_ffi]` function must be a
    /// no-op (idempotent) - the specific property that would have caught the
    /// bug above before it shipped.
    #[test]
    fn test_format_export_ffi_is_idempotent() {
        let code = "#[export_ffi]\nfn calc(a: int, b: int) -> int { a + b }";
        let once = format_source(code).expect("format ok");
        let twice = format_source(&once).expect("re-format ok");
        assert_eq!(once, twice);
    }
}
