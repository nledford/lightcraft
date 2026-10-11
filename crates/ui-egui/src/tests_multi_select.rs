//! Several rows of a sidebar list chosen at once, driven through the control channel: Cmd/Ctrl+click
//! and Shift+click in the left panel's Albums, Folders, Keywords and By Date and in the right
//! panel's Keyword List, and what a context menu, a drag and the − button do with several rows.
//!
//! Scenarios:
//! - Given albums in the sidebar, when two are Cmd+clicked, then both are highlighted and the
//!   grid shows the photos of either; Cmd+click on one of them takes it away again.
//! - When one album is clicked and another Shift+clicked, then every album between them is
//!   chosen, but none inside a closed folder; a plain click chooses one alone again.
//! - Given several albums chosen, when the menu of one of them is opened, then it offers what can
//!   be done to all (Delete N Albums… asks first, and is one step to undo); the menu of an album
//!   that is not chosen is that album's own, and the chosen ones stay chosen.
//! - Given two albums chosen, when one is dragged onto a folder, then both move into it.
//! - Keywords and dates in the left panel: Cmd+click filters by any of those chosen; a plain
//!   click on the one keyword shown still clears it.
//! - Folders: Cmd+click shows the photos of both folders; the menu removes both after asking.
//! - The Keyword List: Cmd+click and Shift+click pick several; − and the menu delete them all
//!   after one question; the menu gives them all to the selected photos; dragging one of them
//!   nests them all.

use std::time::Duration;

use lightcraft_catalog::{AlbumId, PhotoId};
use lightcraft_engine::LibrarySource;
use serde_json::{Value, json};

use crate::headless::Headless;
use crate::panels::side_select::Several;
use crate::state::Dialog;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

/// The demo library in the grid, the left panel open, in a window tall enough for every row.
fn app(right: &str) -> Headless {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1500.0, 3000.0], 1.0);
    ask(&mut h, "ui.set", json!({"view": "photoGrid", "leftPanel": true, "right": right}));
    h
}

fn ask(h: &mut Headless, method: &str, params: Value) {
    let r = h.request(method, params.clone(), T);
    assert_eq!(r["ok"], true, "{method} {params}: {r}");
    h.settle(SETTLE);
}

fn run(h: &mut Headless, command: &str, params: Value) -> Value {
    let r = h.request("engine.execute", json!({"command": command, "params": params}), T);
    assert_eq!(r["ok"], true, "{command}: {r}");
    h.settle(SETTLE);
    r["result"].clone()
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn click(h: &mut Headless, id: &str) {
    ask(h, "ui.clickWidget", json!({"id": id}));
}
fn cmd_click(h: &mut Headless, id: &str) {
    ask(h, "ui.clickWidget", json!({"id": id, "cmd": true}));
}
fn shift_click(h: &mut Headless, id: &str) {
    ask(h, "ui.clickWidget", json!({"id": id, "shift": true}));
}
fn right_click(h: &mut Headless, id: &str) {
    ask(h, "ui.clickWidget", json!({"id": id, "button": "right"}));
}

fn center(h: &Headless, id: &str) -> egui::Pos2 {
    h.app.widgets.iter().rev().find(|(w, _)| w == id).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("no {id} on screen"))
}

/// Drag the widget `from` and drop it on the widget `onto`.
fn drag(h: &mut Headless, from: &str, onto: &str) {
    let to = center(h, onto);
    ask(h, "ui.dragWidget", json!({"id": from, "toX": to.x, "toY": to.y, "steps": 12}));
}

/// An album called `name` holding `photos`.
fn album(h: &mut Headless, name: &str, parent: Option<u64>, photos: &[u64]) -> u64 {
    let id = run(h, "album.create", json!({"name": name, "parent": parent}))["id"].as_u64().unwrap();
    if !photos.is_empty() {
        run(h, "library.select", json!({"ids": photos}));
        run(h, "album.addPhotos", json!({"id": id, "ids": photos}));
    }
    id
}

fn row(id: u64) -> String {
    format!("source:album:{id}")
}

