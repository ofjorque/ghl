//! `ghl run` — parse, typecheck, JIT-compile with Cranelift, and execute a GHL script.

use std::time::Instant;
use ghl_diagnostics::{Diagnostic, RenderCaps, SemanticCode};
use ghl_syntax::SourceIndex;

pub fn cmd_run(args: &[String], caps: &RenderCaps) {
    if args.len() < 3 {
        let err = Diagnostic::compute_error("C0001", "Missing file or code to run")
            .with_help("Usage: `ghl run <script.gh>`, `ghl run -e 'code'`, or `ghl run -`");
        eprintln!("{}", err.render_with_caps(caps));
        std::process::exit(1);
    }

    let mut quiet = false;
    let mut eval_code: Option<String> = None;
    let mut file_target: Option<String> = None;

    let mut idx = 2;
    while idx < args.len() {
        match args[idx].as_str() {
            "-q" | "--quiet" => quiet = true,
            "-e" | "-c" => {
                if idx + 1 < args.len() {
                    eval_code = Some(args[idx + 1].clone());
                    idx += 1;
                } else {
                    let err = Diagnostic::compute_error("C0001", "Missing code string for -e / -c flag");
                    eprintln!("{}", err.render_with_caps(caps));
                    std::process::exit(1);
                }
            }
            "-" => {
                file_target = Some("-".to_string());
            }
            other if !other.starts_with('-') && file_target.is_none() && eval_code.is_none() => {
                file_target = Some(other.to_string());
            }
            _ => {}
        }
        idx += 1;
    }

    let (content, filename) = if let Some(code) = eval_code {
        (code, "<inline>".to_string())
    } else if let Some(target) = file_target {
        if target == "-" {
            use std::io::Read;
            let mut buffer = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut buffer) {
                let err = Diagnostic::compute_error("C0004", format!("Failed to read stdin: {e}"));
                eprintln!("{}", err.render_with_caps(caps));
                std::process::exit(1);
            }
            (buffer, "<stdin>".to_string())
        } else {
            match std::fs::read_to_string(&target) {
                Ok(c) => (c, target),
                Err(io_err) => {
                    let err = Diagnostic::compute_error(
                        "C0004",
                        format!("Failed to read file `{}`: {}", target, io_err),
                    );
                    eprintln!("{}", err.render_with_caps(caps));
                    std::process::exit(1);
                }
            }
        }
    } else {
        let err = Diagnostic::compute_error("C0001", "No input file or inline code provided")
            .with_help("Usage: `ghl run <script.gh>`, `ghl run -e 'code'`, or `ghl run -`");
        eprintln!("{}", err.render_with_caps(caps));
        std::process::exit(1);
    };

    let step_mark = SemanticCode::PipelineStep.glyph(caps);

    // 1. Parsing phase
    let program = match ghl_syntax::parse_spanned(&content) {
        Ok(prog) => prog,
        Err(errors) => {
            let index = SourceIndex::new(&content);
            for syntax_err in errors {
                let (line, col) = index.offset_to_position(syntax_err.span.start);
                let err = Diagnostic::compute_error("C0100", syntax_err.message)
                    .with_location(&filename, line as usize + 1, col as usize + 1);
                eprintln!("{}", err.render_with_caps(caps));
            }
            std::process::exit(1);
        }
    };

    // 2. Semantic and type checking
    if let Err(diags) = ghl_types::check(&program, &filename) {
        for diag in diags {
            eprintln!("{}", diag.render_with_caps(caps));
        }
        std::process::exit(1);
    }

    // 3. JIT Native Compilation (Cranelift)
    let jit_start = Instant::now();
    let opt_jit: Option<(std::sync::Arc<ghl_codegen::JitEngine>, ghl_ir::HirModule)> = (|| {
        let hir_module = ghl_ir::lower_ast(&program).ok()?;
        if hir_module.functions.is_empty() {
            return None;
        }
        let mut jit = ghl_codegen::JitEngine::new().ok()?;
        jit.compile_module(&hir_module).ok()?;
        Some((std::sync::Arc::new(jit), hir_module))
    })();

    let jit_info = opt_jit.as_ref().map(|(_jit, hir)| {
        let elapsed = jit_start.elapsed().as_secs_f64() * 1000.0;
        format!("({} functions compiled in {:.2}ms)", hir.functions.len(), elapsed)
    });

    // Telemetry indicator for interactive users
    if caps.is_tty && !quiet {
        let jit_label = if let Some(info) = &jit_info {
            format!("{} {}", caps.cyan("CRANELIFT JIT ▶"), caps.dim(info))
        } else {
            format!("{}", caps.cyan("EXECUTE ▶"))
        };

        let header = format!(
            "{} {}  {} {}  [3/3 {}]",
            caps.dim("[1/3 PARSE]"),
            caps.green(step_mark),
            caps.dim("[2/3 TYPECHECK]"),
            caps.green(step_mark),
            jit_label,
        );
        println!("{}\n", header);
    }

    // 4. High-performance execution runtime
    let mut interpreter = ghl_runtime::Interpreter::new();

    // Bridge Cranelift JIT compiled functions into runtime environment
    if let Some((jit_arc, hir_module)) = &opt_jit {
        for (name, hir_fn) in &hir_module.functions {
            if let Some(ptr) = jit_arc.get_fn_ptr(name) {
                if let Some(trampoline) = crate::jit_bridge::create_jit_trampoline(hir_fn, ptr, jit_arc.clone()) {
                    interpreter.env.set(name.clone(), ghl_runtime::Value::JitFn {
                        name: name.clone(),
                        func: trampoline,
                    });
                }
            }
        }
    }

    match interpreter.eval_program(&program) {
        Ok(final_val) => {
            if !matches!(final_val, ghl_runtime::Value::Unit) {
                println!("{}", final_val);
            }
        }
        Err(err) => {
            eprintln!("{}", err.render_with_caps(caps));
            std::process::exit(1);
        }
    }
}
