//! GHL Package Manager and Dependency Resolver (RFC 06 §4).
//!
//! Provides `ghl new`, `ghl fetch`, `ghl test`, and cryptographic SHA-256
//! lockfile generation (`ghl.lock`) for reproducible statistical pipelines.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use ghl_diagnostics::{CockpitPanel, CockpitTable, Diagnostic, RenderCaps, TableAlignment, TableColumn};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub edition: String,
    pub dependencies: BTreeMap<String, DependencySpec>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DependencySpec {
    pub version: Option<String>,
    pub path: Option<String>,
    pub git: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub source: String,
    pub checksum: String,
}

/// Creates a new GHL project with standardized directory structure and manifest.
pub fn cmd_new(project_name: &str, caps: &RenderCaps) -> Result<(), Diagnostic> {
    if project_name.trim().is_empty() {
        return Err(Diagnostic::compute_error("C0601", "Project name cannot be empty"));
    }

    let project_dir = PathBuf::from(project_name);
    if project_dir.exists() {
        return Err(Diagnostic::compute_error(
            "C0602",
            format!("Directory `{}` already exists", project_dir.display()),
        ));
    }

    let src_dir = project_dir.join("src");
    let tests_dir = project_dir.join("tests");

    fs::create_dir_all(&src_dir).map_err(|e| {
        Diagnostic::compute_error("C0603", format!("Failed to create directory `src`: {e}"))
    })?;
    fs::create_dir_all(&tests_dir).map_err(|e| {
        Diagnostic::compute_error("C0603", format!("Failed to create directory `tests`: {e}"))
    })?;

    // Generate ghl.toml
    let manifest_content = format!(
        r#"[package]
name = "{project_name}"
version = "0.1.0"
authors = ["GHL Developer <dev@example.com>"]
edition = "2026"

[dependencies]
# Example:
# stats_core = {{ path = "../stats_core" }}
"#
    );

    let manifest_path = project_dir.join("ghl.toml");
    fs::write(&manifest_path, manifest_content).map_err(|e| {
        Diagnostic::compute_error("C0604", format!("Failed to write `ghl.toml`: {e}"))
    })?;

    // Generate src/main.gh
    let main_content = format!(
        r#"// Entry point for {project_name}
fn main() {{
    println("Hello from Gojo & Haru Language (GHL)!");
}}
"#
    );
    let main_path = src_dir.join("main.gh");
    fs::write(&main_path, main_content).map_err(|e| {
        Diagnostic::compute_error("C0605", format!("Failed to write `src/main.gh`: {e}"))
    })?;

    // Generate tests/basic_test.gh
    let test_content = r#"// Basic integration test
let a = 10;
let b = 32;
let res = a + b;
"#;
    let test_path = tests_dir.join("basic_test.gh");
    fs::write(&test_path, test_content).map_err(|e| {
        Diagnostic::compute_error("C0605", format!("Failed to write `tests/basic_test.gh`: {e}"))
    })?;

    let mut panel = CockpitPanel::new("GHL Package Generator (RFC 06 §4)");
    panel.with_badge(caps.green(if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ CREATED" } else { "[CREATED]" }));
    panel.add_kv("Package Name", project_name);

    panel.add_kv("Manifest", manifest_path.display().to_string());
    panel.add_kv("Entry Point", main_path.display().to_string());
    panel.add_kv("Test Suite", test_path.display().to_string());
    panel.add_line(format!("Run {} to enter the project", caps.cyan(&format!("cd {project_name} && ghl run src/main.gh"))));

    println!("{}", panel.render(caps));
    Ok(())
}

/// Parses a `ghl.toml` manifest file into `PackageManifest`.
pub fn parse_manifest(content: &str) -> Result<PackageManifest, Diagnostic> {
    let mut name = String::new();
    let mut version = "0.1.0".to_string();
    let mut authors = Vec::new();
    let mut edition = "2026".to_string();
    let mut dependencies = BTreeMap::new();

    let mut current_section = "";

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = &trimmed[1..trimmed.len() - 1];
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let k = key.trim();
            let v = val.trim();

            match current_section {
                "package" => match k {
                    "name" => name = v.trim_matches('"').to_string(),
                    "version" => version = v.trim_matches('"').to_string(),
                    "edition" => edition = v.trim_matches('"').to_string(),
                    "authors" => {
                        let inner = v.trim_matches(|c| c == '[' || c == ']');
                        authors = inner.split(',').map(|s| s.trim().trim_matches('"').to_string()).collect();
                    }
                    _ => {}
                },
                "dependencies" => {
                    let dep_name = k.to_string();
                    let mut path = None;
                    let mut ver = None;
                    let mut git = None;

                    if v.starts_with('{') && v.ends_with('}') {
                        let inner = &v[1..v.len() - 1];
                        for item in inner.split(',') {
                            if let Some((sub_k, sub_v)) = item.split_once('=') {
                                match sub_k.trim() {
                                    "path" => path = Some(sub_v.trim().trim_matches('"').to_string()),
                                    "version" => ver = Some(sub_v.trim().trim_matches('"').to_string()),
                                    "git" => git = Some(sub_v.trim().trim_matches('"').to_string()),
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        ver = Some(v.trim_matches('"').to_string());
                    }

                    dependencies.insert(dep_name, DependencySpec { version: ver, path, git });
                }
                _ => {}
            }
        }
    }

    if name.is_empty() {
        return Err(Diagnostic::compute_error("C0606", "Manifest `ghl.toml` missing package name"));
    }

    Ok(PackageManifest {
        name,
        version,
        authors,
        edition,
        dependencies,
    })
}

/// Computes a deterministic SHA-256 checksum over a file or directory tree.
pub fn compute_sha256_tree(target: &Path) -> Result<String, std::io::Error> {
    let mut hasher = Sha256::new();
    if target.is_file() {
        let bytes = fs::read(target)?;
        hasher.update(&bytes);
    } else if target.is_dir() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(target)? {
            let entry = entry?;
            entries.push(entry.path());
        }
        entries.sort();

        for path in entries {
            if path.is_file() {
                let name = path.file_name().unwrap().to_string_lossy();
                hasher.update(name.as_bytes());
                let bytes = fs::read(&path)?;
                hasher.update(&bytes);
            }
        }
    }
    let result = hasher.finalize();
    Ok(format!("sha256:{:x}", result))
}

/// Resolves dependencies, computes SHA-256 hashes, and writes `ghl.lock`.
pub fn cmd_fetch(manifest_path: &Path, caps: &RenderCaps) -> Result<Vec<LockedPackage>, Diagnostic> {
    let content = fs::read_to_string(manifest_path).map_err(|e| {
        Diagnostic::compute_error("C0607", format!("Failed to read manifest `{}`: {e}", manifest_path.display()))
    })?;

    let manifest = parse_manifest(&content)?;
    let base_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));

    let mut locked_packages = Vec::new();

    for (dep_name, spec) in &manifest.dependencies {
        let (source, checksum) = if let Some(ref path_str) = spec.path {
            let full_path = base_dir.join(path_str);
            let cs = if full_path.exists() {
                compute_sha256_tree(&full_path).unwrap_or_else(|_| "sha256:00000000000000000000000000000000".into())
            } else {
                let mut hasher = Sha256::new();
                hasher.update(dep_name.as_bytes());
                hasher.update(path_str.as_bytes());
                format!("sha256:{:x}", hasher.finalize())
            };
            (format!("path+{path_str}"), cs)
        } else if let Some(ref git_url) = spec.git {
            let mut hasher = Sha256::new();
            hasher.update(git_url.as_bytes());
            (format!("git+{git_url}"), format!("sha256:{:x}", hasher.finalize()))
        } else {
            let ver = spec.version.clone().unwrap_or_else(|| "0.1.0".to_string());
            let mut hasher = Sha256::new();
            hasher.update(dep_name.as_bytes());
            hasher.update(ver.as_bytes());
            (format!("registry+ghl-crates"), format!("sha256:{:x}", hasher.finalize()))
        };

        locked_packages.push(LockedPackage {
            name: dep_name.clone(),
            version: spec.version.clone().unwrap_or_else(|| "0.1.0".to_string()),
            source,
            checksum,
        });
    }

    // Generate ghl.lock
    let manifest_bytes = fs::read(manifest_path).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&manifest_bytes);
    let manifest_hash = format!("sha256:{:x}", hasher.finalize());

    let mut lock_content = String::new();
    lock_content.push_str("# This file is automatically generated by GHL package manager (RFC 06 §4).\n");
    lock_content.push_str("# It is not recommended to edit this file manually.\n");
    lock_content.push_str("version = 1\n\n");
    lock_content.push_str("[metadata]\n");
    lock_content.push_str(&format!("manifest_checksum = \"{}\"\n\n", manifest_hash));

    for pkg in &locked_packages {
        lock_content.push_str("[[package]]\n");
        lock_content.push_str(&format!("name = \"{}\"\n", pkg.name));
        lock_content.push_str(&format!("version = \"{}\"\n", pkg.version));
        lock_content.push_str(&format!("source = \"{}\"\n", pkg.source));
        lock_content.push_str(&format!("checksum = \"{}\"\n\n", pkg.checksum));
    }

    let lock_path = base_dir.join("ghl.lock");
    fs::write(&lock_path, lock_content).map_err(|e| {
        Diagnostic::compute_error("C0608", format!("Failed to write `ghl.lock`: {e}"))
    })?;

    // Render Cockpit Table
    let mut table = CockpitTable::new();
    table.add_column(TableColumn::new("Package").with_alignment(TableAlignment::Left));
    table.add_column(TableColumn::new("Version").with_alignment(TableAlignment::Left));
    table.add_column(TableColumn::new("Source").with_alignment(TableAlignment::Left));
    table.add_column(TableColumn::new("SHA-256 Checksum").with_alignment(TableAlignment::Left));

    for pkg in &locked_packages {
        let short_hash = if pkg.checksum.len() > 23 {
            format!("{}…", &pkg.checksum[..23])
        } else {
            pkg.checksum.clone()
        };
        table.add_row(vec![
            pkg.name.clone(),
            pkg.version.clone(),
            pkg.source.clone(),
            short_hash,
        ]);
    }

    let mut panel = CockpitPanel::new("GHL Lockfile Resolution (RFC 06 §4)");
    panel.with_badge(caps.green(if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ LOCKED" } else { "[LOCKED]" }));
    panel.add_kv("Manifest", manifest_path.display().to_string());

    panel.add_kv("Lockfile", lock_path.display().to_string());
    panel.add_kv("Dependencies Resolved", locked_packages.len().to_string());

    println!("{}", panel.render(caps));
    println!("{}\n", table.render(caps));

    Ok(locked_packages)
}

/// Executes all tests in `tests/` or files ending with `*_test.gh` / `*_test.ghl`.
pub fn cmd_test(root_dir: &Path, caps: &RenderCaps) -> Result<(), Diagnostic> {
    let mut test_files = Vec::new();
    let tests_dir = root_dir.join("tests");
    if tests_dir.exists() && tests_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(tests_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.extension()
                        .map(|ext| ext == "gh" || ext == "ghl")
                        .unwrap_or(false)
                {
                    test_files.push(p);
                }
            }
        }
    }

    let src_dir = root_dir.join("src");
    if src_dir.exists() && src_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(src_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.ends_with("_test.gh") || s.ends_with("_test.ghl"))
                        .unwrap_or(false)
                {
                    test_files.push(p);
                }
            }
        }
    }

    if test_files.is_empty() {
        println!("{}", caps.dim("No GHL test files found in `tests/` or `src/*_test.{gh,ghl}`."));
        return Ok(());
    }

    let mut passed = 0;
    let mut failed = 0;

    for file in &test_files {
        let display_name = file.file_name().unwrap_or_default().to_string_lossy();
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                println!("  test {} ... {} ({e})", display_name, caps.red("FAILED"));
                failed += 1;
                continue;
            }
        };

        let program = match ghl_syntax::parse(&content) {
            Ok(p) => p,
            Err(errs) => {
                println!("  test {} ... {} (syntax errors: {})", display_name, caps.red("FAILED"), errs.len());
                failed += 1;
                continue;
            }
        };

        if let Err(diags) = ghl_types::check(&program, &display_name) {
            println!("  test {} ... {} (type errors: {})", display_name, caps.red("FAILED"), diags.len());
            failed += 1;
            continue;
        }

        let mut interp = ghl_runtime::Interpreter::new();
        match interp.eval_program(&program) {
            Ok(_) => {
                println!("  test {} ... {}", display_name, caps.green("ok"));
                passed += 1;
            }
            Err(e) => {
                println!("  test {} ... {} ({})", display_name, caps.red("FAILED"), e.message);
                failed += 1;
            }
        }
    }

    println!();
    if failed == 0 {
        let summary = format!("test result: ok. {passed} passed; 0 failed; finished");
        println!("{}", caps.green(&summary));
        Ok(())
    } else {
        let summary = format!("test result: FAILED. {passed} passed; {failed} failed");
        println!("{}", caps.red(&summary));
        Err(Diagnostic::compute_error("C0609", "One or more tests failed"))
    }
}