fn shown(h: &Headless) -> Vec<u64> {
    h.app.session.source_albums().iter().map(|a| a.0).collect()
}

fn visible(h: &mut Headless) -> Vec<u64> {
    let mut v: Vec<u64> = h.app.session.visible_cloned().iter().map(|p| p.0).collect();
    v.sort_unstable();
    v
}

/// Four albums `Sel A`…`Sel D`, each with one photo of its own (and A and C sharing one).
fn four_albums(h: &mut Headless) -> ([u64; 4], Vec<u64>) {
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(5).map(|p| p.0).collect();
    let a = album(h, "Sel A", None, &[ids[0], ids[4]]);
    let b = album(h, "Sel B", None, &[ids[1]]);
    let c = album(h, "Sel C", None, &[ids[2], ids[4]]);
    let d = album(h, "Sel D", None, &[ids[3]]);
    ([a, b, c, d], ids)
}

#[test]
fn cmd_click_shows_several_albums_and_takes_one_away_again() {
    let mut h = app("info");
    let ([a, _, c, _], ids) = four_albums(&mut h);
    click(&mut h, &row(a));
    cmd_click(&mut h, &row(c));
    assert_eq!(shown(&h), vec![a, c]);
    let mut want = vec![ids[0], ids[2], ids[4]];
    want.sort_unstable();
    assert_eq!(visible(&mut h), want, "the photos of either, each once");
    assert!(has(&h, &format!("highlight:album:{a}")) && has(&h, &format!("highlight:album:{c}")), "both rows are highlighted");
    // Cmd+click on one of them takes it away…
    cmd_click(&mut h, &row(a));
    assert_eq!(shown(&h), vec![c]);
    assert!(!has(&h, &format!("highlight:album:{a}")));
    // …and on the last one too: back to every photo
    cmd_click(&mut h, &row(c));
    assert_eq!(h.app.session.source, LibrarySource::All);
}

#[test]
fn shift_click_takes_the_albums_between_and_a_plain_click_one_alone() {
    let mut h = app("info");
    let ([a, b, c, d], _) = four_albums(&mut h);
    click(&mut h, &row(a));
    shift_click(&mut h, &row(c));
    assert_eq!(shown(&h), vec![a, b, c], "the row clicked is the last chosen");
    assert_eq!(h.app.session.source, LibrarySource::Album(AlbumId(c)));
    // a second Shift+click from the same row replaces the range
    shift_click(&mut h, &row(b));
    assert_eq!(shown(&h), vec![a, b]);
    // upwards, from another row
    click(&mut h, &row(d));
    shift_click(&mut h, &row(b));
    assert_eq!(shown(&h), vec![d, c, b]);
    // Cmd+Shift adds a range to what is chosen
    click(&mut h, &row(a));
    cmd_click(&mut h, &row(c));
    ask(&mut h, "ui.clickWidget", json!({"id": row(d), "cmd": true, "shift": true}));
    assert_eq!(shown(&h), vec![a, c, d]);
    // a plain click on one of several chosen: that one alone
    click(&mut h, &row(c));
    assert_eq!(shown(&h), vec![c]);
}

#[test]
fn shift_click_does_not_take_albums_inside_a_closed_folder() {
    let mut h = app("info");
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(2).map(|p| p.0).collect();
    // folders are listed first: Sel E (open, with Sel A), Sel Folder (closed, with Sel Inside), then Sel Z
    let outer = run(&mut h, "album.create", json!({"name": "Sel E", "folder": true}))["id"].as_u64().unwrap();
    let before = album(&mut h, "Sel A", Some(outer), &[ids[0]]);
    let folder = run(&mut h, "album.create", json!({"name": "Sel Folder", "folder": true}))["id"].as_u64().unwrap();
    let inside = album(&mut h, "Sel Inside", Some(folder), &[ids[0]]);
    let after = album(&mut h, "Sel Z", None, &[ids[1]]);
    assert!(has(&h, &row(inside)), "a folder starts open");
    click(&mut h, &format!("albumToggle:{folder}"));
    assert!(!has(&h, &row(inside)), "closed");
    click(&mut h, &row(before));
    shift_click(&mut h, &row(after));
    let chosen = shown(&h);
    assert!(chosen.contains(&before) && chosen.contains(&after), "{chosen:?}");
    assert!(!chosen.contains(&inside), "a row that is not in sight is not taken: {chosen:?}");
}

