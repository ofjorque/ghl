//! `ghl fmt` — format GHL source files to canonical style (RFC 12), or `--check` only.

use ghl_diagnostics::{CockpitPanel, RenderCaps};
use std::time::Instant;

pub fn cmd_fmt(args: &[String], caps: &RenderCaps) {
    let mut check_only = false;
    let mut targets = Vec::new();

    for arg in args {
        if arg == "--check" {
            check_only = true;
        } else if !arg.starts_with('-') || arg == "-" {
            targets.push(arg.clone());
        }
    }

    if targets.len() == 1 && targets[0] == "-" {
        use std::io::Read;
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_ok() {
            match ghl_syntax::format_source(&buf) {
                Ok(formatted) => {
                    print!("{}", formatted);
                    return;
                }
                Err(e) => {
                    eprintln!("{}", e.render_with_caps(caps));
                    std::process::exit(1);
                }
            }
        }
    }

    if targets.is_empty() {
        targets.push(".".to_string());
    }

    let start = Instant::now();
    let mut files_scanned = 0;
    let mut files_formatted = 0;
    let mut files_clean = 0;
    let mut total_lines = 0;
    let mut unformatted_files = Vec::new();

    fn collect_files(path: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "gh" || ext == "ghl" {
                    files.push(path.to_path_buf());
                }
            }
        } else if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        let fname = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
                        if fname != "target"
                            && fname != ".git"
                            && fname != "node_modules"
                            && fname != ".gemini"
                        {
                            collect_files(&p, files);
                        }
                    } else {
                        collect_files(&p, files);
                    }
                }
            }
        }
    }

    let mut file_paths = Vec::new();
    for target in &targets {
        collect_files(std::path::Path::new(target), &mut file_paths);
    }

    for file_path in &file_paths {
        files_scanned += 1;
        match std::fs::read_to_string(file_path) {
            Ok(content) => {
                total_lines += content.lines().count();
                match ghl_syntax::format_source(&content) {
                    Ok(formatted) => {
                        if content == formatted {
                            files_clean += 1;
                        } else {
                            files_formatted += 1;
                            unformatted_files.push(file_path.display().to_string());
                            if !check_only {
                                if let Err(e) = std::fs::write(file_path, &formatted) {
                                    eprintln!("Error writing {}: {}", file_path.display(), e);
                                }
                            }
                        }
                    }
                    Err(d) => {
                        eprintln!("{}: {}", file_path.display(), d.render_with_caps(caps));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", file_path.display(), e);
            }
        }
    }

    let elapsed = start.elapsed();
    let mut panel = CockpitPanel::new("GHL Formatter (RFC 12)");
    if check_only && !unformatted_files.is_empty() {
        panel.with_badge(caps.yellow(if caps.unicode_enabled {
            "/ᐠ ¬`‸´¬ マ DIFF DETECTED"
        } else {
            "[DIFF DETECTED]"
        }));
        panel.add_kv("Files Scanned", files_scanned.to_string());
        panel.add_kv("Total Lines", total_lines.to_string());
        panel.add_kv(
            "Duration",
            format!("{:.2} ms", elapsed.as_secs_f64() * 1000.0),
        );
        panel.add_kv(
            "Unformatted Files",
            format!("{} require formatting", unformatted_files.len()),
        );
        for f in unformatted_files.iter().take(5) {
            panel.add_line(format!("  - {}", f));
        }
        if unformatted_files.len() > 5 {
            panel.add_line(format!("  ... and {} more", unformatted_files.len() - 5));
        }
        panel.add_line("Run `ghl fmt <path>` to format files in-place.");
        println!("{}", panel.render(caps));
        std::process::exit(1);
    } else {
        panel.with_badge(caps.green(if caps.unicode_enabled {
            "/ᐠ˵- ⩊ -˵マ ✧ ALL CLEAN"
        } else {
            "[ALL CLEAN]"
        }));
        panel.add_kv("Files Scanned", files_scanned.to_string());
        panel.add_kv("Total Lines", total_lines.to_string());
        panel.add_kv(
            "Duration",
            format!("{:.2} ms", elapsed.as_secs_f64() * 1000.0),
        );
        if check_only {
            panel.add_kv(
                "Status",
                format!("{} files already canonically formatted", files_clean),
            );
        } else {
            panel.add_kv(
                "Status",
                format!(
                    "{} files formatted | {} clean",
                    files_formatted, files_clean
                ),
            );
        }
        println!("{}", panel.render(caps));
    }
}