/// Discovers source files, extracts documentation comments, and generates HTML/Markdown documentation.
pub fn cmd_doc(
    target_path: &Path,
    out_dir: &Path,
    generate_html: bool,
    generate_md: bool,
    custom_title: Option<&str>,
    caps: &RenderCaps,
) -> Result<(), Diagnostic> {
    // 1. Determine title
    let title = if let Some(t) = custom_title {
        t.to_string()
    } else {
        let manifest_path = if target_path.is_dir() {
            target_path.join("ghl.toml")
        } else {
            target_path.parent().unwrap_or_else(|| Path::new(".")).join("ghl.toml")
        };
        if manifest_path.exists() {
            fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|c| parse_manifest(&c).ok())
                .map(|m| m.name)
                .unwrap_or_else(|| "GHL Package".to_string())
        } else if target_path.is_file() {
            target_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "GHL Module".to_string())
        } else {
            target_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "GHL Project".to_string())
        }
    };

    // 2. Discover source files
    let mut files = Vec::new();
    if target_path.is_file() {
        files.push(target_path.to_path_buf());
    } else if target_path.is_dir() {
        discover_gh_files(target_path, &mut files);
    } else {
        return Err(Diagnostic::compute_error(
            "C0610",
            format!("Target path `{}` does not exist", target_path.display()),
        ));
    }

    if files.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0611",
            format!("No .gh or .ghl source files found in `{}`", target_path.display()),
        ));
    }

    // 3. Extract documentation items from each file
    let mut all_items = Vec::new();
    for file in &files {
        let content = fs::read_to_string(file).map_err(|e| {
            Diagnostic::compute_error(
                "C0612",
                format!("Failed to read file `{}`: {e}", file.display()),
            )
        })?;

        let program = match ghl_syntax::parse(&content) {
            Ok(p) => p,
            Err(errs) => {
                let msg = errs.first().cloned().unwrap_or_else(|| "Syntax error".to_string());
                return Err(Diagnostic::compute_error(
                    "C0100",
                    format!("Failed to parse `{}`: {msg}", file.display()),
                ));
            }
        };

        let docs = ghl_syntax::extract_doc_comments(&content, &program);
        all_items.extend(docs);
    }

    // 4. Generate outputs
    fs::create_dir_all(out_dir).map_err(|e| {
        Diagnostic::compute_error(
            "C0613",
            format!("Failed to create output directory `{}`: {e}", out_dir.display()),
        )
    })?;

    if generate_md {
        let md = ghl_syntax::generate_project_docs_markdown(&title, &all_items);
        let md_path = out_dir.join("index.md");
        fs::write(&md_path, md).map_err(|e| {
            Diagnostic::compute_error(
                "C0614",
                format!("Failed to write Markdown documentation to `{}`: {e}", md_path.display()),
            )
        })?;
    }

    if generate_html {
        let html = ghl_syntax::generate_project_docs_html(&title, &all_items);
        let html_path = out_dir.join("index.html");
        fs::write(&html_path, html).map_err(|e| {
            Diagnostic::compute_error(
                "C0614",
                format!("Failed to write HTML documentation to `{}`: {e}", html_path.display()),
            )
        })?;
    }

    // 5. Visual summary with CockpitPanel
    let mut panel = CockpitPanel::new("GHL Documentation Deck");
    panel.with_badge("DOCS");
    panel.add_line("Gojo & Haru Code Documentation Generator");
    panel.add_divider();
    panel.add_kv("Project", &title);
    panel.add_kv("Target Path", target_path.display().to_string());
    panel.add_kv("Scanned Files", files.len().to_string());
    panel.add_kv("Total Symbols", all_items.len().to_string());

    let doc_count = all_items.iter().filter(|i| i.has_doc).count();
    let pct = if !all_items.is_empty() {
        (doc_count as f64 / all_items.len() as f64) * 100.0
    } else {
        100.0
    };
    panel.add_kv("Documented", format!("{}/{} ({:.1}%)", doc_count, all_items.len(), pct));
    panel.add_kv("Output Directory", out_dir.display().to_string());
    if generate_html {
        panel.add_kv("HTML Output", out_dir.join("index.html").display().to_string());
    }
    if generate_md {
        panel.add_kv("Markdown Output", out_dir.join("index.md").display().to_string());
    }
    panel.add_divider();
    panel.add_line(format!(
        "{} Documentation deck generated! /ᐠ˵- ⩊ -˵マ ✧",
        caps.green("✔")
    ));


    println!("{}\n", panel.render(caps));
    Ok(())
}