#[test]
fn the_menu_of_several_albums_deletes_them_after_asking_as_one_step() {
    let mut h = app("info");
    let ([a, b, c, d], _) = four_albums(&mut h);
    click(&mut h, &row(a));
    cmd_click(&mut h, &row(c));
    // the menu of an album that is not chosen is its own, and the chosen ones stay
    right_click(&mut h, &row(d));
    assert!(has(&h, &format!("albumDelete:{d}")), "one album's menu");
    assert!(!has(&h, "sideMenu:albums:delete"));
    assert_eq!(shown(&h), vec![a, c]);
    ask(&mut h, "ui.key", json!({"key": "Escape"}));
    // the menu of one of them: what can be done to both
    right_click(&mut h, &row(c));
    assert!(has(&h, "sideMenu:albums:delete") && has(&h, "sideMenu:albums:export"));
    assert!(!has(&h, &format!("albumDelete:{c}")), "not the single album's Delete");
    let undo = h.app.session.undo.len();
    click(&mut h, "sideMenu:albums:delete");
    assert_eq!(h.app.ui.dialog, Some(Dialog::Several(Several::DeleteAlbums { ids: vec![a, c] })));
    ask(&mut h, "ui.dialog.cancel", json!({}));
    let exists = |h: &Headless, id: u64| h.app.session.catalog.album(AlbumId(id)).is_some();
    assert!(exists(&h, a) && exists(&h, c), "Cancel keeps them");
    right_click(&mut h, &row(c));
    click(&mut h, "sideMenu:albums:delete");
    click(&mut h, "button:dialogOk");
    assert!(!exists(&h, a) && !exists(&h, c) && exists(&h, b) && exists(&h, d));
    assert_eq!(h.app.session.undo.len(), undo + 1, "one step");
    assert_eq!(h.app.session.source, LibrarySource::All, "nothing of what was shown is left");
    run(&mut h, "edit.undo", json!({}));
    assert!(exists(&h, a) && exists(&h, c));
}

#[test]
fn dragging_one_of_several_albums_onto_a_folder_moves_them_all() {
    let mut h = app("info");
    let ([a, b, c, _], _) = four_albums(&mut h);
    let folder = run(&mut h, "album.create", json!({"name": "Sel Folder", "folder": true}))["id"].as_u64().unwrap();
    let parent = |h: &Headless, id: u64| h.app.session.catalog.album(AlbumId(id)).unwrap().parent.map(|p| p.0);
    click(&mut h, &row(a));
    cmd_click(&mut h, &row(c));
    let undo = h.app.session.undo.len();
    drag(&mut h, &row(c), &format!("source:folder:{folder}"));
    assert_eq!((parent(&h, a), parent(&h, c), parent(&h, b)), (Some(folder), Some(folder), None));
    assert_eq!(h.app.session.undo.len(), undo + 1, "one step");
    assert_eq!(shown(&h), vec![a, c], "still the ones shown");
    // an album that is not one of them goes alone
    drag(&mut h, &row(b), &format!("source:folder:{folder}"));
    assert_eq!(parent(&h, b), Some(folder));
    run(&mut h, "edit.undo", json!({}));
    run(&mut h, "edit.undo", json!({}));
    assert_eq!((parent(&h, a), parent(&h, c)), (None, None));
}

