//! `cargo xtask type`: the interface sets its text in the sizes of `theme::TextSize`.
//!
//! Outside `crates/ui-egui/src/theme.rs`, a line of the interface's code may not give a font a
//! size in numbers past the theme (`.size(11.5)`, `FontId::proportional(13.0)`,
//! `FontId::new(13.0, …)`, `FontId::monospace(12.0)`): that is how text comes to be off the scale.
//! Name the size (`t.caption()`, `t.body()`).
//!
//! A size asked of the theme that is not on the scale (`t.font(12.5)`, `t.semibold(13.5)`) is
//! drawn at the nearest one, so it does no harm on screen: such lines are counted and listed with
//! `--list`, not refused, though the number is then not what is drawn.
//!
//! The scale is read from `TextSize::points` in theme.rs. Test code is left alone.

use std::path::Path;

/// The sizes of the scale, read from the arms of `TextSize::points` (`TextSize::Body => 13.0,`).
pub fn scale(theme: &str) -> Vec<f32> {
    theme
        .lines()
        .filter_map(|line| {
            let (name, points) = line.trim().strip_prefix("TextSize::")?.split_once(" => ")?;
            let named = !name.is_empty() && name.chars().all(char::is_alphanumeric);
            points.strip_suffix(',').filter(|_| named)?.parse::<f32>().ok()
        })
        .collect()
}

/// The number written right after `open` (an index just past a `(`), when the argument starts
/// with one: `13.0`, `11.5_f32`, `12`.
fn number_at(line: &str, open: usize) -> Option<f32> {
    let rest = line.get(open..)?.trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
    let digits = rest.get(..end)?;
    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    digits.trim_end_matches('.').parse().ok()
}

/// What one file of the interface does with text sizes, a line each (`path:line: what, and what
/// to write`): `refused` are sizes given past the theme, `nearest` sizes asked of the theme that
/// it draws at another.
#[derive(Debug, Default, PartialEq)]
pub struct Found {
    pub refused: Vec<String>,
    pub nearest: Vec<String>,
}

pub fn check(path: &str, source: &str, scale: &[f32]) -> Found {
    const PAST_THE_THEME: [&str; 4] = [".size(", "FontId::proportional(", "FontId::monospace(", "FontId::new("];
    const THROUGH_THE_THEME: [&str; 2] = [".font(", ".semibold("];
    let mut found = Found::default();
    let mut lines = source.lines().enumerate().peekable();
    while let Some((n, line)) = lines.next() {
        let code = line.trim_start();
        // (the tests of a file are its last module)
        if code == "#[cfg(test)]" && lines.peek().is_some_and(|(_, next)| next.trim_start().starts_with("mod ")) {
            break;
        }
        if code.starts_with("//") {
            continue;
        }
        let at = format!("{path}:{}", n + 1);
        for needle in PAST_THE_THEME {
            for (i, _) in line.match_indices(needle) {
                if let Some(points) = number_at(line, i + needle.len()) {
                    found.refused.push(format!("{at}: a text size of its own ({points}): name a size of the scale (t.caption(), t.body(), …)"));
                }
            }
        }
        for needle in THROUGH_THE_THEME {
            for (i, _) in line.match_indices(needle) {
                if let Some(points) = number_at(line, i + needle.len()).filter(|p| !scale.contains(p)) {
                    found.nearest.push(format!("{at}: {points} pt is drawn at the nearest size of the scale: name the size it is to be"));
                }
            }
        }
    }
    found
}

/// The source files of the interface, test modules (`tests_*.rs`, `*_tests.rs`) left out.
fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if path.is_dir() {
            sources(&path, out)?;
        } else if name.ends_with(".rs") && !name.starts_with("tests_") && !name.ends_with("_tests.rs") && name != "theme.rs" {
            out.push(path);
        }
    }
    Ok(())
}

