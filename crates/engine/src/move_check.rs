//! Sorting the photos of a move to another folder (`photo.moveToFolder`) before anything moves:
//! which ones go, which are in that folder already, and which can't go (a name taken there, the
//! file missing or offline). `photo.checkMove` reports the sort so a confirmation can show the
//! problems up front; the move itself uses the same sort, and leaves out the photos it was told
//! to skip (`skip`) instead of refusing the batch for them.
//!
//! Reads the disk (one look at each file and each name) but writes nothing.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lightcraft_catalog::{PhotoId, Source};
use serde_json::{Value, json};

use crate::cmd::bad;
use crate::rename::RenamePlan;
use crate::{EngineError, Result, Session};

const C: &str = "photo.moveToFolder";

/// Why a photo can't be moved into the folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A file of that name is in the folder already, or another photo of the batch brings one.
    Clash,
    /// The file isn't where the library has it: the disk is offline, or it was moved or deleted.
    Missing,
    /// Only for a photo told to be skipped that has no problem (any more).
    Asked,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Clash => "clash",
            Kind::Missing => "missing",
            Kind::Asked => "asked",
        }
    }
}

/// One photo that doesn't move: its problem, or that it was skipped.
#[derive(Clone, Debug)]
pub(crate) struct Left {
    pub id: PhotoId,
    pub file: String,
    pub kind: Kind,
}

impl Left {
    pub(crate) fn json(&self) -> Value {
        json!({"id": self.id.0, "name": self.file, "kind": self.kind.word()})
    }
}

/// How the photos of a move sort out.
#[derive(Debug, Default)]
pub(crate) struct Sorted {
    /// One per file that moves (a master and its virtual copies move once).
    pub plans: Vec<RenamePlan>,
    /// Photos that go (virtual copies counted, as the person sees them).
    pub movable: usize,
    /// Photos already in the folder.
    pub here: usize,
    /// Photos that can't go and weren't asked to be skipped: they refuse the move.
    pub problems: Vec<Left>,
    /// Photos the caller asked to leave out, with the problem each has (or [`Kind::Asked`]).
    pub skipped: Vec<Left>,
}

#[derive(Clone, Copy)]
enum Fate {
    Move,
    Here,
    Clash,
    Missing,
}

/// Sort `ids` for a move into `dir`. Refuses (as a whole) a photo that isn't in the library or
/// has no file, which no skip can cure; a name clash or a missing file goes to `problems`, or to
/// `skipped` when its photo is in `skip`. A photo skipped never holds a name in the folder.
pub(crate) fn sort(s: &Session, dir: &Path, ids: &[PhotoId], skip: &HashSet<u64>) -> Result<Sorted> {
    // skipping is by file: a virtual copy goes where its master goes
    let skip_paths: HashSet<String> = skip
        .iter()
        .filter_map(|id| match s.catalog.photo(PhotoId(*id)).map(|p| &p.source) {
            Some(Source::File { path }) => Some(path.clone()),
            _ => None,
        })
        .collect();
    let mut out = Sorted::default();
    let mut fates: HashMap<String, Fate> = HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    for &id in ids {
        // (no command named: `execute` puts in the one that was asked for)
        let ph = s.catalog.photo(id).ok_or_else(|| bad("", format!("no photo {}", id.0)))?;
        if ph.local {
            return Err(EngineError::Other(format!("{} is not in the library (Local): add it to the library first", ph.file_name)));
        }
        let Source::File { path } = &ph.source else {
            return Err(EngineError::Other(format!("{} has no file on disk, so it can't be moved", ph.file_name)));
        };
        let left_out = skip_paths.contains(path);
        let fate = match fates.get(path) {
            Some(f) => *f,
            None => {
                let src = Path::new(path);
                let file = src.file_name().ok_or_else(|| EngineError::Other(format!("{path} has no file name")))?;
                let to = dir.join(file);
                let to_s = to.to_string_lossy().to_string();
                let fate = if !src.is_file() {
                    Fate::Missing
                } else if src.parent() == Some(dir) {
                    Fate::Here
                } else if to.exists() || (!left_out && !taken.insert(to_s.to_lowercase())) {
                    Fate::Clash
                } else {
                    Fate::Move
                };
                if matches!(fate, Fate::Move) && !left_out {
                    let name = ph.file_name.clone();
                    out.plans.push(RenamePlan { id: id.0, from: name.clone(), to: name, from_path: Some(path.clone()), to_path: Some(to_s) });
                }
                fates.insert(path.clone(), fate);
                fate
            }
        };
        let kind = match fate {
            Fate::Clash => Kind::Clash,
            Fate::Missing => Kind::Missing,
            Fate::Move | Fate::Here => Kind::Asked,
        };
        let left = Left { id, file: ph.file_name.clone(), kind };
        match (left_out, fate) {
            (true, _) => out.skipped.push(left),
            (false, Fate::Move) => out.movable += 1,
            (false, Fate::Here) => out.here += 1,
            (false, Fate::Clash | Fate::Missing) => out.problems.push(left),
        }
    }
    Ok(out)
}