#[test]
fn cmd_click_filters_by_any_of_several_keywords() {
    let mut h = app("info");
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(3).map(|p| p.0).collect();
    run(&mut h, "library.select", json!({"ids": [ids[0]]}));
    run(&mut h, "photo.setMeta", json!({"ids": [ids[0]], "addKeywords": ["Sel One"]}));
    run(&mut h, "photo.setMeta", json!({"ids": [ids[1]], "addKeywords": ["Sel Two"]}));
    run(&mut h, "photo.setMeta", json!({"ids": [ids[2]], "addKeywords": ["Sel Three"]}));
    click(&mut h, "source:keyword:Sel One");
    assert_eq!(h.app.session.filter.keyword.as_deref(), Some("Sel One"), "a plain click is what it was");
    cmd_click(&mut h, "source:keyword:Sel Two");
    assert_eq!(h.app.session.filter.keywords, vec!["Sel One".to_string(), "Sel Two".to_string()]);
    assert_eq!(h.app.session.filter.keyword, None);
    assert_eq!(visible(&mut h), {
        let mut v = vec![ids[0], ids[1]];
        v.sort_unstable();
        v
    });
    assert!(has(&h, "highlight:keyword:Sel One") && has(&h, "highlight:keyword:Sel Two"));
    // the menu of one of them offers what can be done to both
    right_click(&mut h, "source:keyword:Sel Two");
    assert!(has(&h, "sideMenu:keywords:delete") && has(&h, "sideMenu:keywords:merge"));
    ask(&mut h, "ui.key", json!({"key": "Escape"}));
    // a plain click: that keyword alone; again: every photo
    click(&mut h, "source:keyword:Sel Two");
    assert_eq!((h.app.session.filter.keyword.as_deref(), h.app.session.filter.keywords.len()), (Some("Sel Two"), 0));
    click(&mut h, "source:keyword:Sel Two");
    assert_eq!(h.app.session.filter, lightcraft_catalog::Filter::default());
}

#[test]
fn cmd_click_filters_by_any_of_several_dates() {
    let mut h = app("info");
    // the months of the demo library's first year
    let year: String = h.app.widgets.iter().find_map(|(w, _)| w.strip_prefix("source:date:").map(str::to_string)).expect("a year");
    click(&mut h, &format!("dateToggle:{year}"));
    let years: Vec<String> =
        h.app.widgets.iter().filter_map(|(w, _)| w.strip_prefix("source:date:").filter(|k| k.len() == 7).map(str::to_string)).collect();
    assert!(years.len() >= 2, "the demo library spans months: {years:?}");
    let all = h.app.session.visible().len();
    click(&mut h, &format!("source:date:{}", years[0]));
    let first = h.app.session.visible().len();
    cmd_click(&mut h, &format!("source:date:{}", years[1]));
    assert_eq!(h.app.session.filter.dates, vec![years[0].clone(), years[1].clone()]);
    let both = h.app.session.visible().len();
    assert!(both > first && both <= all, "{first} then {both} of {all}");
    cmd_click(&mut h, &format!("source:date:{}", years[0]));
    assert_eq!((h.app.session.filter.date.as_deref(), h.app.session.filter.dates.len()), (Some(years[1].as_str()), 0));
}

/// A library of file-backed photos that exist only in the catalog.
fn folders_app(paths: &[&str]) -> Headless {
    use lightcraft_catalog::{Op, Photo, Source};
    let mut session = lightcraft_engine::Session::new();
    for path in paths {
        let id = session.catalog.alloc_photo_id();
        let p = Photo::new(id, Source::File { path: (*path).into() }, "x.jpg", "JPEG", 60, 40, "2026-01-01T10:00:00");
        session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    let app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1500.0, 2000.0], 1.0);
    ask(&mut h, "ui.set", json!({"view": "photoGrid", "leftPanel": true}));
    h
}

