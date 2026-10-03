use std::io::{self, Write};
use ghl_diagnostics::{CockpitPanel, CockpitTable, Diagnostic, RenderCaps, TableAlignment, TableColumn};
use ghl_runtime::{Interpreter, Value};
use ghl_types::TypeEnv;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Editor, Helper};

/// Canonical spellings of every REPL command, used to suggest a close match
/// when the user mistypes one (see `levenshtein_distance`).
const KNOWN_COMMANDS: &[&str] = &[
    ":quit", ":q", ":exit", ":help", ":h", ":vars", ":var", ":v", ":rm", ":clear-vars", ":doc",
    ":clear", ":c", ":reset", ":r",
];

/// Classic Wagner-Fischer edit distance between two strings, by Unicode scalar value.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());

    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];

    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[m]
}

#[derive(Clone)]
struct GhlPromptHelper {
    caps: RenderCaps,
}

impl Helper for GhlPromptHelper {}
impl Completer for GhlPromptHelper {
    type Candidate = String;
}
impl Hinter for GhlPromptHelper {
    type Hint = String;
}
impl Validator for GhlPromptHelper {}
impl Highlighter for GhlPromptHelper {
    // Safe to emit raw ANSI here: rustyline's cursor-position math (`calculate_position`)
    // always operates on `prompt.raw()`, never on this styled output, and its rendering
    // path (`wrap_at_eol`, both Unix and Windows) treats CSI escape sequences as zero-width.
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> std::borrow::Cow<'b, str> {
        if prompt.contains("ฅ(•⩊ •マ") {
            std::borrow::Cow::Owned(format!("ghl{}> ", self.caps.haru("ฅ(•⩊ •マ")))
        } else if prompt.contains("...") {
            std::borrow::Cow::Owned(format!("   {} ", self.caps.dim("...")))
        } else {
            std::borrow::Cow::Borrowed(prompt)
        }

    }
}

pub struct ReplSession {
    pub interpreter: Interpreter,
    pub type_env: TypeEnv,
    pub caps: RenderCaps,
    pub user_vars: Vec<String>,
    pub user_docs: std::collections::HashMap<String, ghl_syntax::ItemDoc>,
    pub last_assigned_var: Option<String>,
    pub last_expr_ident: Option<String>,
}

impl ReplSession {
    pub fn new(caps: RenderCaps) -> Self {
        Self {
            interpreter: Interpreter::new(),
            type_env: TypeEnv::with_prelude(),
            caps,
            user_vars: Vec::new(),
            user_docs: std::collections::HashMap::new(),
            last_assigned_var: None,
            last_expr_ident: None,
        }
    }

