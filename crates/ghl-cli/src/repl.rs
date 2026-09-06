use std::io::{self, Write};
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps};
use ghl_runtime::{Interpreter, Value};
use ghl_types::TypeEnv;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

pub struct ReplSession {
    pub interpreter: Interpreter,
    pub type_env: TypeEnv,
    pub caps: RenderCaps,
    pub user_vars: Vec<String>,
}

impl ReplSession {
    pub fn new(caps: RenderCaps) -> Self {
        Self {
            interpreter: Interpreter::new(),
            type_env: TypeEnv::with_prelude(),
            caps,
            user_vars: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.print_welcome();

        let history_path = std::env::var("HOME")
            .ok()
            .map(|h| std::path::PathBuf::from(h).join(".ghl_history"));

        let mut rl = match DefaultEditor::new() {
            Ok(mut editor) => {
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
                format!("ghl{}> ", self.caps.cyan("(=^･ω･^=)"))
            } else {
                format!("   {} ", self.caps.dim("..."))
            };

            let line = if let Some(ref mut editor) = rl {
                match editor.readline(&prompt) {
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
                print!("{}", prompt);
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

            // Handle REPL commands (e.g. :quit, :help, :vars)
            if multi_line_accum.is_empty() && trimmed.starts_with(':') {
                if let Some(ref mut editor) = rl {
                    let _ = editor.add_history_entry(trimmed);
                }
                if self.handle_command(trimmed) {
                    break; // :quit was issued
                }
                continue;
            }

            // Accumulate multi-line if open brackets exist
            multi_line_accum.push_str(trimmed);
            multi_line_accum.push('\n');

            if Self::has_unbalanced_brackets(&multi_line_accum) {
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
        panel.with_badge("READY");
        panel.add_line("Gojo & Haru High-Performance Statistical System");
        panel.add_line(format!(
            "Type {} for session commands or {} to exit.",
            self.caps.bold(":help"),
            self.caps.bold(":quit")
        ));
        println!("{}\n", panel.render(&self.caps));
    }

    fn print_goodbye(&self) {
        println!(
            "\n{} Haru says goodbye! (U・ᴥ・U) See you soon.\n",
            self.caps.green("✔")
        );
    }

    fn handle_command(&mut self, cmd: &str) -> bool {
        match cmd {
            ":quit" | ":q" | ":exit" => true,
            ":help" | ":h" => {
                let mut panel = CockpitPanel::new("REPL Commands");
                panel.add_kv(":help, :h", "Show this help table");
                panel.add_kv(":vars, :v", "List active user-defined variables and types");
                panel.add_kv(":clear, :c", "Clear the terminal screen");
                panel.add_kv(":reset, :r", "Reset the environment to initial clean state");
                panel.add_kv(":quit, :q", "Exit the interactive shell");
                println!("{}\n", panel.render(&self.caps));
                false
            }
            ":vars" | ":v" => {
                let mut panel = CockpitPanel::new("Active Variables");
                if self.user_vars.is_empty() {
                    panel.add_line(self.caps.dim("(No user variables defined yet. Use `let x = ...`)"));
                } else {
                    for var in &self.user_vars {
                        if let Some(val) = self.interpreter.env.get(var) {
                            let ty_str = self
                                .type_env
                                .lookup(var)
                                .map(|info| info.ty.to_string())
                                .unwrap_or_else(|| val.type_name().to_string());
                            let val_preview = match &val {
                                Value::DataFrame { columns, data } => {
                                    let rows = data.values().next().map(|v| v.len()).unwrap_or(0);
                                    format!("DataFrame ({} rows x {} cols)", rows, columns.len())
                                }
                                Value::Matrix { rows, cols, .. } => {
                                    format!("Matrix ({} x {})", rows, cols)
                                }
                                other => {
                                    let s = format!("{other}");
                                    if s.len() > 30 {
                                        format!("{}…", &s[..28])
                                    } else {
                                        s
                                    }
                                }
                            };
                            panel.add_kv(format!("{var} [{ty_str}]"), val_preview);
                        }
                    }
                }
                println!("{}\n", panel.render(&self.caps));
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
                println!("(=^･ω･^=) Session environment successfully reset.\n");
                false
            }
            other => {
                let diag = Diagnostic::compute_error(
                    "C0005",
                    format!("Unknown REPL command `{other}`"),
                )
                .with_help("Type `:help` to see available commands.");
                eprintln!("{}\n", diag.render_with_caps(&self.caps));
                false
            }
        }
    }

    fn eval_input(&mut self, code: &str) {
        // 1. Parse
        let program = match ghl_syntax::parse(code) {
            Ok(prog) => prog,
            Err(errors) => {
                for err_msg in errors {
                    let err = Diagnostic::compute_error("C0100", err_msg);
                    eprintln!("{}", err.render_with_caps(&self.caps));
                }
                return;
            }
        };

        // 2. Track new variable declarations
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
        let mut checker = ghl_types::TypeChecker::new("<repl>".to_string());
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
                if !matches!(final_val, Value::Unit) {
                    // Check if last statement was an expression
                    let is_expr = program
                        .statements
                        .last()
                        .map(|s| matches!(s.kind, ghl_syntax::ast::StmtKind::Expr(_)))
                        .unwrap_or(false);

                    if is_expr {
                        match &final_val {
                            Value::DataFrame { .. } | Value::ModelFit(_) | Value::Matrix { .. } | Value::Plot(_) => {
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

    fn has_unbalanced_brackets(s: &str) -> bool {
        let mut open_braces = 0i32;
        let mut open_parens = 0i32;
        let mut open_brackets = 0i32;
        let mut in_string = false;

        for ch in s.chars() {
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

        open_braces > 0 || open_parens > 0 || open_brackets > 0
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
}
