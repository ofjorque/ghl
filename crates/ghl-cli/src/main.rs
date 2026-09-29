use std::env;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps};

fn print_banner(caps: &RenderCaps) {
    let mut panel = CockpitPanel::new("GHL Cockpit Deck");
    panel.with_badge(if caps.unicode_enabled { "𝑴𝒆𝒐𝒘. ฅ(•- •マ v0.1.0" } else { "v0.1.0" });

    let caps_str = if caps.unicode_enabled && caps.color_enabled {
        "[UTF8, ANSI]"
    } else if caps.unicode_enabled {
        "[UTF8, NO_COLOR]"
    } else {
        "[ASCII_PLAIN]"
    };

    panel.add_line("Gojo & Haru High-Performance Statistical Computing System");
    panel.add_line(format!(
        "JIT: Cranelift 0.135 (x86_64)   Runtime: NEKO v0.1.6   Caps: {}",
        caps.cyan(caps_str)
    ));

    println!("{}", panel.render(caps));
}


fn print_help(caps: &RenderCaps) {
    print_banner(caps);
    println!(r#"Usage: ghl <command> [options]

Commands:
    run <file|options>         Execute a GHL script with fast Cranelift JIT
                               (Options: -e "code" for inline, - for stdin, -q for quiet, -v for verbose)
    repl                       Launch the interactive shell
    check <file.gh|file.ghl>   Validate syntax, types, and Cranelift HIR lowering
    new <name>                 Create a new structured GHL project (RFC 06 §4)
    fetch                      Resolve dependencies and generate reproducible ghl.lock (RFC 06 §4)
    test [dir]                 Run unit and statistical tests
    doc [path] [options]       Generate documentation from /// comments (--html, --md, -o <dir>)
    fmt [options] [path]       Format GHL source files to canonical style (--check)
    build <file.gh|file.ghl>   Compile standalone binary or shared library (--release, --shared, -o)
    lsp                        Start the Language Server Protocol (stdio)
    version                    Display version information

Examples:
    ghl run model.gh
    ghl fmt model.gh
    ghl fmt --check .
    ghl new my_project
    ghl fetch
    ghl test
    ghl build model.gh --release
    ghl build model.gh --shared -o libmodel.so
    ghl repl
"#);
}

mod repl;
mod package;
mod jit_bridge;
mod run;
mod check;
mod build;
mod fmt;

fn run_repl(caps: &RenderCaps) {
    let mut session = repl::ReplSession::new(caps.clone());
    session.start();
}

fn main() {
    const STACK_SIZE: usize = 16 * 1024 * 1024;
    let child = std::thread::Builder::new()
        .name("ghl-main".to_string())
        .stack_size(STACK_SIZE)
        .spawn(real_main);

    match child {
        Ok(handle) => {
            if let Err(e) = handle.join() {
                std::panic::resume_unwind(e);
            }
        }
        Err(_) => real_main(),
    }
}

fn real_main() {
    let caps = RenderCaps::detect();
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help(&caps);
        return;
    }

    match args[1].as_str() {
        "repl" => run_repl(&caps),
        "lsp" => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to initialize tokio runtime for LSP");
            rt.block_on(ghl_lsp::run_server());
        }
        "version" | "-v" | "--version" => {
            println!("ghl version 0.1.0 (built with Cranelift 0.135 JIT for x86_64-unknown-linux-gnu)");
        }
        "new" => {
            if args.len() < 3 {
                let err = Diagnostic::compute_error("C0001", "Missing project name")
                    .with_help("Provide a project name: `ghl new my_project`");
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }
            if let Err(e) = package::cmd_new(&args[2], &caps) {
                eprintln!("{}", e.render_with_caps(&caps));
                std::process::exit(1);
            }
        }
        "fetch" => {
            let manifest_path = if args.len() >= 3 {
                std::path::PathBuf::from(&args[2])
            } else {
                std::path::PathBuf::from("ghl.toml")
            };
            if !manifest_path.exists() {
                let err = Diagnostic::compute_error("C0607", format!("Cannot find manifest `{}`", manifest_path.display()))
                    .with_help("Run `ghl new <name>` to create a new project, or navigate to a project root.");
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }
            if let Err(e) = package::cmd_fetch(&manifest_path, &caps) {
                eprintln!("{}", e.render_with_caps(&caps));
                std::process::exit(1);
            }
        }
        "test" => {
            let root = if args.len() >= 3 {
                std::path::PathBuf::from(&args[2])
            } else {
                std::path::PathBuf::from(".")
            };
            if let Err(_) = package::cmd_test(&root, &caps) {
                std::process::exit(1);
            }
        }
        "doc" => {
            let mut target_path = std::path::PathBuf::from(".");
            let mut out_dir = std::path::PathBuf::from("docs/api");
            let mut html = true;
            let mut md = true;
            let mut custom_title = None;

            let mut idx = 2;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--html" => {
                        html = true;
                        md = false;
                    }
                    "--markdown" | "--md" => {
                        md = true;
                        html = false;
                    }
                    "-o" | "--out" => {
                        if idx + 1 < args.len() {
                            out_dir = std::path::PathBuf::from(&args[idx + 1]);
                            idx += 1;
                        }
                    }
                    "--title" => {
                        if idx + 1 < args.len() {
                            custom_title = Some(args[idx + 1].clone());
                            idx += 1;
                        }
                    }
                    other if !other.starts_with('-') => {
                        target_path = std::path::PathBuf::from(other);
                    }
                    _ => {}
                }
                idx += 1;
            }

            if let Err(e) = package::cmd_doc(
                &target_path,
                &out_dir,
                html,
                md,
                custom_title.as_deref(),
                &caps,
            ) {
                eprintln!("{}", e.render_with_caps(&caps));
                std::process::exit(1);
            }
        }
        "run" => run::cmd_run(&args, &caps),
        "check" => check::cmd_check(&args, &caps),
        "build" => build::cmd_build(&args, &caps),
        "fmt" => fmt::cmd_fmt(&args[2..], &caps),
        "help" | "-h" | "--help" => print_help(&caps),
        other => {
            let err = Diagnostic::compute_error("C0003", format!("Unknown command `{}`", other))
                .with_help("Run `ghl --help` to see available commands.");
            eprintln!("{}", err.render_with_caps(&caps));
            std::process::exit(1);
        }
    }
}