    pub fn start(&mut self) {
        self.print_welcome();

        let history_path = std::env::var("HOME")
            .ok()
            .map(|h| std::path::PathBuf::from(h).join(".ghl_history"));

        let mut rl = match Editor::<GhlPromptHelper, DefaultHistory>::new() {
            Ok(mut editor) => {
                editor.set_helper(Some(GhlPromptHelper { caps: self.caps.clone() }));
                if let Some(ref p) = history_path {
                    let _ = editor.load_history(p);
                }
                Some(editor)
            }
            Err(_) => None,
        };

        let mut multi_line_accum = String::new();

        loop {
            let prompt = if multi_line_accum.is_empty() {
                if self.caps.unicode_enabled { "ghl ฅ(•⩊ •マ> " } else { "ghl> " }
            } else {
                "   ... "
            };

            let line = if let Some(ref mut editor) = rl {
                match editor.readline(prompt) {
                    Ok(l) => l,
                    Err(ReadlineError::Interrupted) => {
                        println!("^C");
                        multi_line_accum.clear();
                        continue;
                    }
                    Err(ReadlineError::Eof) => break,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        break;
                    }
                }
            } else {
                let styled_prompt = if multi_line_accum.is_empty() {
                    if self.caps.unicode_enabled {
                        format!("ghl {}> ", self.caps.haru("ฅ(•⩊ •マ"))
                    } else {
                        "ghl> ".to_string()
                    }
                } else {
                    format!("   {} ", self.caps.dim("..."))
                };
                print!("{}", styled_prompt);

                io::stdout().flush().unwrap_or(());
                let mut buf = String::new();
                match io::stdin().read_line(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => buf,
                    Err(e) => {
                        eprintln!("Error reading input: {e}");
                        break;
                    }
                }
            };

            let trimmed = line.trim();
            if trimmed.is_empty() && multi_line_accum.is_empty() {
                continue;
            }

            // Handle doc lookup shorthand e.g. `?mean` or `?ols`
            if multi_line_accum.is_empty() && trimmed.starts_with('?') {
                if let Some(ref mut editor) = rl {
                    let _ = editor.add_history_entry(trimmed);
                }
                let query = trimmed.trim_start_matches('?').trim();
                if query.is_empty() {
                    self.list_docs();
                } else {
                    self.show_doc(query);
                }
                continue;
            }

            // Handle REPL commands (e.g. :quit, :help, :vars, :rm, :doc)
            if multi_line_accum.is_empty() && trimmed.starts_with(':') {
                if let Some(ref mut editor) = rl {
                    let _ = editor.add_history_entry(trimmed);
                }
                if self.handle_command(trimmed) {
                    break; // :quit was issued
                }
                continue;
            }

            // Accumulate multi-line if input is incomplete (unbalanced brackets, trailing operators, etc.)
            multi_line_accum.push_str(trimmed);
            multi_line_accum.push('\n');

            if Self::is_incomplete_input(&multi_line_accum) {
                continue;
            }

            let code_to_eval = multi_line_accum.trim().to_string();
            multi_line_accum.clear();

            if let Some(ref mut editor) = rl {
                let _ = editor.add_history_entry(code_to_eval.as_str());
            }

            self.eval_input(&code_to_eval);
        }

        if let (Some(mut editor), Some(ref p)) = (rl, history_path) {
            let _ = editor.save_history(p);
        }