pub fn run(root: &Path, list: bool) -> Result<(), String> {
    let ui = root.join("crates/ui-egui/src");
    let theme = ui.join("theme.rs");
    let scale = scale(&std::fs::read_to_string(&theme).map_err(|e| format!("{}: {e}", theme.display()))?);
    if scale.is_empty() {
        return Err(format!("{}: no `TextSize::… => points,` arms found: where is the scale?", theme.display()));
    }
    let mut files = Vec::new();
    sources(&ui, &mut files)?;
    files.sort();
    let mut found = Found::default();
    for file in &files {
        let source = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let shown = file.strip_prefix(root).unwrap_or(file).display().to_string();
        let Found { refused, nearest } = check(&shown, &source, &scale);
        found.refused.extend(refused);
        found.nearest.extend(nearest);
    }
    if list {
        found.nearest.iter().for_each(|line| println!("{line}"));
    }
    for line in &found.refused {
        eprintln!("{line}");
    }
    if !found.refused.is_empty() {
        return Err(format!("{} text size(s) given past the theme (see crates/ui-egui/src/theme.rs, TextSize)", found.refused.len()));
    }
    println!("type: {} files give no text a size past the scale {scale:?}", files.len());
    if !found.nearest.is_empty() && !list {
        println!("type: {} size(s) asked by number are drawn at the nearest size (cargo xtask type --list)", found.nearest.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALE: [f32; 7] = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 18.0];

    #[test]
    fn the_scale_is_read_from_the_theme() {
        let theme = "match self {\n    TextSize::Micro => 10.0,\n    TextSize::Body => 13.0,\n}\nlet x = TextSize::nearest(2.0);\nTextSize::Body => t.body(),";
        assert_eq!(scale(theme), vec![10.0, 13.0]);
        let real = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ui-egui/src/theme.rs")).unwrap();
        assert_eq!(scale(&real), SCALE);
    }

    #[test]
    fn a_size_given_past_the_theme_is_refused() {
        // Given code that gives text a size in numbers
        let source = "ui.label(RichText::new(a).size(11.5));\nlet f = egui::FontId::proportional(13.0);\nlet g = FontId::new( 12.0, family);\nFontId::monospace(12.0)";
        // When it is checked
        let Found { refused, nearest } = check("panels/x.rs", source, &SCALE);
        // Then each line is refused, on or off the scale
        assert!(nearest.is_empty(), "{nearest:?}");
        assert_eq!(refused.len(), 4, "{refused:?}");
        assert!(refused[0].starts_with("panels/x.rs:1: ") && refused[0].contains("11.5"), "{refused:?}");
        assert!(refused[2].starts_with("panels/x.rs:3: "), "{refused:?}");
    }

    #[test]
    fn a_size_asked_of_the_theme_off_the_scale_is_listed_and_not_refused() {
        // Given code that asks the theme for sizes by number, two of them between sizes
        let source = "p.text(at, a, s, t.font(12.5), c);\nlet f = t.semibold(13.5);\nlet ok = (t.font(13.0), tokens.semibold(11.0));";
        // When it is checked
        let Found { refused, nearest } = check("a.rs", source, &SCALE);
        // Then nothing is refused (the theme draws them on the scale), and the two are listed
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(nearest.len(), 2, "{nearest:?}");
        assert!(nearest[0].contains("a.rs:1") && nearest[0].contains("12.5") && nearest[1].contains("a.rs:2"), "{nearest:?}");
    }

    #[test]
    fn named_sizes_sizes_of_other_things_comments_and_tests_pass() {
        let source = "\
let a = RichText::new(x).font(t.caption()).color(c);
let b = galley.size().x + rect.size().y;
let c = FontId::new(size.points(), family);
let d = image.size();
let e = t.font(size);
// was .size(11.5) and t.font(12.5)
let f = vec.resize(4.0);
#[cfg(test)]
mod tests {
    fn t() { let f = egui::FontId::proportional(9.0); }
}";
        assert_eq!(check("a.rs", source, &SCALE), Found::default());
    }
}