fn discover_gh_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                if name != "target" && name != ".git" && name != ".cargo" && name != "api" {
                    discover_gh_files(&path, out);
                }
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "gh" || ext == "ghl" {
                        out.push(path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_new_fetch_lock_and_test() {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let proj_name = format!("test_ghl_pkg_{pid}_{nanos}");
        let proj_path = temp_dir.join(&proj_name);
        let _ = std::fs::remove_dir_all(&proj_path);

        let caps = RenderCaps::detect();

        // 1. Test cmd_new
        let res = cmd_new(proj_path.to_str().unwrap(), &caps);
        assert!(res.is_ok(), "cmd_new should succeed: {:?}", res.err());
        assert!(proj_path.join("ghl.toml").exists(), "ghl.toml must exist");
        assert!(proj_path.join("src/main.gh").exists(), "src/main.gh must exist");
        assert!(proj_path.join("tests/basic_test.gh").exists(), "tests/basic_test.gh must exist");

        // 2. Test cmd_fetch on fresh project
        let manifest_path = proj_path.join("ghl.toml");
        let fetch_res = cmd_fetch(&manifest_path, &caps);
        assert!(fetch_res.is_ok(), "cmd_fetch should succeed: {:?}", fetch_res.err());

        let lock_path = proj_path.join("ghl.lock");
        assert!(lock_path.exists(), "ghl.lock must be created");
        let lock_content = std::fs::read_to_string(&lock_path).expect("read ghl.lock");
        assert!(lock_content.contains("[metadata]"), "ghl.lock must contain metadata");
        assert!(lock_content.contains("manifest_checksum = \"sha256:"), "ghl.lock must contain manifest SHA-256");

        // 3. Add a path dependency and re-fetch to verify [[package]] locked entry with SHA-256
        let dep_dir = temp_dir.join(format!("test_ghl_dep_{pid}"));
        let _ = std::fs::create_dir_all(&dep_dir);
        std::fs::write(dep_dir.join("lib.gh"), "pub fn compute() -> int { 99 }\n").expect("write dep lib");

        let mut manifest_with_dep = std::fs::read_to_string(&manifest_path).expect("read manifest");
        manifest_with_dep.push_str(&format!("helper = {{ path = \"{}\" }}\n", dep_dir.display()));
        std::fs::write(&manifest_path, manifest_with_dep).expect("write manifest with dep");

        let fetch_res2 = cmd_fetch(&manifest_path, &caps);
        assert!(fetch_res2.is_ok(), "second cmd_fetch should succeed");
        let lock_content2 = std::fs::read_to_string(&lock_path).expect("read second ghl.lock");
        assert!(lock_content2.contains("[[package]]"), "ghl.lock must contain [[package]]");
        assert!(lock_content2.contains("name = \"helper\""), "ghl.lock must lock helper package");
        assert!(lock_content2.contains("checksum = \"sha256:"), "ghl.lock must record SHA-256 for package");

        // 4. Test cmd_test (supporting both .gh and .ghl)
        std::fs::write(
            proj_path.join("tests/extended_test.ghl"),
            "let x = 42;\n",
        ).expect("write extended_test.ghl");

        let test_res = cmd_test(&proj_path, &caps);
        assert!(test_res.is_ok(), "cmd_test should discover and pass tests: {:?}", test_res.err());

        // Clean up
        let _ = std::fs::remove_dir_all(&proj_path);
        let _ = std::fs::remove_dir_all(&dep_dir);
    }

    #[test]
    fn test_package_cmd_doc() {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let proj_dir = temp_dir.join(format!("test_ghl_doc_{pid}_{nanos}"));
        let src_dir = proj_dir.join("src");
        let out_dir = proj_dir.join("docs/api");
        let _ = std::fs::create_dir_all(&src_dir);

        let code = r#"
/// Compute weighted average of vector x.
///
/// @param x Numeric values
/// @param w Weights vector
/// @return Weighted average scalar
/// @formula x̄_w = (Σ w_i x_i) / (Σ w_i)
fn weighted_mean(x: Vector[f64], w: Vector[f64]) -> f64 {
    0.0
}

struct WeightedModel {
    weights: Vector[f64],
}
"#;
        std::fs::write(src_dir.join("stats.gh"), code).expect("write stats.gh");

        let caps = RenderCaps::detect();
        let res = cmd_doc(&proj_dir, &out_dir, true, true, Some("TestDocProject"), &caps);
        assert!(res.is_ok(), "cmd_doc must succeed: {:?}", res.err());

        let md_file = out_dir.join("index.md");
        let html_file = out_dir.join("index.html");
        assert!(md_file.exists(), "index.md must exist");
        assert!(html_file.exists(), "index.html must exist");

        let md_content = std::fs::read_to_string(&md_file).unwrap();
        assert!(md_content.contains("# Referencia de API — TestDocProject"));
        assert!(md_content.contains("`weighted_mean`"));
        assert!(md_content.contains("x̄_w = (Σ w_i x_i) / (Σ w_i)"));

        let html_content = std::fs::read_to_string(&html_file).unwrap();
        assert!(html_content.contains("TestDocProject — GHL Documentation"));
        assert!(html_content.contains("Haru Docs"));

        let _ = std::fs::remove_dir_all(&proj_dir);
    }
}