        self.print_goodbye();
    }

    fn print_welcome(&self) {
        let mut panel = CockpitPanel::new("GHL Interactive Shell (REPL)");
        panel.with_badge(if self.caps.unicode_enabled { "ฅ(•⩊ •マ READY" } else { "READY" });
        panel.add_line("Gojo & Haru High-Performance Statistical System");
        panel.add_line(format!(
            "Type {} for session commands, {} for docs, or {} to exit.",
            self.caps.bold(":help"),
            self.caps.bold("?<fn>"),
            self.caps.bold(":quit")
        ));
        println!("{}\n", panel.render(&self.caps));
    }

    fn print_goodbye(&self) {
        let cat = if self.caps.unicode_enabled { "ฅ(•⩊ •マ" } else { "[GHL]" };
        println!(
            "\n{} {}\n",
            self.caps.haru(cat),
            self.caps.dim("See you soon!")
        );
    }


    fn handle_command(&mut self, cmd: &str) -> bool {
        match cmd {
            ":quit" | ":q" | ":exit" => true,
            ":help" | ":h" => {
                let mut panel = CockpitPanel::new("REPL Commands");
                panel.add_kv(":help, :h", "Show this help table");
                panel.add_kv(":vars, :var, :v", "List active user-defined variables and types");
                panel.add_kv(":rm <vars>", "Remove one or more variables from session (or :rm *)");
                panel.add_kv(":clear-vars", "Clear all user-defined variables");
                panel.add_kv(":doc <fn>, ?<fn>", "View documentation & formula for a function");
                panel.add_kv(":clear, :c", "Clear the terminal screen");
                panel.add_kv(":reset, :r", "Reset the environment to initial clean state");
                panel.add_kv(":quit, :q", "Exit the interactive shell");
                println!("{}\n", panel.render(&self.caps));
                false
            }
            ":vars" | ":var" | ":v" => {
                if self.user_vars.is_empty() {
                    let mut panel = CockpitPanel::new("Active Variables");
                    panel.add_line(self.caps.dim("(No user variables defined yet. Use `let x = ...`)"));
                    println!("{}\n", panel.render(&self.caps));
                    return false;
                }

                let mut table = CockpitTable::new().with_title("Active Variables");
                table.set_show_row_numbers(false);
                table.set_max_display_rows(self.user_vars.len().max(10));
                table.add_column(TableColumn::new("Name").with_alignment(TableAlignment::Left));
                table.add_column(TableColumn::new("Type").with_alignment(TableAlignment::Left));
                table.add_column(TableColumn::new("Value").with_alignment(TableAlignment::Left));
                for var in &self.user_vars {
                    if let Some(val) = self.interpreter.env.get(var) {
                        let ty_str = self
                            .type_env
                            .lookup(var)
                            .map(|info| info.ty.to_string())
                            .unwrap_or_else(|| val.type_name().to_string());
                        let val_preview = match &val {
                            Value::DataFrame { frame, .. } => {
                                format!("DataFrame ({} rows x {} cols)", frame.height(), frame.width())
                            }
                            Value::Matrix { rows, cols, .. } => {
                                format!("Matrix ({} x {})", rows, cols)
                            }
                            other => {
                                let s = format!("{other}");
                                if s.chars().count() > 30 {
                                    format!("{}…", s.chars().take(28).collect::<String>())
                                } else {
                                    s
                                }
                            }
                        };
                        table.add_row(vec![var.clone(), ty_str, val_preview]);
                    }
                }
                println!("{}\n", table.render(&self.caps));
                false
            }
            ":clear" | ":c" => {
                print!("\x1b[2J\x1b[H");
                io::stdout().flush().unwrap_or(());
                self.print_welcome();
                false
            }
            ":reset" | ":r" => {
                self.interpreter = Interpreter::new();
                self.type_env = TypeEnv::with_prelude();
                self.user_vars.clear();
                self.user_docs.clear();
                self.last_assigned_var = None;
                self.last_expr_ident = None;
                let glyph = if self.caps.unicode_enabled { "ฅ(•⩊ •マ" } else { "[OK]" };
                println!("{} Session environment successfully reset.\n", self.caps.haru(glyph));
                false
            }
            ":clear-vars" => {
                self.clear_all_vars();
                false
            }
            cmd if cmd.starts_with(":rm") => {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() < 2 {
                    let err = Diagnostic::compute_error("C0006", "Usage: `:rm <var1> [var2...]` or `:clear-vars`");
                    eprintln!("{}\n", err.render_with_caps(&self.caps));
                    return false;
                }
                if parts[1] == "*" {
                    self.clear_all_vars();
                    return false;
                }
                let mut removed = Vec::new();
                for var in &parts[1..] {
                    self.interpreter.env.remove(var);
                    self.type_env.remove(var);
                    self.user_vars.retain(|v| v != var);
                    removed.push(*var);
                }
                let glyph = if self.caps.unicode_enabled { "ฅ(•⩊ •マ" } else { "[OK]" };
                println!("{} Removed variable(s): {}\n", self.caps.haru(glyph), removed.join(", "));
                false

            }
            cmd if cmd.starts_with(":doc") => {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() < 2 {
                    self.list_docs();
                } else {
                    self.show_doc(parts[1]);
                }
                false
            }
            other => {
                let typed = other.split_whitespace().next().unwrap_or(other);
                let suggestion = KNOWN_COMMANDS
                    .iter()
                    .map(|&cmd| (cmd, levenshtein_distance(typed, cmd)))
                    .filter(|&(_, dist)| dist <= 2)
                    .min_by_key(|&(_, dist)| dist)
                    .map(|(cmd, _)| cmd);

                let help = match suggestion {
                    Some(cmd) => format!("Did you mean `{cmd}`? Type `:help` to see available commands."),
                    None => "Type `:help` to see available commands.".to_string(),
                };
                let diag = Diagnostic::compute_error(
                    "C0005",
                    format!("Unknown REPL command `{other}`"),
                )
                .with_help(help);
                eprintln!("{}\n", diag.render_with_caps(&self.caps));
                false
            }
        }
    }

    fn clear_all_vars(&mut self) {
        let count = self.user_vars.len();
        for var in &self.user_vars {
            self.interpreter.env.remove(var);
            self.type_env.remove(var);
        }
        self.user_vars.clear();
        self.last_assigned_var = None;
        self.last_expr_ident = None;
        let glyph = if self.caps.unicode_enabled { "ฅ(•⩊ •マ" } else { "[OK]" };
        println!("{} Cleared {count} user variable(s).\n", self.caps.haru(glyph));
    }


    fn show_doc(&self, name: &str) {
        if let Some(doc) = ghl_runtime::lookup_doc(name) {
            println!("{}\n", doc.render(&self.caps));
        } else if let Some(user_doc) = self.user_docs.get(name) {
            println!("{}\n", user_doc.render_cockpit(&self.caps));
        } else if let Some(val) = self.interpreter.env.get(name) {
            match val {
                Value::Closure { params, body, .. } => {
                    let mut panel = CockpitPanel::new(format!("User Function: {}()", name));
                    panel.with_badge("CLOSURE");
                    panel.add_line("User-defined function in current session");
                    panel.add_divider();
                    panel.add_kv("Signature", format!("fn({})", params.join(", ")));
                    panel.add_divider();
                    panel.add_line(self.caps.dim("Definition:"));
                    panel.add_line(format!("  fn {}({}) {{\n      {}\n  }}", name, params.join(", "), body));
                    println!("{}\n", panel.render(&self.caps));
                }
                other => {
                    let ty_str = self
                        .type_env
                        .lookup(name)
                        .map(|info| info.ty.to_string())
                        .unwrap_or_else(|| other.type_name().to_string());
                    let mut panel = CockpitPanel::new(format!("Variable: {}", name));
                    panel.with_badge("VARIABLE");
                    panel.add_kv("Type", ty_str);
                    panel.add_kv("Value", format!("{}", other));
                    println!("{}\n", panel.render(&self.caps));
                }
            }
        } else {
            let diag = Diagnostic::compute_error(
                "C0204",
                format!("No documentation or variable found for `{name}`"),
            )
            .with_help("Type `:doc` to see all documented standard library functions.");
            eprintln!("{}\n", diag.render_with_caps(&self.caps));
        }
    }

    fn list_docs(&self) {
        let mut panel = CockpitPanel::new("Standard Library Functions");
        panel.with_badge(if self.caps.unicode_enabled { "/ᐠ • ˕ •マ ? DOCS" } else { "DOCS" });
        panel.add_line("Use `?<name>` or `:doc <name>` to view signatures and mathematical formulas.");

        panel.add_divider();
        for doc in ghl_runtime::doc::all_docs() {
            panel.add_kv(format!("{}()", doc.name), doc.summary);
        }
        println!("{}\n", panel.render(&self.caps));
    }

    fn is_continuation_input(&self, s: &str) -> bool {
        let trimmed = s.trim();
        trimmed.starts_with('+')
            || (trimmed.starts_with('|') && !trimmed.starts_with("||") && !trimmed.starts_with("|>"))
            || trimmed.starts_with("|>")
    }

    fn eval_input(&mut self, code: &str) {
        let trimmed_code = code.trim();
        if trimmed_code.is_empty() {
            return;
        }

        // Handle continuation lines starting with +, |, |>
        let desugared_storage;
        let effective_code = if self.is_continuation_input(trimmed_code) {
            if let Some(ref var_name) = self.last_assigned_var {
                if self.interpreter.env.get(var_name).is_some() {
                    let body = trimmed_code.trim_end_matches(';').trim();
                    desugared_storage = format!("let {} = {} {};", var_name, var_name, body);
                    desugared_storage.as_str()
                } else {
                    code
                }
            } else if let Some(ref expr_id) = self.last_expr_ident {
                if self.interpreter.env.get(expr_id).is_some() {
                    desugared_storage = format!("{} {}", expr_id, trimmed_code);
                    desugared_storage.as_str()
                } else {
                    code
                }
            } else {
                code
            }
        } else {
            code
        };

        // 1. Parse
        let mut program = match ghl_syntax::parse(effective_code) {
            Ok(prog) => prog,
            Err(errors) => {
                for err_msg in errors {
                    let err = Diagnostic::compute_error("C0100", err_msg);
                    eprintln!("{}", err.render_with_caps(&self.caps));
                }
                return;
            }
        };

        // 1b. Module & Package Resolution Phase (RFC 06)
        if let Err(diag) = crate::package::resolve_package_imports(&mut program, std::path::Path::new(".")) {
            eprintln!("{}", diag.render_with_caps(&self.caps));
            return;
        }

        // If user simply typed a documented standard library function name (e.g. `mean`, `ols`)
        // like in R, show its documentation and mathematical formula directly!
        if program.statements.len() == 1 {
            if let ghl_syntax::ast::StmtKind::Expr(ref expr) = program.statements[0].kind {
                if let ghl_syntax::ast::ExprKind::Ident(ref id) = expr.kind {
                    if let Some(doc) = ghl_runtime::lookup_doc(id) {
                        println!("{}\n", doc.render(&self.caps));
                        return;
                    }
                }
            }
        }

        // 2. Track new variable declarations and doc comments
        let extracted_docs = ghl_syntax::extract_doc_comments(effective_code, &program);
        for d in extracted_docs {
            if d.has_doc {
                self.user_docs.insert(d.name.clone(), d);
            }
        }

        for stmt in &program.statements {
            if let ghl_syntax::ast::StmtKind::Let { name, .. } = &stmt.kind {
                if !self.user_vars.contains(name) {
                    self.user_vars.push(name.clone());
                }
            }
            if let ghl_syntax::ast::StmtKind::Fn { name, .. } = &stmt.kind {
                if !self.user_vars.contains(name) {
                    self.user_vars.push(name.clone());
                }
            }
        }

        // 3. Typecheck
        let mut checker = ghl_types::TypeChecker::new("<repl>".to_string(), effective_code);
        checker.env = self.type_env.clone();
        for stmt in &program.statements {
            checker.check_stmt(stmt);
        }
        if !checker.diagnostics.is_empty() {
            for diag in checker.diagnostics {
                eprintln!("{}", diag.render_with_caps(&self.caps));
            }
            return;
        }
        self.type_env = checker.env;

        // 4. Evaluate
        match self.interpreter.eval_program(&program) {
            Ok(final_val) => {
                // Synchronize variables if `rm` was called in code
                let before_vars = self.user_vars.clone();
                self.user_vars.retain(|v| self.interpreter.env.get(v).is_some());
                for v in before_vars {
                    if !self.user_vars.contains(&v) {
                        self.type_env.remove(&v);
                    }
                }

                // Track newly assigned var or expr ident for REPL continuations
                let mut newly_assigned_var = None;
                let mut newly_expr_id = None;
                let mut is_only_expr = false;

                for stmt in &program.statements {
                    match &stmt.kind {
                        ghl_syntax::ast::StmtKind::Let { name, .. } => {
                            newly_assigned_var = Some(name.clone());
                        }
                        ghl_syntax::ast::StmtKind::Expr(expr) => {
                            is_only_expr = true;
                            if let ghl_syntax::ast::ExprKind::Ident(ref id) = expr.kind {
                                newly_expr_id = Some(id.clone());
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(v) = newly_assigned_var {
                    self.last_assigned_var = Some(v);
                    self.last_expr_ident = None;
                } else if is_only_expr {
                    self.last_assigned_var = None;
                    self.last_expr_ident = newly_expr_id;
                }

                if !matches!(final_val, Value::Unit) {
                    // Check if last statement was an expression
                    let is_expr = program
                        .statements
                        .last()
                        .map(|s| matches!(s.kind, ghl_syntax::ast::StmtKind::Expr(_)))
                        .unwrap_or(false);

                    if is_expr {
                        match &final_val {
                            Value::Plot(p) => {
                                println!("{}\n", final_val.render_styled(&self.caps));
                                if let Some(plots_dir) = std::env::var("POSITRON_PLOTS_DIR")
                                    .ok()
                                    .or_else(|| std::env::var("GHL_PLOTS_DIR").ok())
                                {
                                    let timestamp = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis())
                                        .unwrap_or(0);
                                    let plot_path = std::path::Path::new(&plots_dir).join(format!("ghl_plot_{}_{}.svg", std::process::id(), timestamp));
                                    if let Err(e) = p.save_file(&plot_path.to_string_lossy()) {
                                        eprintln!("{} [Plots Pane] Warning: failed to save SVG plot: {e}", self.caps.gojo("/ᐠ ¬`‸´¬ マ"));
                                    }
                                }
                            }
                            Value::DataFrame { .. } | Value::ModelFit(_) | Value::Matrix { .. } => {
                                println!("{}\n", final_val.render_styled(&self.caps));
                            }
                            _ => {
                                let val_str = final_val.render_styled(&self.caps);
                                println!("{} {}\n", self.caps.cyan("=>"), val_str);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                eprintln!("{}", err.render_with_caps(&self.caps));
            }
        }
    }

    pub fn is_incomplete_input(s: &str) -> bool {
        if Self::has_unbalanced_brackets(s) {
            return true;
        }

        // Clean lines of comments to check trailing operator
        let mut clean_lines = Vec::new();
        for line in s.lines() {
            let line_trimmed = line.trim();
            if let Some(idx) = line_trimmed.find("//") {
                let prefix = &line_trimmed[..idx];
                let quotes = prefix.chars().filter(|&c| c == '"').count();
                if quotes % 2 == 0 {
                    let stripped = prefix.trim();
                    if !stripped.is_empty() {
                        clean_lines.push(stripped);
                    }
                    continue;
                }
            }
            if !line_trimmed.is_empty() {
                clean_lines.push(line_trimmed);
            }
        }

        if let Some(last) = clean_lines.last() {
            let trimmed = last.trim();
            if trimmed.ends_with('+')
                || (trimmed.ends_with('|') && !trimmed.ends_with("||") && !trimmed.ends_with("|>"))
                || trimmed.ends_with("|>")
                || trimmed.ends_with('*')
                || trimmed.ends_with('/')
                || trimmed.ends_with('%')
                || trimmed.ends_with('^')
                || trimmed.ends_with("&&")
                || trimmed.ends_with("||")
                || trimmed.ends_with("==")
                || trimmed.ends_with("!=")
                || trimmed.ends_with("<=")
                || trimmed.ends_with(">=")
                || (trimmed.ends_with('<') && !trimmed.ends_with("<<"))
                || (trimmed.ends_with('>') && !trimmed.ends_with(">>"))
                || trimmed.ends_with('~')
                || trimmed.ends_with(',')
                || trimmed.ends_with('=')
            {
                return true;
            }
        }

        false
    }

    pub fn has_unbalanced_brackets(s: &str) -> bool {
        let mut open_braces = 0i32;
        let mut open_parens = 0i32;
        let mut open_brackets = 0i32;
        let mut in_string = false;

        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' && in_string {
                chars.next();
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            match ch {
                '{' => open_braces += 1,
                '}' => open_braces = (open_braces - 1).max(0),
                '(' => open_parens += 1,
                ')' => open_parens = (open_parens - 1).max(0),
                '[' => open_brackets += 1,
                ']' => open_brackets = (open_brackets - 1).max(0),
                _ => {}
            }
        }

        in_string || open_braces > 0 || open_parens > 0 || open_brackets > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unbalanced_brackets_detection() {
        assert!(ReplSession::has_unbalanced_brackets("fn add(a, b) {"));
        assert!(!ReplSession::has_unbalanced_brackets("fn add(a, b) { a + b }"));
        assert!(ReplSession::has_unbalanced_brackets("let v = [1, 2,"));
        assert!(!ReplSession::has_unbalanced_brackets("let v = [1, 2, 3];"));
    }

    #[test]
    fn test_repl_variable_management() {
        let caps = RenderCaps::detect();
        let mut session = ReplSession::new(caps);

        // Define variables
        session.eval_input("let x = 42;");
        session.eval_input("let y = 100;");
        assert_eq!(session.user_vars, vec!["x".to_string(), "y".to_string()]);
        assert!(session.interpreter.env.get("x").is_some());
        assert!(session.interpreter.env.get("y").is_some());

        // Remove x via REPL command
        session.handle_command(":rm x");
        assert_eq!(session.user_vars, vec!["y".to_string()]);
        assert!(session.interpreter.env.get("x").is_none());
        assert!(session.interpreter.env.get("y").is_some());

        // Remove y via code `rm("y")`
        session.eval_input("rm(\"y\");");
        assert!(session.user_vars.is_empty());
        assert!(session.interpreter.env.get("y").is_none());
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance(":clear", ":clear"), 0);
        assert_eq!(levenshtein_distance(":clera", ":clear"), 2);
        assert_eq!(levenshtein_distance(":rm", ":rm"), 0);
        assert_eq!(levenshtein_distance("", "abc"), 3);
    }

    #[test]
    fn test_unknown_command_suggestion() {
        let caps = RenderCaps::ascii_plain(80);
        let mut session = ReplSession::new(caps);
        assert!(!session.handle_command(":clera"));
        assert!(!session.handle_command(":totallybogus"));
    }

    #[test]
    fn test_vars_table_utf8_safe_truncation() {
        let caps = RenderCaps::ascii_plain(80);
        let mut session = ReplSession::new(caps);
        // A string value with multibyte UTF-8 chars placed right around the
        // truncation boundary used to panic on a raw byte-index slice.
        let long_value = "á".repeat(40);
        session.eval_input(&format!("let x = \"{long_value}\";"));
        assert!(!session.handle_command(":vars"));
    }

    #[test]
    fn test_repl_doc_lookup() {
        let caps = RenderCaps::detect();
        let _session = ReplSession::new(caps);

        assert!(ghl_runtime::lookup_doc("mean").is_some());
        assert!(ghl_runtime::lookup_doc("ols").is_some());
        assert!(ghl_runtime::lookup_doc("fit_logistic").is_some());
        assert!(ghl_runtime::lookup_doc("nonexistent_fn").is_none());

        let doc = ghl_runtime::lookup_doc("mean").unwrap();
        assert_eq!(doc.name, "mean");
        assert!(doc.formula.unwrap().contains("x̄"));
    }

    #[test]
    fn test_incomplete_input_detection() {
        assert!(ReplSession::is_incomplete_input("let x = 10 +"));
        assert!(ReplSession::is_incomplete_input("let p = ggplot(df, aes(x = \"a\")) +"));
        assert!(ReplSession::is_incomplete_input("df |>"));
        assert!(ReplSession::is_incomplete_input("p1 |"));
        assert!(ReplSession::is_incomplete_input("let x = 10 + // comment"));
        assert!(!ReplSession::is_incomplete_input("let x = 10 + 5;"));
        assert!(!ReplSession::is_incomplete_input("let p = ggplot(df, aes(x = \"a\")) + geom_point();"));
    }

    #[test]
    fn test_repl_continuation_lines() {
        let caps = RenderCaps::ascii_plain(80);
        let mut session = ReplSession::new(caps);

        // Incremental variable updates with leading +
        session.eval_input("let x = 10;");
        session.eval_input("+ 5;");
        session.eval_input("+ 2;");
        assert_eq!(session.interpreter.env.get("x"), Some(Value::I64(17)));

        // Multi-line plot creation with incremental layers
        session.eval_input("let cars = dataframe { wt: [2.0, 3.0], mpg: [30.0, 20.0], pop: [100.0, 200.0], brand: [\"Sedan\", \"SUV\"] };");
        session.eval_input("let p1 = ggplot(cars, aes(x = \"wt\", y = \"mpg\", size = \"pop\", shape = \"brand\"))");
        session.eval_input("+ geom_point()");
        session.eval_input("+ labs(title = \"Fuel Economy by Weight\")");
        session.eval_input("+ scale_size(range = [3.0, 9.0])");
        session.eval_input("+ theme(font = \"Segoe UI\", style = \"minimal\");");

        assert!(session.interpreter.env.get("p1").is_some());
        if let Some(Value::Plot(plot)) = session.interpreter.env.get("p1") {
            assert_eq!(plot.labels.title.as_deref(), Some("Fuel Economy by Weight"));
            assert_eq!(plot.layers.len(), 1);
            assert_eq!(plot.size_range, (3.0, 9.0));
            assert_eq!(plot.font_family.as_deref(), Some("Segoe UI"));
        } else {
            panic!("Expected Value::Plot for p1");
        }
    }
}