#[test]
fn cmd_click_shows_several_folders_and_the_menu_removes_them_after_asking() {
    let mut h = folders_app(&["/pics/trip/a.jpg", "/pics/trip/b.jpg", "/pics/home/c.jpg", "/pics/work/d.jpg"]);
    click(&mut h, "libraryFolderToggle:/pics");
    click(&mut h, "source:libfolder:/pics/trip");
    cmd_click(&mut h, "source:libfolder:/pics/work");
    assert_eq!(h.app.session.source_library_folders(), vec!["/pics/trip".to_string(), "/pics/work".to_string()]);
    assert_eq!(h.app.session.visible().len(), 3);
    assert!(has(&h, "highlight:libfolder:/pics/trip") && has(&h, "highlight:libfolder:/pics/work"));
    right_click(&mut h, "source:libfolder:/pics/work");
    assert!(has(&h, "sideMenu:folders:remove"));
    click(&mut h, "sideMenu:folders:remove");
    assert_eq!(
        h.app.ui.dialog,
        Some(Dialog::Several(Several::RemoveFolders { paths: vec!["/pics/trip".to_string(), "/pics/work".to_string()], count: 3 }))
    );
    let undo = h.app.session.undo.len();
    click(&mut h, "button:dialogOk");
    assert_eq!(h.app.session.undo.len(), undo + 1);
    assert_eq!(h.app.session.source, LibrarySource::All);
    assert_eq!(h.app.session.visible().len(), 1, "the photo of the folder that stayed");
    // Shift+click takes the folders between, as drawn
    run(&mut h, "edit.undo", json!({}));
    let rows: Vec<String> = h.app.widgets.iter().filter_map(|(w, _)| w.strip_prefix("source:libfolder:/pics/").map(str::to_string)).collect();
    assert_eq!(rows.len(), 3, "{rows:?}");
    click(&mut h, &format!("source:libfolder:/pics/{}", rows[0]));
    shift_click(&mut h, &format!("source:libfolder:/pics/{}", rows[2]));
    assert_eq!(h.app.session.source_library_folders().len(), 3);
}

/// The Keyword List with three keywords of its own on the first photos, at the top level:
/// `Sel K1`, `Sel K2`, `Sel K3`, and an empty `Sel Parent`.
fn keyword_list() -> (Headless, Vec<u64>) {
    let mut h = app("keywords");
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(3).map(|p| p.0).collect();
    run(&mut h, "library.select", json!({"ids": [ids[0]]}));
    run(&mut h, "photo.setMeta", json!({"ids": [ids[0]], "addKeywords": ["Sel K1", "Sel K2"]}));
    run(&mut h, "photo.setMeta", json!({"ids": [ids[1]], "addKeywords": ["Sel K2", "Sel K3"]}));
    run(&mut h, "keyword.create", json!({"name": "Sel Parent"}));
    // only these in the list: every row is on screen
    ask(&mut h, "ui.clickWidget", json!({"id": "field:keywordFilter"}));
    ask(&mut h, "ui.text", json!({"text": "Sel "}));
    assert!(has(&h, "keywordRow:Sel K1") && has(&h, "keywordRow:Sel Parent"));
    (h, ids)
}

fn picked(h: &Headless) -> Vec<String> {
    crate::panels::side_select::list_picked(&h.app)
}

fn keywords_of(h: &Headless, id: u64) -> Vec<String> {
    let mut k: Vec<String> = h.app.session.catalog.photo(PhotoId(id)).unwrap().meta.keywords.iter().filter(|k| k.contains("Sel")).cloned().collect();
    k.sort();
    k
}

#[test]
fn the_keyword_list_picks_several_with_cmd_and_shift() {
    let (mut h, _) = keyword_list();
    click(&mut h, "keywordRow:Sel K1");
    assert_eq!(picked(&h), vec!["Sel K1"]);
    cmd_click(&mut h, "keywordRow:Sel K3");
    assert_eq!(picked(&h), vec!["Sel K1", "Sel K3"]);
    assert_eq!(h.app.ui.keyword_list_selected.as_deref(), Some("Sel K3"), "the last picked is the one Edit takes");
    assert!(has(&h, "keywordPicked:Sel K1") && has(&h, "keywordPicked:Sel K3") && !has(&h, "keywordPicked:Sel K2"));
    cmd_click(&mut h, "keywordRow:Sel K1");
    assert_eq!(picked(&h), vec!["Sel K3"]);
    click(&mut h, "keywordRow:Sel K1");
    shift_click(&mut h, "keywordRow:Sel K3");
    assert_eq!(picked(&h), vec!["Sel K1", "Sel K2", "Sel K3"]);
    // picking never changes what the grid shows
    assert_eq!(h.app.session.filter, lightcraft_catalog::Filter::default());
}

