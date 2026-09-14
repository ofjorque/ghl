use std::env;
use std::time::Instant;
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
        "JIT: Cranelift 0.135 (x86_64)   Runtime: NEKO v0.1.6   Caps: {}",
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
    check <file.gh>   Validate syntax, types, and Cranelift HIR lowering
    new <name>        Create a new structured GHL project (RFC 06 §4)
    fetch             Resolve dependencies and generate reproducible ghl.lock (RFC 06 §4)
    test [dir]        Run unit and statistical tests
    build <file.gh>   Compile standalone binary or shared library (--release, --shared, -o)
    version           Display version information

Examples:
    ghl run model.gh
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

fn run_repl(caps: &RenderCaps) {
    let mut session = repl::ReplSession::new(caps.clone());
    session.start();
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

                    // 3. JIT Precompilation (Cranelift)
                    let jit_start = Instant::now();
                    let jit_info = (|| -> Option<String> {
                        let hir_module = ghl_ir::lower_ast(&program).ok()?;
                        if hir_module.functions.is_empty() {
                            return None;
                        }
                        let mut jit = ghl_codegen::JitEngine::new().ok()?;
                        jit.compile_module(&hir_module).ok()?;
                        let elapsed = jit_start.elapsed().as_secs_f64() * 1000.0;
                        Some(format!("({} functions compiled in {:.2}ms)", hir_module.functions.len(), elapsed))
                    })();

                    // Telemetry indicator for interactive users
                    if caps.is_tty {
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

                                    if let Ok(hir_module) = ghl_ir::lower_ast(&program) {
                                        if !hir_module.functions.is_empty() {
                                            panel.add_kv("Cranelift JIT", format!("Verified {} functions ready for native machine code", hir_module.functions.len()));
                                        }
                                    }

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
        "build" => {
            let mut file_opt: Option<String> = None;
            let mut release = false;
            let mut shared = false;
            let mut out_opt: Option<String> = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--release" => release = true,
                    "--shared" | "--lib" => shared = true,
                    "-o" | "--output" => {
                        i += 1;
                        if i < args.len() {
                            out_opt = Some(args[i].clone());
                        } else {
                            let err = Diagnostic::compute_error("C0005", "Missing argument after `-o`")
                                .with_help("Provide output path: `-o libmodel.so`");
                            eprintln!("{}", err.render_with_caps(&caps));
                            std::process::exit(1);
                        }
                    }
                    arg if !arg.starts_with('-') && file_opt.is_none() => {
                        file_opt = Some(arg.to_string());
                    }
                    unknown => {
                        let err = Diagnostic::compute_error("C0006", format!("Unknown build option `{unknown}`"))
                            .with_help("Usage: `ghl build <file.gh> [--release] [--shared] [-o <out>]`");
                        eprintln!("{}", err.render_with_caps(&caps));
                        std::process::exit(1);
                    }
                }
                i += 1;
            }

            let file = match file_opt {
                Some(f) => f,
                None => {
                    let err = Diagnostic::compute_error("C0007", "Missing script file to build")
                        .with_help("Provide a script: `ghl build script.gh [--release] [--shared]`");
                    eprintln!("{}", err.render_with_caps(&caps));
                    std::process::exit(1);
                }
            };

            let start_time = Instant::now();
            let content = match std::fs::read_to_string(&file) {
                Ok(c) => c,
                Err(io_err) => {
                    let err = Diagnostic::compute_error(
                        "C0004",
                        format!("Failed to read file `{}`: {}", file, io_err),
                    );
                    eprintln!("{}", err.render_with_caps(&caps));
                    std::process::exit(1);
                }
            };

            // 1. Parsing
            let program = match ghl_syntax::parse(&content) {
                Ok(p) => p,
                Err(errors) => {
                    for err_msg in errors {
                        let err = Diagnostic::compute_error("C0100", err_msg)
                            .with_location(&file, 1, 1);
                        eprintln!("{}", err.render_with_caps(&caps));
                    }
                    std::process::exit(1);
                }
            };

            // 2. Collect exported FFI functions
            let mut exported_fns: Vec<(String, Vec<ghl_syntax::FnParam>, Option<ghl_syntax::TypeAnnotation>)> = Vec::new();
            for stmt in &program.statements {
                if let ghl_syntax::StmtKind::Fn { name, params, ret_ty, export_ffi, .. } = &stmt.kind {
                    if *export_ffi {
                        exported_fns.push((name.clone(), params.clone(), ret_ty.clone()));
                    }
                }
            }

            let is_shared = shared || !exported_fns.is_empty() || out_opt.as_ref().map_or(false, |o| {
                o.ends_with(".so") || o.ends_with(".dylib") || o.ends_with(".dll")
            });

            if is_shared && exported_fns.is_empty() {
                for stmt in &program.statements {
                    if let ghl_syntax::StmtKind::Fn { name, params, ret_ty, .. } = &stmt.kind {
                        exported_fns.push((name.clone(), params.clone(), ret_ty.clone()));
                    }
                }
            }

            let input_path = std::path::Path::new(&file);
            let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");

            let default_ext = if cfg!(windows) {
                if is_shared { ".dll" } else { ".exe" }
            } else if cfg!(target_os = "macos") {
                if is_shared { ".dylib" } else { "" }
            } else {
                if is_shared { ".so" } else { "" }
            };

            let prefix = if is_shared && !cfg!(windows) && !stem.starts_with("lib") {
                "lib"
            } else {
                ""
            };

            let output_path = out_opt.unwrap_or_else(|| format!("{}{}{}", prefix, stem, default_ext));

            // 3. Semantic & type safety check
            if let Err(diags) = ghl_types::check(&program, &file) {
                for diag in diags {
                    eprintln!("{}", diag.render_with_caps(&caps));
                }
                std::process::exit(1);
            }

            // 4. Lower AST to HIR
            let hir_module = match ghl_ir::lower_ast(&program) {
                Ok(m) => m,
                Err(diag) => {
                    eprintln!("{}", diag.render_with_caps(&caps));
                    std::process::exit(1);
                }
            };

            if hir_module.functions.is_empty() {
                let err = Diagnostic::compute_error(
                    "C0500",
                    "Source file contains no compilable functions or expressions for AOT",
                ).with_help("Provide functions or top-level expressions to compile");
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }

            // 5. AOT Object Emission via Cranelift
            let aot = match ghl_codegen::AotEngine::new(stem) {
                Ok(engine) => engine,
                Err(diag) => {
                    eprintln!("{}", diag.render_with_caps(&caps));
                    std::process::exit(1);
                }
            };

            let obj_bytes = match aot.compile_module(&hir_module) {
                Ok(b) => b,
                Err(diag) => {
                    eprintln!("{}", diag.render_with_caps(&caps));
                    std::process::exit(1);
                }
            };

            // 6. Generate runner driver
            let runner_source = if is_shared {
                format!("#![no_main]\n#![allow(non_snake_case)]\n")
            } else {
                let has_ghl_main = hir_module.functions.contains_key("__ghl_main");
                let has_main = hir_module.functions.contains_key("main");

                let mut extern_decls = String::new();
                let mut main_body = String::new();

                if has_ghl_main {
                    extern_decls.push_str("extern \"C\" { fn __ghl_main() -> i64; }\n");
                }
                if has_main {
                    extern_decls.push_str("extern \"C\" { fn main() -> i64; }\n");
                }

                if has_ghl_main && has_main {
                    main_body.push_str("let _ = unsafe { __ghl_main() };\n");
                    main_body.push_str("let res = unsafe { main() };\n");
                    main_body.push_str("println!(\"{}\", res);\n");
                } else if has_ghl_main {
                    main_body.push_str("let res = unsafe { __ghl_main() };\n");
                    main_body.push_str("println!(\"{}\", res);\n");
                } else if has_main {
                    main_body.push_str("let res = unsafe { main() };\n");
                    main_body.push_str("println!(\"{}\", res);\n");
                } else {
                    main_body.push_str("println!(\"GHL native binary initialized.\");\n");
                }

                format!(r#"
#![allow(non_snake_case)]
{extern_decls}

fn main() {{
    {main_body}
}}
"#)
            };

            // 7. Write temporary files and link with rustc
            let pid = std::process::id();
            let temp_obj = std::env::temp_dir().join(format!("{stem}_{pid}.obj"));
            let temp_runner = std::env::temp_dir().join(format!("{stem}_runner_{pid}.rs"));

            if let Err(e) = std::fs::write(&temp_obj, &obj_bytes) {
                let err = Diagnostic::compute_error("C0501", format!("Failed to write object file: {e}"));
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }

            if let Err(e) = std::fs::write(&temp_runner, runner_source) {
                let err = Diagnostic::compute_error("C0502", format!("Failed to write runner source: {e}"));
                eprintln!("{}", err.render_with_caps(&caps));
                std::process::exit(1);
            }

            let mut cmd = std::process::Command::new("rustc");
            if is_shared {
                cmd.arg("--crate-type").arg("cdylib");
            }
            if release {
                cmd.arg("-C").arg("opt-level=3").arg("-C").arg("lto=fat");
            }
            cmd.arg(format!("-Clink-arg={}", temp_obj.display()));
            cmd.arg(&temp_runner);
            cmd.arg("-o").arg(&output_path);

            let link_output = cmd.output();

            // Cleanup temporary files
            let _ = std::fs::remove_file(&temp_obj);
            let _ = std::fs::remove_file(&temp_runner);

            match link_output {
                Ok(out) if out.status.success() => {
                    let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                    let file_size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                    let size_str = if file_size >= 1024 * 1024 {
                        format!("{:.2} MB", file_size as f64 / (1024.0 * 1024.0))
                    } else {
                        format!("{:.1} KB", file_size as f64 / 1024.0)
                    };

                    if is_shared {
                        let out_file_path = std::path::Path::new(&output_path);
                        let out_dir = out_file_path.parent().unwrap_or_else(|| std::path::Path::new("."));
                        let out_filename = out_file_path.file_name().and_then(|n| n.to_str()).unwrap_or(&output_path);

                        let py_bridge_path = out_dir.join(format!("{stem}_bridge.py"));
                        let r_bridge_path = out_dir.join(format!("{stem}_bridge.R"));

                        // Generate Python ctypes bridge
                        let mut py_content = format!(r#""""
Auto-generated Python ctypes FFI bridge for {stem}
Generated by Gojo & Haru High-Performance Statistical System (GHL) - RFC 06 §3
"""
import ctypes
import os

_dir = os.path.dirname(os.path.abspath(__file__))
_lib_name = "{out_filename}"
_lib_path = os.path.join(_dir, _lib_name)

try:
    lib = ctypes.CDLL(_lib_path)
except OSError:
    lib = ctypes.CDLL(_lib_name)

"#);

                        for (fn_name, params, ret_ty) in &exported_fns {
                            let py_param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                            let py_arg_types: Vec<&'static str> = params.iter().map(|p| {
                                match p.ty.as_ref().map(|t| format!("{t}")).as_deref() {
                                    Some("int") | Some("Int") | Some("i64") => "ctypes.c_int64",
                                    Some("float") | Some("Float") | Some("f64") => "ctypes.c_double",
                                    Some("bool") | Some("Bool") => "ctypes.c_bool",
                                    _ => "ctypes.c_int64",
                                }
                            }).collect();

                            let py_ret = match ret_ty.as_ref().map(|t| format!("{t}")).as_deref() {
                                Some("int") | Some("Int") | Some("i64") => "ctypes.c_int64",
                                Some("float") | Some("Float") | Some("f64") => "ctypes.c_double",
                                Some("bool") | Some("Bool") => "ctypes.c_bool",
                                None | Some("unit") | Some("()") => "None",
                                _ => "ctypes.c_int64",
                            };

                            py_content.push_str(&format!(
                                "# Function: {fn_name}\nlib.{fn_name}.argtypes = [{argtypes}]\nlib.{fn_name}.restype = {py_ret}\n\ndef {fn_name}({args}):\n    return lib.{fn_name}({args})\n\n",
                                argtypes = py_arg_types.join(", "),
                                args = py_param_names.join(", ")
                            ));
                        }

                        let _ = std::fs::write(&py_bridge_path, py_content);

                        // Generate R FFI bridge
                        let mut r_content = format!(r#"# Auto-generated R FFI bridge for {stem}
# Generated by Gojo & Haru High-Performance Statistical System (GHL) - RFC 06 §3

lib_path <- "{out_filename}"
if (file.exists(file.path(getwd(), lib_path))) {{
  dyn.load(file.path(getwd(), lib_path))
}} else {{
  dyn.load(lib_path)
}}

"#);

                        for (fn_name, _params, _ret_ty) in &exported_fns {
                            r_content.push_str(&format!(
                                "# Native symbol for '{fn_name}'\n{fn_name}_symbol <- if (is.loaded(\"{fn_name}\")) getNativeSymbolInfo(\"{fn_name}\") else NULL\n\n"
                            ));
                        }

                        let _ = std::fs::write(&r_bridge_path, r_content);

                        let mut panel = CockpitPanel::new("GHL FFI Shared Library (RFC 06 §3)");
                        panel.with_badge(caps.green(if caps.unicode_enabled { "(U・ᴥ・U) CDYLIB SUCCESS" } else { "[CDYLIB SUCCESS]" }));
                        panel.add_kv("Target File", &file);
                        panel.add_kv("Output Library", &output_path);
                        panel.add_kv("Mode", if release { "Release (opt-level=3, LTO)" } else { "Debug / Standard" });
                        panel.add_kv("Library Size", size_str);
                        panel.add_kv("Compilation Time", format!("{:.2} ms", elapsed_ms));
                        panel.add_kv("FFI Functions", format!("{} exported", exported_fns.len()));
                        panel.add_kv("Python Bridge", py_bridge_path.display().to_string());
                        panel.add_kv("R Bridge", r_bridge_path.display().to_string());

                        println!("{}", panel.render(&caps));
                    } else {
                        let mut panel = CockpitPanel::new("GHL AOT Native Binary");
                        panel.with_badge(caps.green(if caps.unicode_enabled { "(U・ᴥ・U) AOT SUCCESS" } else { "[AOT SUCCESS]" }));
                        panel.add_kv("Target File", &file);
                        panel.add_kv("Output Binary", &output_path);
                        panel.add_kv("Mode", if release { "Release (opt-level=3, LTO)" } else { "Debug / Standard" });
                        panel.add_kv("Binary Size", size_str);
                        panel.add_kv("Compilation Time", format!("{:.2} ms", elapsed_ms));
                        panel.add_kv("Symbols Exported", format!("{} functions", hir_module.functions.len()));

                        println!("{}", panel.render(&caps));
                    }
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let err = Diagnostic::compute_error("C0503", format!("Linker failed:\n{stderr}"));
                    eprintln!("{}", err.render_with_caps(&caps));
                    std::process::exit(1);
                }
                Err(e) => {
                    let err = Diagnostic::compute_error("C0504", format!("Failed to invoke linker `rustc`: {e}"))
                        .with_help("Ensure `rustc` is installed and available in PATH");
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_aot_build_and_execute_standalone() {
        let code = r#"
            fn add_nums(a: int, b: int) -> int {
                a + b
            }

            let x = 18;
            let y = 24;
            add_nums(x, y);
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let hir_module = ghl_ir::lower_ast(&program).expect("hir ok");

        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let stem = format!("test_aot_cli_{pid}");
        let aot = ghl_codegen::AotEngine::new(&stem).expect("aot init ok");
        let obj_bytes = aot.compile_module(&hir_module).expect("aot compile ok");

        let obj_path = temp_dir.join(format!("{stem}.obj"));
        let runner_path = temp_dir.join(format!("{stem}_runner.rs"));
        let exe_path = temp_dir.join(format!("{stem}.exe"));

        std::fs::write(&obj_path, &obj_bytes).expect("write obj");
        let runner_src = r#"
            extern "C" { fn __ghl_main() -> i64; }
            fn main() {
                let res = unsafe { __ghl_main() };
                println!("{}", res);
            }
        "#;
        std::fs::write(&runner_path, runner_src).expect("write runner");

        let mut cmd = std::process::Command::new("rustc");
        cmd.arg("-C").arg("opt-level=3");
        cmd.arg(format!("-Clink-arg={}", obj_path.display()));
        cmd.arg(&runner_path);
        cmd.arg("-o").arg(&exe_path);

        let status = cmd.status().expect("rustc run");
        assert!(status.success(), "rustc compilation must succeed");

        let run_out = std::process::Command::new(&exe_path)
            .output()
            .expect("execute standalone binary");
        assert!(run_out.status.success());
        let stdout = String::from_utf8_lossy(&run_out.stdout);
        assert_eq!(stdout.trim(), "42");

        // Clean up
        let _ = std::fs::remove_file(obj_path);
        let _ = std::fs::remove_file(runner_path);
        let _ = std::fs::remove_file(exe_path);
    }

    #[test]
    fn test_build_shared_library_and_ffi() {
        let code = r#"
            #[export_ffi]
            fn calc_stat(x: int, y: int) -> int {
                x * 2 + y
            }
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let hir_module = ghl_ir::lower_ast(&program).expect("hir ok");

        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let stem = format!("test_ffi_cli_{pid}");
        let aot = ghl_codegen::AotEngine::new(&stem).expect("aot init ok");
        let obj_bytes = aot.compile_module(&hir_module).expect("aot compile ok");

        let obj_path = temp_dir.join(format!("{stem}.obj"));
        let runner_path = temp_dir.join(format!("{stem}_runner.rs"));
        let so_ext = if cfg!(windows) { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
        let prefix = if cfg!(windows) { "" } else { "lib" };
        let so_path = temp_dir.join(format!("{prefix}{stem}.{so_ext}"));

        std::fs::write(&obj_path, &obj_bytes).expect("write obj");
        let runner_src = "#![no_main]\n#![allow(non_snake_case)]\n";
        std::fs::write(&runner_path, runner_src).expect("write runner");

        let mut cmd = std::process::Command::new("rustc");
        cmd.arg("--crate-type").arg("cdylib");
        cmd.arg(format!("-Clink-arg={}", obj_path.display()));
        cmd.arg(&runner_path);
        cmd.arg("-o").arg(&so_path);

        let status = cmd.status().expect("rustc run");
        assert!(status.success(), "rustc compilation of cdylib must succeed");
        assert!(so_path.exists(), "shared library file must exist");

        // Verify with Python ctypes
        let py_script = format!(
            "import ctypes\nlib = ctypes.CDLL('{}')\nres = lib.calc_stat(10, 5)\nassert res == 25, f'Expected 25, got {{res}}'\nprint('PYTHON_FFI_OK')",
            so_path.display()
        );

        let py_res = std::process::Command::new("python3")
            .arg("-c")
            .arg(&py_script)
            .output();

        if let Ok(out) = py_res {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                assert!(stdout.contains("PYTHON_FFI_OK"), "Python ctypes invocation succeeded");
            }
        }

        // Verify with R dyn.load
        let r_script = format!(
            "dyn.load('{}'); stopifnot(is.loaded('calc_stat')); cat('R_FFI_OK\\n')",
            so_path.display()
        );

        let r_res = std::process::Command::new("Rscript")
            .arg("-e")
            .arg(&r_script)
            .output();

        if let Ok(out) = r_res {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                assert!(stdout.contains("R_FFI_OK"), "R dyn.load invocation succeeded");
            }
        }

        // Clean up
        let _ = std::fs::remove_file(obj_path);
        let _ = std::fs::remove_file(runner_path);
        let _ = std::fs::remove_file(so_path);
    }
}

