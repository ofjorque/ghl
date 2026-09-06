use std::env;
use ghl_diagnostics::Diagnostic;

fn print_banner() {
    println!(r#"
       /\_/\      (U・ᴥ・U)
      ( o.o )       GHL (Generalized Hypothesis Language) v0.1.0
       > ^ <        Gojo & Haru High-Performance Statistical System
"#);
}

fn print_help() {
    print_banner();
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

fn run_repl() {
    print_banner();
    println!("Interactive shell ready. Type :quit to exit. (=^･ω･^=)\n");

    let sample_diag = Diagnostic::statistical_warning(
        "SW001",
        "Interactive session initialized with default PRNG seed (Xoshiro256++)"
    );
    println!("{}", sample_diag.render());
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "repl" => run_repl(),
        "version" | "-v" | "--version" => {
            println!("ghl version 0.1.0 (built for x86_64-unknown-linux-gnu)");
        }
        "run" => {
            if args.len() < 3 {
                let err = Diagnostic::compute_error("C0001", "Missing file path to run")
                    .with_help("Provide a script: `ghl run script.gh`");
                eprintln!("{}", err.render());
                std::process::exit(1);
            }
            let file = &args[2];
            println!("(U・ᴥ・U) Haru is preparing to run: {}", file);
            // Runtime invocation hook
        }
        "check" => {
            if args.len() < 3 {
                let err = Diagnostic::compute_error("C0002", "Missing file path to check")
                    .with_help("Provide a script: `ghl check script.gh`");
                eprintln!("{}", err.render());
                std::process::exit(1);
            }
            let file = &args[2];
            println!("(=^･ω･^=) Gojo is checking syntax and statistical validity for: {}", file);
        }
        "help" | "-h" | "--help" => print_help(),
        other => {
            let err = Diagnostic::compute_error("C0003", format!("Unknown command `{}`", other))
                .with_help("Run `ghl --help` to see available commands.");
            eprintln!("{}", err.render());
            std::process::exit(1);
        }
    }
}
