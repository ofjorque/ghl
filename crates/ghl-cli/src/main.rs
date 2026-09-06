use std::env;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps, SemanticCode};

fn print_banner(caps: &RenderCaps) {
    let mut panel = CockpitPanel::new("GHL Cockpit Deck");
    panel.with_badge("v0.1.0");

    let caps_str = if caps.unicode_enabled && caps.color_enabled {
        "[UTF8, ANSI]"
    } else if caps.unicode_enabled {
        "[UTF8, NO_COLOR]"
    } else {
        "[ASCII_PLAIN]"
    };

    panel.add_line("Gojo & Haru High-Performance Statistical Computing System");
    panel.add_line(format!(
        "JIT: Cranelift (x86_64)   Runtime: NEKO v0.1.6   Caps: {}",
        caps.cyan(caps_str)
    ));

    println!("{}", panel.render(caps));
}

fn print_help(caps: &RenderCaps) {
    print_banner(caps);
    println!(r#"Usage: ghl <command> [options]

Commands:
    run <file.gh>     Execute a GHL script with fast Cranelift JIT
    repl              Launch the interactive shell
    check <file.gh>   Validate syntax and statistical types
    test              Run unit and statistical tests
    build --release   Compile standalone native binary via AOT
    version           Display version information

Examples:
    ghl run model.gh
    ghl repl
"#);
}

fn run_repl(caps: &RenderCaps) {
    print_banner(caps);
    println!("Interactive shell ready. Type :quit to exit. (=^･ω･^=)\n");

    let sample_diag = Diagnostic::statistical_warning(
        "SW001",
        "Interactive session initialized with default PRNG seed (Xoshiro256++)",
    );
    println!("{}", sample_diag.render_with_caps(caps));
}

fn main() {
    let caps = RenderCaps::detect();
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help(&caps);
        return;
    }

    match args[1].as_str() {
        "repl" => run_repl(&caps),
        "version" | "-v" | "--version" => {
            println!("ghl version 0.1.0 (built for x86_64-unknown-linux-gnu)");
        }
        "run" => {
            if args.len() < 3 {
                let err = Diagnostic::compute_error("C0001", "Missing file path to run")
                    .with_help("Provide a script: `ghl run script.gh`");
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }
            let file = &args[2];
            match std::fs::read_to_string(file) {
                Ok(content) => {
                    let step_mark = SemanticCode::PipelineStep.glyph(&caps);

                    // 1. Parsing phase
                    let program = match ghl_syntax::parse(&content) {
                        Ok(prog) => prog,
                        Err(errors) => {
                            for err_msg in errors {
                                let err = Diagnostic::compute_error("C0100", err_msg)
                                    .with_location(file, 1, 1);
                                eprintln!("{}", err.render_with_caps(&caps));
                            }
                            std::process::exit(1);
                        }
                    };

                    // 2. Semantic and type checking
                    if let Err(diags) = ghl_types::check(&program, file) {
                        for diag in diags {
                            eprintln!("{}", diag.render_with_caps(&caps));
                        }
                        std::process::exit(1);
                    }

                    // Telemetry indicator for interactive users
                    if caps.is_tty {
                        let header = format!(
                            "{} {}  {} {}  {} {}",
                            caps.dim("[1/3 PARSE]"),
                            caps.green(step_mark),
                            caps.dim("[2/3 TYPECHECK]"),
                            caps.green(step_mark),
                            caps.dim("[3/3 EXECUTE]"),
                            caps.cyan("▶")
                        );
                        println!("{}\n", header);
                    }

                    // 3. High-performance execution runtime
                    let mut interpreter = ghl_runtime::Interpreter::new();
                    match interpreter.eval_program(&program) {
                        Ok(final_val) => {
                            if !matches!(final_val, ghl_runtime::Value::Unit) {
                                println!("{}", final_val);
                            }
                        }
                        Err(err) => {
                            eprintln!("{}", err.render_with_caps(&caps));
                            std::process::exit(1);
                        }
                    }
                }
                Err(io_err) => {
                    let err = Diagnostic::compute_error(
                        "C0004",
                        format!("Failed to read file `{}`: {}", file, io_err),
                    );
                    eprintln!("{}", err.render_with_caps(&caps));
                    std::process::exit(1);
                }
            }
        }
        "check" => {
            if args.len() < 3 {
                let err = Diagnostic::compute_error("C0002", "Missing file path to check")
                    .with_help("Provide a script: `ghl check script.gh`");
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }
            let file = &args[2];
            match std::fs::read_to_string(file) {
                Ok(content) => {
                    match ghl_syntax::parse(&content) {
                        Ok(program) => {
                            let parse_stat = format!("Parsed {} top-level statements", program.statements.len());
                            match ghl_types::check(&program, file) {
                                Ok(_) => {
                                    let mut panel = CockpitPanel::new("GHL Verification Deck");
                                    panel.with_badge(caps.green(if caps.unicode_enabled { "(U・ᴥ・U) PASS" } else { "[PASS]" }));
                                    panel.add_kv("Target File", file);
                                    panel.add_kv("Syntax", format!("(=^･ω･^=) Gojo verified syntax ({parse_stat})"));
                                    panel.add_kv("Type Safety", "(U・ᴥ・U) Haru verified zero semantic/type errors");
                                    println!("{}", panel.render(&caps));
                                }
                                Err(diags) => {
                                    for diag in diags {
                                        eprintln!("{}", diag.render_with_caps(&caps));
                                    }
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(errors) => {
                            for err_msg in errors {
                                let err = Diagnostic::compute_error("C0100", err_msg)
                                    .with_location(file, 1, 1);
                                eprintln!("{}", err.render_with_caps(&caps));
                            }
                            std::process::exit(1);
                        }
                    }
                }
                Err(io_err) => {
                    let err = Diagnostic::compute_error(
                        "C0004",
                        format!("Failed to read file `{}`: {}", file, io_err),
                    );
                    eprintln!("{}", err.render_with_caps(&caps));
                    std::process::exit(1);
                }
            }
        }
        "help" | "-h" | "--help" => print_help(&caps),
        other => {
            let err = Diagnostic::compute_error("C0003", format!("Unknown command `{}`", other))
                .with_help("Run `ghl --help` to see available commands.");
            eprintln!("{}", err.render_with_caps(&caps));
            std::process::exit(1);
        }
    }
}
