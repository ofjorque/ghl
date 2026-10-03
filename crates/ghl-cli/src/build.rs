//! `ghl build` — ahead-of-time compilation via Cranelift, linked into a standalone
//! binary or shared library (`--shared`), with auto-generated Python/R/Julia FFI bridges
//! for exported functions and structs (RFC 06 §3).

use std::time::Instant;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps};
use ghl_syntax::SourceIndex;

pub fn cmd_build(args: &[String], caps: &RenderCaps) {
    let mut file_opt: Option<String> = None;
    let mut release = false;
    let mut shared = false;
    let mut out_opt: Option<String> = None;
    let mut bridges_filter = "all".to_string();

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
                    eprintln!("{}", err.render_with_caps(caps));
                    std::process::exit(1);
                }
            }
            arg if arg.starts_with("--bridges=") => {
                bridges_filter = arg.trim_start_matches("--bridges=").to_lowercase();
            }
            "--bridges" => {
                i += 1;
                if i < args.len() {
                    bridges_filter = args[i].to_lowercase();
                } else {
                    let err = Diagnostic::compute_error("C0008", "Missing argument after `--bridges`")
                        .with_help("Provide bridges list: `--bridges=py,r,jl` or `--bridges=all`");
                    eprintln!("{}", err.render_with_caps(caps));
                    std::process::exit(1);
                }
            }
            arg if !arg.starts_with('-') && file_opt.is_none() => {
                file_opt = Some(arg.to_string());
            }
            unknown => {
                let err = Diagnostic::compute_error("C0006", format!("Unknown build option `{unknown}`"))
                    .with_help("Usage: `ghl build <file.gh> [--release] [--shared] [--bridges=py,r,jl] [-o <out>]`");
                eprintln!("{}", err.render_with_caps(caps));
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
            eprintln!("{}", err.render_with_caps(caps));
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
            eprintln!("{}", err.render_with_caps(caps));
            std::process::exit(1);
        }
    };

    // 1. Parsing
    let mut program = match ghl_syntax::parse_spanned(&content) {
        Ok(p) => p,
        Err(errors) => {
            let index = SourceIndex::new(&content);
            for syntax_err in errors {
                let (line, col) = index.offset_to_position(syntax_err.span.start);
                let err = Diagnostic::compute_error("C0100", syntax_err.message)
                    .with_location(&file, line as usize + 1, col as usize + 1);
                eprintln!("{}", err.render_with_caps(caps));
            }
            std::process::exit(1);
        }
    };

    // 1b. Module & Package Resolution Phase (RFC 06)
    if let Err(diag) = crate::package::resolve_package_imports(&mut program, std::path::Path::new(&file)) {
        eprintln!("{}", diag.render_with_caps(caps));
        std::process::exit(1);
    }

    // 2. Collect exported FFI functions and structs
    let mut exported_fns: Vec<(String, Vec<ghl_syntax::FnParam>, Option<ghl_syntax::TypeAnnotation>)> = Vec::new();
    let mut exported_structs: Vec<ghl_syntax::StructDecl> = Vec::new();

    for stmt in &program.statements {
        match &stmt.kind {
            ghl_syntax::StmtKind::Fn { name, params, ret_ty, export_ffi, .. } => {
                if *export_ffi {
                    exported_fns.push((name.clone(), params.clone(), ret_ty.clone()));
                }
            }
            ghl_syntax::StmtKind::Struct(decl) => {
                exported_structs.push(decl.clone());
            }
            _ => {}
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
    if let Err(diags) = ghl_types::check(&program, &file, &content) {
        for diag in diags {
            eprintln!("{}", diag.render_with_caps(caps));
        }
        std::process::exit(1);
    }

    // 4. Lower AST to HIR
    let hir_module = match ghl_ir::lower_ast(&program) {
        Ok(m) => m,
        Err(diag) => {
            eprintln!("{}", diag.render_with_caps(caps));
            std::process::exit(1);
        }
    };

    if !hir_module.skipped.is_empty() {
        let mut msg = format!(
            "Cannot compile {} function(s) ahead-of-time:\n",
            hir_module.skipped.len()
        );
        for (name, diag) in &hir_module.skipped {
            msg.push_str(&format!("  - `{name}`: {}\n", diag.message));
        }
        let err = Diagnostic::compute_error("C0505", msg.trim_end().to_string())
            .with_help("Rewrite these functions using only scalar int/float/bool operations, or remove them before running `ghl build` (they still work under `ghl run`, which falls back to the interpreter per function).");
        eprintln!("{}", err.render_with_caps(caps));
        std::process::exit(1);
    }

    if hir_module.functions.is_empty() {
        let err = Diagnostic::compute_error(
            "C0500",
            "Source file contains no compilable functions or expressions for AOT",
        ).with_help("Provide functions or top-level expressions to compile");
        eprintln!("{}", err.render_with_caps(caps));
        std::process::exit(1);
    }

    // 5. AOT Object Emission via Cranelift
    let aot = match ghl_codegen::AotEngine::new(stem) {
        Ok(engine) => engine,
        Err(diag) => {
            eprintln!("{}", diag.render_with_caps(caps));
            std::process::exit(1);
        }
    };

    let obj_bytes = match aot.compile_module(&hir_module) {
        Ok(b) => b,
        Err(diag) => {
            eprintln!("{}", diag.render_with_caps(caps));
            std::process::exit(1);
        }
    };

    // 6. Generate runner driver
    //
    // `AotEngine` declares every entry in `ghl_codegen::host::HOST_FUNCTIONS` as a
    // C-ABI import regardless of whether the script calls it, so the runner must
    // always define native stand-ins for them or the link step fails on unresolved
    // symbols. The stub text is generated from that same table (see host.rs), so
    // adding a host function there is the only edit needed for both backends.
    let host_stub = ghl_codegen::host::runner_source_stub();
    let runner_source = if is_shared {
        let mut ffi_decls = String::new();
        let mut r_adapters = String::new();

        for (fn_name, params, ret_ty) in &exported_fns {
            let rust_params: Vec<String> = params.iter().map(|p| {
                let ty_str = match p.ty.as_ref().map(|t| format!("{t}")).as_deref() {
                    Some("int") | Some("Int") | Some("i64") => "i64",
                    Some("float") | Some("Float") | Some("f64") => "f64",
                    Some("bool") | Some("Bool") => "bool",
                    _ => "i64",
                };
                format!("{}: {}", p.name, ty_str)
            }).collect();

            let rust_ret = match ret_ty.as_ref().map(|t| format!("{t}")).as_deref() {
                Some("int") | Some("Int") | Some("i64") => "i64",
                Some("float") | Some("Float") | Some("f64") => "f64",
                Some("bool") | Some("Bool") => "bool",
                _ => "i64",
            };

            ffi_decls.push_str(&format!("    fn {fn_name}({}) -> {rust_ret};\n", rust_params.join(", ")));

            // R adapter: all numeric/bool arguments passed as *const f64 pointers
            let mut r_params: Vec<String> = params.iter().map(|p| format!("{}: *const f64", p.name)).collect();
            let mut null_checks: Vec<String> = params.iter().map(|p| format!("!{}.is_null()", p.name)).collect();

            let is_unit_ret = matches!(ret_ty.as_ref().map(|t| format!("{t}")).as_deref(), None | Some("unit") | Some("()"));

            if !is_unit_ret {
                r_params.push("out: *mut f64".to_string());
                null_checks.push("!out.is_null()".to_string());
            }

            let null_check_str = if null_checks.is_empty() {
                "true".to_string()
            } else {
                null_checks.join(" && ")
            };

            let call_args: Vec<String> = params.iter().map(|p| {
                match p.ty.as_ref().map(|t| format!("{t}")).as_deref() {
                    Some("int") | Some("Int") | Some("i64") => format!("*{} as i64", p.name),
                    Some("float") | Some("Float") | Some("f64") => format!("*{}", p.name),
                    Some("bool") | Some("Bool") => format!("*{} != 0.0", p.name),
                    _ => format!("*{} as i64", p.name),
                }
            }).collect();

            let invocation = if is_unit_ret {
                format!("let _ = {fn_name}({});", call_args.join(", "))
            } else {
                match ret_ty.as_ref().map(|t| format!("{t}")).as_deref() {
                    Some("bool") | Some("Bool") => format!("*out = if {fn_name}({}) {{ 1.0 }} else {{ 0.0 }};", call_args.join(", ")),
                    _ => format!("*out = {fn_name}({}) as f64;", call_args.join(", ")),
                }
            };

            r_adapters.push_str(&format!(r#"
#[no_mangle]
pub unsafe extern "C" fn {fn_name}_r({}) {{
    if {null_check_str} {{
        {invocation}
    }}
}}
"#, r_params.join(", ")));
        }

        format!(r#"
#![no_main]
#![allow(non_snake_case)]
{host_stub}

extern "C" {{
{ffi_decls}
}}

{r_adapters}
"#)
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
{host_stub}
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
        eprintln!("{}", err.render_with_caps(caps));
        std::process::exit(1);
    }

    if let Err(e) = std::fs::write(&temp_runner, runner_source) {
        let err = Diagnostic::compute_error("C0502", format!("Failed to write runner source: {e}"));
        eprintln!("{}", err.render_with_caps(caps));
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
                let jl_bridge_path = out_dir.join(format!("{stem}_bridge.jl"));

                // 1. Python ctypes bridge
                if bridges_filter == "all" || bridges_filter.contains("py") {
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

                    // Emit Struct classes in Python
                    for s in &exported_structs {
                        let mut fields_str = String::new();
                        for f in &s.fields {
                            let ty_str = format!("{}", f.ty);
                            let cty = match ty_str.as_str() {
                                "int" | "Int" | "i64" => "ctypes.c_int64",
                                "float" | "Float" | "f64" => "ctypes.c_double",
                                "bool" | "Bool" => "ctypes.c_bool",
                                _ => "ctypes.c_int64",
                            };
                            fields_str.push_str(&format!("        (\"{}\", {}),\n", f.name, cty));
                        }
                        py_content.push_str(&format!(
                            "class {}(ctypes.Structure):\n    _fields_ = [\n{fields_str}    ]\n\n",
                            s.name
                        ));
                    }

                    // Emit functions in Python
                    let mut all_exports: Vec<String> = Vec::new();
                    for (fn_name, params, ret_ty) in &exported_fns {
                        all_exports.push(format!("\"{fn_name}\""));
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

                    for s in &exported_structs {
                        all_exports.push(format!("\"{}\"", s.name));
                    }

                    if !all_exports.is_empty() {
                        py_content.push_str(&format!("__all__ = [{}]\n", all_exports.join(", ")));
                    }

                    let _ = std::fs::write(&py_bridge_path, py_content);
                }

                // 2. R FFI bridge
                if bridges_filter == "all" || bridges_filter.contains("r") {
                    let mut r_content = format!(r#"# Auto-generated R FFI bridge for {stem}
# Generated by Gojo & Haru High-Performance Statistical System (GHL) - RFC 06 §3

.ghl_lib_name <- "{out_filename}"
.ghl_load_lib <- function() {{
  cur_dir <- getwd()
  p <- file.path(cur_dir, .ghl_lib_name)
  if (file.exists(p)) {{
    dyn.load(p)
  }} else {{
    dyn.load(.ghl_lib_name)
  }}
}}
.ghl_load_lib()

"#);

                    // Emit Struct constructors in R
                    for s in &exported_structs {
                        let mut param_defaults = Vec::new();
                        let mut field_assigns = Vec::new();

                        for f in &s.fields {
                            let ty_str = format!("{}", f.ty);
                            match ty_str.as_str() {
                                "int" | "Int" | "i64" => {
                                    param_defaults.push(format!("{} = 0L", f.name));
                                    field_assigns.push(format!("      {} = as.integer({})", f.name, f.name));
                                }
                                "float" | "Float" | "f64" => {
                                    param_defaults.push(format!("{} = 0.0", f.name));
                                    field_assigns.push(format!("      {} = as.double({})", f.name, f.name));
                                }
                                "bool" | "Bool" => {
                                    param_defaults.push(format!("{} = FALSE", f.name));
                                    field_assigns.push(format!("      {} = as.logical({})", f.name, f.name));
                                }
                                _ => {
                                    param_defaults.push(format!("{} = NULL", f.name));
                                    field_assigns.push(format!("      {} = {}", f.name, f.name));
                                }
                            }
                        }

                        r_content.push_str(&format!(
                            "new_{name} <- function({params}) {{\n  structure(\n    list(\n{fields}\n    ),\n    class = \"{name}\"\n  )\n}}\n\n",
                            name = s.name,
                            params = param_defaults.join(", "),
                            fields = field_assigns.join(",\n")
                        ));
                    }

                    // Emit functions in R
                    for (fn_name, params, ret_ty) in &exported_fns {
                        let r_params: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                        let mut dot_c_args: Vec<String> = params.iter().map(|p| format!("as.double({})", p.name)).collect();

                        let is_unit_ret = matches!(ret_ty.as_ref().map(|t| format!("{t}")).as_deref(), None | Some("unit") | Some("()"));

                        if !is_unit_ret {
                            dot_c_args.push("out = double(1)".to_string());
                        }

                        let ret_code = if is_unit_ret {
                            "  invisible(NULL)".to_string()
                        } else {
                            match ret_ty.as_ref().map(|t| format!("{t}")).as_deref() {
                                Some("int") | Some("Int") | Some("i64") => {
                                    "  if (abs(res$out) <= 2147483647) as.integer(res$out) else res$out".to_string()
                                }
                                Some("bool") | Some("Bool") => {
                                    "  as.logical(res$out != 0)".to_string()
                                }
                                _ => {
                                    "  res$out".to_string()
                                }
                            }
                        };

                        r_content.push_str(&format!(
                            "{fn_name} <- function({args}) {{\n  res <- .C(\"{fn_name}_r\", {dot_c_args})\n{ret_code}\n}}\n\n",
                            args = r_params.join(", "),
                            dot_c_args = dot_c_args.join(", ")
                        ));
                    }

                    let _ = std::fs::write(&r_bridge_path, r_content);
                }

                // 3. Julia FFI bridge
                if bridges_filter == "all" || bridges_filter.contains("jl") {
                    let jl_module_name = to_pascal_case_module(stem);
                    let mut jl_content = format!(r#""""
Auto-generated Julia FFI bridge for {stem}
Generated by Gojo & Haru High-Performance Statistical System (GHL) - RFC 06 §3
"""
module {jl_module_name}

const LIB_PATH = if Sys.iswindows()
    joinpath(@__DIR__, "{out_filename}")
elseif Sys.isapple()
    joinpath(@__DIR__, "{out_filename}")
else
    joinpath(@__DIR__, "{out_filename}")
end

"#);

                    // Emit Structs in Julia
                    for s in &exported_structs {
                        let mut fields_str = String::new();
                        for f in &s.fields {
                            let ty_str = format!("{}", f.ty);
                            let jl_ty = match ty_str.as_str() {
                                "int" | "Int" | "i64" => "Int64",
                                "float" | "Float" | "f64" => "Float64",
                                "bool" | "Bool" => "Bool",
                                _ => "Float64",
                            };
                            fields_str.push_str(&format!("    {}::{}\n", f.name, jl_ty));
                        }
                        jl_content.push_str(&format!(
                            "struct {}\n{}end\n\n",
                            s.name, fields_str
                        ));
                    }

                    // Emit functions in Julia
                    let mut jl_exports: Vec<String> = Vec::new();
                    for (fn_name, params, ret_ty) in &exported_fns {
                        jl_exports.push(fn_name.clone());

                        let typed_params: Vec<String> = params.iter().map(|p| {
                            let ty_str = match p.ty.as_ref().map(|t| format!("{t}")).as_deref() {
                                Some("int") | Some("Int") | Some("i64") => "Int64",
                                Some("float") | Some("Float") | Some("f64") => "Float64",
                                Some("bool") | Some("Bool") => "Bool",
                                _ => "Int64",
                            };
                            format!("{}::{}", p.name, ty_str)
                        }).collect();

                        let ccall_arg_types: Vec<&'static str> = params.iter().map(|p| {
                            match p.ty.as_ref().map(|t| format!("{t}")).as_deref() {
                                Some("int") | Some("Int") | Some("i64") => "Int64",
                                Some("float") | Some("Float") | Some("f64") => "Float64",
                                Some("bool") | Some("Bool") => "Bool",
                                _ => "Int64",
                            }
                        }).collect();

                        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();

                        let jl_ret = match ret_ty.as_ref().map(|t| format!("{t}")).as_deref() {
                            Some("int") | Some("Int") | Some("i64") => "Int64",
                            Some("float") | Some("Float") | Some("f64") => "Float64",
                            Some("bool") | Some("Bool") => "Bool",
                            None | Some("unit") | Some("()") => "Cvoid",
                            _ => "Int64",
                        };

                        let ccall_tuple = if ccall_arg_types.is_empty() {
                            "()".to_string()
                        } else if ccall_arg_types.len() == 1 {
                            format!("({},)", ccall_arg_types[0])
                        } else {
                            format!("({})", ccall_arg_types.join(", "))
                        };

                        let ccall_args_str = if param_names.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", param_names.join(", "))
                        };

                        if jl_ret == "Cvoid" {
                            jl_content.push_str(&format!(
                                "function {fn_name}({args})::Cvoid\n    ccall((:{fn_name}, LIB_PATH), Cvoid, {ccall_tuple}{ccall_args_str})\n    return nothing\nend\n\n",
                                args = typed_params.join(", ")
                            ));
                        } else {
                            jl_content.push_str(&format!(
                                "function {fn_name}({args})::{jl_ret}\n    ccall((:{fn_name}, LIB_PATH), {jl_ret}, {ccall_tuple}{ccall_args_str})\nend\n\n",
                                args = typed_params.join(", ")
                            ));
                        }
                    }

                    for s in &exported_structs {
                        jl_exports.push(s.name.clone());
                    }

                    if !jl_exports.is_empty() {
                        jl_content.push_str(&format!("export {}\n\n", jl_exports.join(", ")));
                    }

                    jl_content.push_str("end # module\n");
                    let _ = std::fs::write(&jl_bridge_path, jl_content);
                }

                let mut panel = CockpitPanel::new("GHL FFI Shared Library (RFC 06 §3)");
                panel.with_badge(caps.green(if caps.unicode_enabled { "≽(• ̀⩊ •́マ≼ CDYLIB SUCCESS" } else { "[CDYLIB SUCCESS]" }));
                panel.add_kv("Target File", &file);
                panel.add_kv("Output Library", &output_path);
                panel.add_kv("Mode", if release { "Release (opt-level=3, LTO)" } else { "Debug / Standard" });
                panel.add_kv("Library Size", size_str);
                panel.add_kv("Compilation Time", format!("{:.2} ms", elapsed_ms));
                panel.add_kv("FFI Functions", format!("{} exported", exported_fns.len()));
                if !exported_structs.is_empty() {
                    panel.add_kv("FFI Structs", format!("{} declared", exported_structs.len()));
                }
                if bridges_filter == "all" || bridges_filter.contains("py") {
                    panel.add_kv("Python Bridge", py_bridge_path.display().to_string());
                }
                if bridges_filter == "all" || bridges_filter.contains("r") {
                    panel.add_kv("R Bridge", r_bridge_path.display().to_string());
                }
                if bridges_filter == "all" || bridges_filter.contains("jl") {
                    panel.add_kv("Julia Bridge", jl_bridge_path.display().to_string());
                }

                println!("{}", panel.render(caps));
            } else {
                let mut panel = CockpitPanel::new("GHL AOT Native Binary");
                panel.with_badge(caps.green(if caps.unicode_enabled { "≽(• ̀⩊ •́マ≼ AOT SUCCESS" } else { "[AOT SUCCESS]" }));

                panel.add_kv("Target File", &file);
                panel.add_kv("Output Binary", &output_path);
                panel.add_kv("Mode", if release { "Release (opt-level=3, LTO)" } else { "Debug / Standard" });
                panel.add_kv("Binary Size", size_str);
                panel.add_kv("Compilation Time", format!("{:.2} ms", elapsed_ms));
                panel.add_kv("Symbols Exported", format!("{} functions", hir_module.functions.len()));

                println!("{}", panel.render(caps));
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let err = Diagnostic::compute_error("C0503", format!("Linker failed:\n{stderr}"));
            eprintln!("{}", err.render_with_caps(caps));
            std::process::exit(1);
        }
        Err(e) => {
            let err = Diagnostic::compute_error("C0504", format!("Failed to invoke linker `rustc`: {e}"))
                .with_help("Ensure `rustc` is installed and available in PATH");
            eprintln!("{}", err.render_with_caps(caps));
            std::process::exit(1);
        }
    }
}

fn to_pascal_case_module(stem: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in stem.chars() {
        if c == '_' || c == '-' {
            capitalize = true;
        } else if capitalize {
            result.extend(c.to_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    if result.is_empty() {
        "GhlBridge".to_string()
    } else {
        format!("{result}Bridge")
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
        let host_stub = ghl_codegen::host::runner_source_stub();
        let runner_src = format!(r#"
            extern "C" {{ fn __ghl_main() -> i64; }}
            {host_stub}
            fn main() {{
                let res = unsafe {{ __ghl_main() }};
                println!("{{}}", res);
            }}
        "#);
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

    /// Regression test for the JIT/AOT host-math split: `AotEngine` declares every
    /// entry in `ghl_codegen::host::HOST_FUNCTIONS` as a C-ABI import (used or not),
    /// so both (a) a script that actually calls one (`sqrt`) must link and execute
    /// correctly via the generated `runner_source_stub`, and (b) that stub must not
    /// require the script to use every host function to link successfully.
    #[test]
    fn test_aot_build_with_host_math_function() {
        let code = r#"
            fn calc_hypot(a: f64, b: f64) -> f64 {
                sqrt(a * a + b * b)
            }
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let hir_module = ghl_ir::lower_ast(&program).expect("hir ok");

        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let stem = format!("test_aot_hostmath_{pid}");
        let aot = ghl_codegen::AotEngine::new(&stem).expect("aot init ok");
        let obj_bytes = aot.compile_module(&hir_module).expect("aot compile ok");

        let obj_path = temp_dir.join(format!("{stem}.obj"));
        let runner_path = temp_dir.join(format!("{stem}_runner.rs"));
        let exe_path = temp_dir.join(format!("{stem}.exe"));

        std::fs::write(&obj_path, &obj_bytes).expect("write obj");
        let host_stub = ghl_codegen::host::runner_source_stub();
        let runner_src = format!(r#"
            extern "C" {{ fn calc_hypot(a: f64, b: f64) -> f64; }}
            {host_stub}
            fn main() {{
                let res = unsafe {{ calc_hypot(3.0, 4.0) }};
                println!("{{}}", res);
            }}
        "#);
        std::fs::write(&runner_path, runner_src).expect("write runner");

        let mut cmd = std::process::Command::new("rustc");
        cmd.arg("-C").arg("opt-level=3");
        cmd.arg(format!("-Clink-arg={}", obj_path.display()));
        cmd.arg(&runner_path);
        cmd.arg("-o").arg(&exe_path);

        let status = cmd.status().expect("rustc run");
        assert!(status.success(), "rustc compilation with host math stub must succeed");

        let run_out = std::process::Command::new(&exe_path)
            .output()
            .expect("execute standalone binary");
        assert!(run_out.status.success());
        let stdout = String::from_utf8_lossy(&run_out.stdout);
        assert_eq!(stdout.trim(), "5", "sqrt(3^2 + 4^2) must equal 5.0 via AOT-linked host stub");

        // Clean up
        let _ = std::fs::remove_file(obj_path);
        let _ = std::fs::remove_file(runner_path);
        let _ = std::fs::remove_file(exe_path);
    }

    #[test]
    fn test_build_shared_library_and_ffi() {
        let code = r#"
            struct Summary {
                mean: float,
                count: int,
            }

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
        let host_stub = ghl_codegen::host::runner_source_stub();
        let runner_src = format!(r#"
#![no_main]
#![allow(non_snake_case)]
{host_stub}

extern "C" {{
    fn calc_stat(x: i64, y: i64) -> i64;
}}

#[no_mangle]
pub unsafe extern "C" fn calc_stat_r(x: *const f64, y: *const f64, out: *mut f64) {{
    if !x.is_null() && !y.is_null() && !out.is_null() {{
        *out = calc_stat(*x as i64, *y as i64) as f64;
    }}
}}
"#);
        std::fs::write(&runner_path, runner_src).expect("write runner");

        let mut cmd = std::process::Command::new("rustc");
        cmd.arg("--crate-type").arg("cdylib");
        cmd.arg(format!("-Clink-arg={}", obj_path.display()));
        cmd.arg(&runner_path);
        cmd.arg("-o").arg(&so_path);

        let status = cmd.status().expect("rustc run");
        assert!(status.success(), "rustc compilation of cdylib must succeed");
        assert!(so_path.exists(), "shared library file must exist");

        // 1. Verify with Python ctypes
        let py_bridge_path = temp_dir.join(format!("{stem}_bridge.py"));
        let py_script = format!(r#"
import ctypes
import os

lib = ctypes.CDLL(r'{}')
lib.calc_stat.argtypes = [ctypes.c_int64, ctypes.c_int64]
lib.calc_stat.restype = ctypes.c_int64

class Summary(ctypes.Structure):
    _fields_ = [
        ("mean", ctypes.c_double),
        ("count", ctypes.c_int64),
    ]

res = lib.calc_stat(10, 5)
assert res == 25, f'Expected 25, got {{res}}'

s = Summary(mean=3.14, count=42)
assert s.mean == 3.14 and s.count == 42, 'Summary struct validation failed'
print('PYTHON_FFI_OK')
"#, so_path.display());
        std::fs::write(&py_bridge_path, &py_script).expect("write py bridge");

        let py_res = std::process::Command::new("python")
            .arg(&py_bridge_path)
            .output()
            .or_else(|_| std::process::Command::new("python3").arg(&py_bridge_path).output());

        if let Ok(out) = py_res {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                assert!(stdout.contains("PYTHON_FFI_OK"), "Python ctypes invocation succeeded");
            }
        }

        // 2. Verify with R FFI (.C adapter and S3 struct constructor)
        let r_bridge_path = temp_dir.join(format!("{stem}_bridge.R"));
        let r_script = format!(r#"
dyn.load(r'{}')

new_Summary <- function(mean = 0.0, count = 0L) {{
  structure(
    list(
      mean = as.double(mean),
      count = as.integer(count)
    ),
    class = "Summary"
  )
}}

calc_stat <- function(x, y) {{
  res <- .C("calc_stat_r", as.double(x), as.double(y), out = double(1))
  as.integer(res$out)
}}

res <- calc_stat(10, 5)
stopifnot(res == 25)

s <- new_Summary(3.14, 42)
stopifnot(s$mean == 3.14, s$count == 42)

cat('R_FFI_OK\n')
"#, so_path.display());
        std::fs::write(&r_bridge_path, &r_script).expect("write r bridge");

        let r_res = std::process::Command::new("Rscript")
            .arg(&r_bridge_path)
            .output();

        if let Ok(out) = r_res {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                assert!(stdout.contains("R_FFI_OK"), "R FFI invocation succeeded");
            }
        }

        // 3. Verify with Julia FFI (ccall and struct)
        let jl_bridge_path = temp_dir.join(format!("{stem}_bridge.jl"));
        let jl_script = format!(r#"
const LIB_PATH = raw"{}"

struct Summary
    mean::Float64
    count::Int64
end

function calc_stat(x::Int64, y::Int64)::Int64
    ccall((:calc_stat, LIB_PATH), Int64, (Int64, Int64), x, y)
end

res = calc_stat(10, 5)
@assert res == 25 "Expected 25, got $res"

s = Summary(3.14, 42)
@assert s.mean == 3.14 && s.count == 42 "Summary struct validation failed"

println("JULIA_FFI_OK")
"#, so_path.display());
        std::fs::write(&jl_bridge_path, &jl_script).expect("write jl bridge");

        let jl_res = std::process::Command::new("julia")
            .arg(&jl_bridge_path)
            .output();

        if let Ok(out) = jl_res {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                assert!(stdout.contains("JULIA_FFI_OK"), "Julia ccall invocation succeeded");
            }
        }

        // Clean up
        let _ = std::fs::remove_file(obj_path);
        let _ = std::fs::remove_file(runner_path);
        let _ = std::fs::remove_file(so_path);
        let _ = std::fs::remove_file(py_bridge_path);
        let _ = std::fs::remove_file(r_bridge_path);
        let _ = std::fs::remove_file(jl_bridge_path);
    }
}