#[test]
fn the_keyword_list_deletes_several_after_one_question() {
    let (mut h, ids) = keyword_list();
    click(&mut h, "keywordRow:Sel K1");
    cmd_click(&mut h, "keywordRow:Sel K2");
    let undo = h.app.session.undo.len();
    click(&mut h, "keywordList:delete");
    assert_eq!(
        h.app.ui.dialog,
        Some(Dialog::Several(Several::DeleteKeywords { keywords: vec!["Sel K1".to_string(), "Sel K2".to_string()], count: 2 }))
    );
    ask(&mut h, "ui.dialog.cancel", json!({}));
    assert!(h.app.session.catalog.has_keyword("Sel K1"), "Cancel kept them");
    // from the menu of one of them, the same question
    right_click(&mut h, "keywordRow:Sel K2");
    assert!(has(&h, "sideMenu:keywords:delete") && !has(&h, "keywordMenu:delete"), "the menu for several");
    click(&mut h, "sideMenu:keywords:delete");
    click(&mut h, "button:dialogOk");
    assert!(!h.app.session.catalog.has_keyword("Sel K1") && !h.app.session.catalog.has_keyword("Sel K2"));
    assert!(h.app.session.catalog.has_keyword("Sel K3"));
    assert_eq!(h.app.session.undo.len(), undo + 1, "one step");
    assert!(picked(&h).is_empty(), "nothing that is gone stays picked");
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(keywords_of(&h, ids[0]), vec!["Sel K1", "Sel K2"]);
}

#[test]
fn the_menu_of_several_keywords_gives_them_all_to_the_selected_photos() {
    let (mut h, ids) = keyword_list();
    run(&mut h, "library.select", json!({"ids": [ids[2]]}));
    click(&mut h, "keywordRow:Sel K1");
    cmd_click(&mut h, "keywordRow:Sel K3");
    // the menu of a keyword that is not picked is its own
    right_click(&mut h, "keywordRow:Sel K2");
    assert!(has(&h, "keywordMenu:delete") && !has(&h, "sideMenu:keywords:add"));
    assert_eq!(picked(&h), vec!["Sel K1", "Sel K3"], "the picked ones stay picked");
    ask(&mut h, "ui.key", json!({"key": "Escape"}));
    let undo = h.app.session.undo.len();
    right_click(&mut h, "keywordRow:Sel K3");
    click(&mut h, "sideMenu:keywords:add");
    assert_eq!(keywords_of(&h, ids[2]), vec!["Sel K1", "Sel K3"]);
    assert_eq!(h.app.session.undo.len(), undo + 1, "one step");
    // …and shows the photos with any of them
    right_click(&mut h, "keywordRow:Sel K3");
    click(&mut h, "sideMenu:keywords:show");
    assert_eq!(h.app.session.filter.keywords, vec!["Sel K1".to_string(), "Sel K3".to_string()]);
}

#[test]
fn dragging_one_of_several_keywords_nests_them_all() {
    let (mut h, ids) = keyword_list();
    click(&mut h, "keywordRow:Sel K1");
    cmd_click(&mut h, "keywordRow:Sel K3");
    let undo = h.app.session.undo.len();
    drag(&mut h, "keywordRow:Sel K3", "keywordRow:Sel Parent");
    assert_eq!(keywords_of(&h, ids[0]), vec!["Sel K2", "Sel Parent|Sel K1"]);
    assert_eq!(keywords_of(&h, ids[1]), vec!["Sel K2", "Sel Parent|Sel K3"]);
    assert_eq!(h.app.session.undo.len(), undo + 1, "one step");
    assert_eq!(picked(&h), vec!["Sel Parent|Sel K1", "Sel Parent|Sel K3"], "still picked, where they are now");
    assert!(h.app.ui.dragging_keyword.is_none(), "the drag ended");
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(keywords_of(&h, ids[0]), vec!["Sel K1", "Sel K2"]);
}