/// The `skip` parameter: photo ids to leave out of the move.
pub(crate) fn skip_param(p: &Value) -> Result<HashSet<u64>> {
    match p.get("skip") {
        None | Some(Value::Null) => Ok(HashSet::new()),
        Some(Value::Array(a)) => a.iter().map(|v| v.as_u64().ok_or_else(|| bad(C, "`skip` must be photo ids (whole numbers)"))).collect(),
        Some(_) => Err(bad(C, "`skip` must be photo ids (whole numbers)")),
    }
}

/// The folder as a person names it: its last part.
fn folder_name(folder: &str) -> String {
    Path::new(folder).file_name().map_or_else(|| folder.to_string(), |f| f.to_string_lossy().to_string())
}

/// Up to three names, then "and N more".
fn names(of: &[&Left]) -> String {
    let shown: Vec<&str> = of.iter().take(3).map(|l| l.file.as_str()).collect();
    match of.len() - shown.len() {
        0 => shown.join(", "),
        more => format!("{} and {more} more", shown.join(", ")),
    }
}

/// Why a move with `problems` was refused, in plain words with what to do about it.
pub(crate) fn refusal(folder: &str, problems: &[Left]) -> String {
    let clashes: Vec<&Left> = problems.iter().filter(|l| l.kind == Kind::Clash).collect();
    let missing: Vec<&Left> = problems.iter().filter(|l| l.kind == Kind::Missing).collect();
    let verb = |n: usize| if n == 1 { "is" } else { "are" };
    let mut text = String::from("Nothing was moved. ");
    if !clashes.is_empty() {
        text += &format!("{} {} already in “{}”. ", names(&clashes), verb(clashes.len()), folder_name(folder));
    }
    if !missing.is_empty() {
        text += &format!("{} {} missing or offline. ", names(&missing), verb(missing.len()));
    }
    let mut fixes = Vec::new();
    if !clashes.is_empty() {
        fixes.push("rename the clashing photos");
    }
    if !missing.is_empty() {
        fixes.push("reconnect the disk or use File ▸ Find Missing Photos…");
    }
    text += &format!("To go on: {}, or leave these photos out of the move.", fixes.join(", "));
    text
}

/// `photo.checkMove`: how a move of the photos (default: the selection) into `folder` would go,
/// without moving anything.
pub(crate) fn check_command(s: &Session, p: &Value) -> Result<Value> {
    let folder = crate::cmd::browse::folder_param(p, "folder").ok_or_else(|| bad("photo.checkMove", "missing folder"))?;
    let dir = Path::new(folder);
    if !dir.is_dir() {
        return Err(EngineError::Other(format!("{folder} is not a folder (offline, moved or deleted?)")));
    }
    let ids = s.targets(p).map_err(|e| match e {
        EngineError::BadParams { cmd, msg } if cmd.is_empty() => bad("photo.checkMove", msg),
        e => e,
    })?;
    let sorted = sort(s, dir, &ids, &HashSet::new())?;
    Ok(json!({
        "folder": folder,
        "photos": ids.len(),
        "movable": sorted.movable,
        "here": sorted.here,
        "problems": sorted.problems.iter().map(Left::json).collect::<Vec<_>>(),
    }))
}
