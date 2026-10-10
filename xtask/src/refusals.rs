//! Refused commands are said, not dropped (`cargo xtask refusals`).
//!
//! The interface runs commands for the person at it. `LightcraftApp::run` hands the error back;
//! written as `let _ = app.run(…)` a refusal went nowhere: the click did nothing and nobody said
//! why. A widget now says which it means: `app.act(…)` (a toast gives the reason),
//! `app.quiet(…)` (a refusal is expected here; the call says why), or `run` with the `Result`
//! dealt with. This check keeps the dropped form out of the interface's code.

use std::path::{Path, PathBuf};

/// Where the interface's code is.
const DIRS: &[&str] = &["crates/ui-egui/src", "apps/lightcraft/src"];

/// A result dropped on the floor: what follows `let _ =` runs a command.
const RUNS: &[&str] = &["app.run(", "self.run(", "run_item("];

/// The offending lines of one file's text: `(line number, the line)`. Test code (a `tests_*.rs`
/// file is not passed in; a `#[cfg(test)]` module ends the scan) may drop what it likes.
pub fn dropped(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("#[cfg(test)]") {
            break;
        }
        if code.starts_with("//") {
            continue;
        }
        if let Some(rest) = code.strip_prefix("let _ = ")
            && RUNS.iter().any(|run| rest.contains(run))
        {
            found.push((i + 1, code.to_string()));
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") && !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("tests_")) {
            out.push(path);
        }
    }
    Ok(())
}

pub fn run(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    for dir in DIRS {
        rust_files(&root.join(dir), &mut files)?;
    }
    files.sort();
    let mut problems = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        for (line, code) in dropped(&text) {
            problems.push(format!("{}:{line}: {code}", file.strip_prefix(root).unwrap_or(file).display()));
        }
    }
    if problems.is_empty() {
        println!("OK: no command's refusal is dropped in {} ({} files).", DIRS.join(", "), files.len());
        return Ok(());
    }
    for p in &problems {
        eprintln!("{p}");
    }
    Err(format!(
        "{} command result(s) dropped with `let _ =`: use `app.act(…)` so a refusal is said in a toast, `app.quiet(…)` (and say why) where one is expected, or deal with the `Result`",
        problems.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_result_is_found() {
        let text = "fn ui(app: &mut App) {\n    if clicked {\n        let _ = app.run(\"album.move\", json!({}));\n    }\n}\n";
        assert_eq!(dropped(text), [(3, "let _ = app.run(\"album.move\", json!({}));".to_string())]);
        assert_eq!(dropped("    let _ = self.run(\"x\", p);").len(), 1);
        assert_eq!(dropped("    let _ = lightcraft_ui_egui::menubar::run_item(app, &cmd, params);").len(), 1);
        assert_eq!(dropped("        let _ = app.run(\n            \"photo.setMeta\",\n        );").len(), 1, "a call over several lines");
    }

    #[test]
    fn saying_which_is_meant_passes() {
        for ok in [
            "    app.act(\"album.move\", json!({}));",
            "    app.quiet(\"library.select\", json!({}));",
            "    if let Err(e) = app.run(\"x\", p) { app.toast(ctx, e) }",
            "    let r = app.run(\"x\", p);",
            "    let ok = app.run(id, params).is_ok();",
            "    let _ = std::fs::remove_file(path);",
            "    let _ = app.session.execute(\"x\", &p);",
            "    // let _ = app.run(\"x\", p);",
        ] {
            assert!(dropped(ok).is_empty(), "{ok}");
        }
    }

    #[test]
    fn test_code_is_left_alone() {
        let text = "fn ui() {}\n\n#[cfg(test)]\nmod tests {\n    fn t() {\n        let _ = app.run(\"x\", p);\n    }\n}\n";
        assert!(dropped(text).is_empty());
        let before = "fn ui() {\n    let _ = app.run(\"x\", p);\n}\n\n#[cfg(test)]\nmod tests {}\n";
        assert_eq!(dropped(before).len(), 1, "code before the test module still counts");
    }

    #[test]
    fn the_interface_drops_none() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        run(root).unwrap();
    }
}
