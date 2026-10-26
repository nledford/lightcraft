//! Filter presets: the filter bar's settings saved under a name ("Five stars", "Unedited raws")
//! and applied in one step. Saved with the library.

use lightcraft_catalog::Filter;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::{LibrarySource, Result, Session};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilterPreset {
    pub name: String,
    pub filter: Filter,
}

fn list(s: &Session) -> Value {
    json!(s.filter_presets)
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "filter.savePreset";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(C, "missing `name`"))?.to_string();
    if s.filter == Filter::default() {
        return Err(bad(C, "the filter is empty: set some filters first"));
    }
    let preset = FilterPreset { name: name.clone(), filter: s.filter.clone() };
    match s.filter_presets.iter_mut().find(|f| f.name.eq_ignore_ascii_case(&name)) {
        Some(f) => *f = preset,
        None => s.filter_presets.push(preset),
    }
    s.save_prefs()?;
    Ok(list(s))
}

fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "filter.applyPreset";
    let name = str_param(p, "name").ok_or_else(|| bad(C, "missing `name`"))?;
    let f = s.filter_presets.iter().find(|f| f.name.eq_ignore_ascii_case(name)).ok_or_else(|| bad(C, format!("no filter preset `{name}`")))?;
    let mut filter = f.filter.clone();
    // the album shown is the source: an album in the filter would be replaced on screen and narrow
    // the next source unseen (the filter bar has no album chip), so the rest of the preset applies
    let skipped_album = filter.album.is_some() && matches!(s.source, LibrarySource::Album(_));
    if skipped_album {
        filter.album = None;
    }
    s.filter = filter;
    s.shelved_filter = None;
    let mut r = json!({"photos": s.visible().len()});
    if skipped_album {
        r["skipped"] = json!(["album"]);
    }
    Ok(r)
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "filter.deletePreset";
    let name = str_param(p, "name").ok_or_else(|| bad(C, "missing `name`"))?;
    let n = s.filter_presets.len();
    s.filter_presets.retain(|f| !f.name.eq_ignore_ascii_case(name));
    if n == s.filter_presets.len() {
        return Err(bad(C, format!("no filter preset `{name}`")));
    }
    s.save_prefs()?;
    Ok(list(s))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "filter.presets", "Filter Presets", [], None, "{} → [{name, filter}]", always, |s, _| Ok(list(s))),
        cmd!(
            "filter.savePreset",
            "Save Filter Preset",
            [],
            None,
            "{name} — the current filter (library.filter), saved with the library",
            always,
            save
        ),
        cmd!(
            "filter.applyPreset",
            "Apply Filter Preset",
            [],
            None,
            "{name} — replaces the current filter → {photos, skipped?: [\"album\"]}; while an album is shown the preset's album is left out (the album shown is the source) and `skipped` says so",
            always,
            apply
        ),
        cmd!("filter.deletePreset", "Delete Filter Preset", [], None, "{name}", always, delete),
    ]
}
