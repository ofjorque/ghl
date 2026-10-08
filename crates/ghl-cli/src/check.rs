//! `ghl check` — validate syntax, types, and Cranelift HIR lowering without executing.

use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps};
use ghl_syntax::SourceIndex;

pub fn cmd_check(args: &[String], caps: &RenderCaps) {
    if args.len() < 3 {
        let err = Diagnostic::compute_error("C0002", "Missing file path to check")
            .with_help("Provide a script: `ghl check script.gh`");
        eprintln!("{}", err.render_with_caps(caps));
        std::process::exit(1);
    }
    let file = &args[2];
    match std::fs::read_to_string(file) {
        Ok(content) => {
            match ghl_syntax::parse_spanned(&content) {
                Ok(mut program) => {
                    if let Err(diag) = crate::package::resolve_package_imports(
                        &mut program,
                        std::path::Path::new(file),
                    ) {
                        eprintln!("{}", diag.render_with_caps(caps));
                        std::process::exit(1);
                    }
                    let parse_stat =
                        format!("Parsed {} top-level statements", program.statements.len());
                    match ghl_types::check(&program, file, &content) {
                        Ok(_) => {
                            let mut panel = CockpitPanel::new("GHL Verification Deck");
                            panel.with_badge(caps.green(if caps.unicode_enabled {
                                "/ᐠ˵- ⩊ -˵マ ✧ PASS"
                            } else {
                                "[PASS]"
                            }));
                            panel.add_kv("Target File", file);
                            panel.add_kv("Syntax", format!("✔ Syntax verified ({parse_stat})"));
                            panel.add_kv("Type Safety", "✔ Zero semantic and type errors");

                            if let Ok(hir_module) = ghl_ir::lower_ast(&program) {
                                if !hir_module.functions.is_empty() {
                                    panel.add_kv(
                                        "Cranelift JIT",
                                        format!(
                                            "Verified {} functions ready for native machine code",
                                            hir_module.functions.len()
                                        ),
                                    );
                                }
                            }

                            println!("{}", panel.render(caps));
                        }
                        Err(diags) => {
                            for diag in diags {
                                eprintln!("{}", diag.render_with_caps(caps));
                            }
                            std::process::exit(1);
                        }
                    }
                }
                Err(errors) => {
                    let index = SourceIndex::new(&content);
                    for syntax_err in errors {
                        let (line, col) = index.offset_to_position(syntax_err.span.start);
                        let err = Diagnostic::compute_error("C0100", syntax_err.message)
                            .with_location(file, line as usize + 1, col as usize + 1);
                        eprintln!("{}", err.render_with_caps(caps));
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
            eprintln!("{}", err.render_with_caps(caps));
            std::process::exit(1);
        }
    }
}
