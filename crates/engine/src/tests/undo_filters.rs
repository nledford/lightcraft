//! Undo and redo carry what a step did to the person's filter: a keyword deleted or renamed, a person renamed, a
//! library folder removed. Nothing in those commands records it; `run_command` compares the filter before and after
//! any command that committed a step.
//!
//! * Given a filter that names a keyword, person or folder the step took out or renamed, when the step is undone, then
//!   the filter names it again (put aside or in use, wherever the filter is now), and redo carries it forward again.
//! * Given the filter was changed after the step, when it is undone, then only what the step changed comes back and
//!   nothing the person did since is overwritten.
//! * Given the person threw that filter away after the step and built another one, when the step is undone, then the
//!   new filter is left alone.

use lightcraft_catalog::{Catalog, Filter, Op, Photo, PhotoId, Source};
use lightcraft_geom::Rect;
use lightcraft_meta::{Region, RegionKind};
use serde_json::{Value, json};

use crate::Session;

/// Photos in two folders; the ones in /pics/trip carry the keyword "cat" and the one in /pics/home "dog".
fn session() -> Session {
    let mut s = Session::new();
    for (path, keyword) in [("/pics/trip/a.jpg", "cat"), ("/pics/trip/b.jpg", "cat"), ("/pics/home/c.jpg", "dog")] {
        let id = s.catalog.alloc_photo_id();
        let mut p = Photo::new(id, Source::File { path: path.into() }, "x.jpg", "JPEG", 60, 40, "2026-01-01T10:00:00");
        p.meta.keywords = vec![keyword.into()];
        s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    s
}

fn run(s: &mut Session, command: &str, params: Value) {
    s.execute(command, &params).unwrap();
}

fn delete_cat(s: &mut Session) {
    run(s, "keyword.delete", json!({"keyword": "cat"}));
}

fn remove_trip(s: &mut Session) {
    run(s, "library.removeFolder", json!({"path": "/pics/trip"}));
}

fn undo(s: &mut Session) {
    run(s, "edit.undo", json!({}));
}

fn redo(s: &mut Session) {
    run(s, "edit.redo", json!({}));
}

fn toggle(s: &mut Session) {
    run(s, "library.toggleFilters", json!({}));
}

fn filtered(keyword: Option<&str>, folder: Option<&str>, rating: u8) -> Filter {
    Filter { keyword: keyword.map(Into::into), library_folder: folder.map(Into::into), rating, ..Default::default() }
}

#[test]
fn undoing_a_keyword_delete_brings_the_live_filters_keyword_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat"}));
    delete_cat(&mut s);
    assert_eq!(s.filter().keyword, None, "the deletion takes the keyword out of the filter");
    undo(&mut s);
    assert_eq!(s.filter().keyword.as_deref(), Some("cat"), "undo puts the keyword back in the filter");
    assert_eq!(s.visible().len(), 2);
    redo(&mut s);
    assert_eq!(s.filter().keyword, None, "redo takes it out again");
    assert_eq!(s.visible().len(), 3);
}

#[test]
fn undoing_a_keyword_delete_brings_the_put_aside_filters_keyword_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 0}));
    toggle(&mut s);
    delete_cat(&mut s);
    undo(&mut s);
    assert!(s.filters_off(), "the filter is still put aside, and Enable Filters has something to bring back");
    toggle(&mut s);
    assert_eq!(*s.filter(), filtered(Some("cat"), None, 0));
    assert_eq!(s.visible().len(), 2);
}

#[test]
fn undoing_a_keyword_rename_brings_the_filters_old_keyword_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 0}));
    run(&mut s, "keyword.rename", json!({"from": "cat", "to": "kitten"}));
    assert_eq!(s.filter().keyword.as_deref(), Some("kitten"), "the filter follows the rename");
    undo(&mut s);
    assert_eq!(s.filter().keyword.as_deref(), Some("cat"), "undo gives the photos their old keyword, so the filter too");
    assert_eq!(s.visible().len(), 2);
    redo(&mut s);
    assert_eq!(s.filter().keyword.as_deref(), Some("kitten"));
    assert_eq!(s.visible().len(), 2);
}

#[test]
fn undoing_a_keyword_rename_that_merged_two_keywords_brings_the_filters_old_keyword_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat"}));
    run(&mut s, "keyword.rename", json!({"from": "cat", "to": "dog"}));
    assert_eq!(s.filter().keyword.as_deref(), Some("dog"));
    assert_eq!(s.visible().len(), 3, "the merged keyword is on every photo");
    undo(&mut s);
    assert_eq!(s.filter().keyword.as_deref(), Some("cat"));
    assert_eq!(s.visible().len(), 2);
}

#[test]
fn undoing_a_keyword_rename_follows_the_filter_put_aside_since() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 3}));
    run(&mut s, "keyword.rename", json!({"from": "cat", "to": "kitten"}));
    toggle(&mut s);
    undo(&mut s);
    assert!(s.filters_off());
    toggle(&mut s);
    assert_eq!(*s.filter(), filtered(Some("cat"), None, 3), "the put-aside filter has the old keyword again, and its stars");
}

#[test]
fn undoing_a_person_rename_brings_the_filters_old_name_back() {
    let mut s = session();
    let id = s.catalog.photos().map(|p| p.id).min().unwrap();
    let mut meta = s.catalog.photo(id).unwrap().meta.clone();
    let rect = Rect { x0: 0.1, y0: 0.2, x1: 0.35, y1: 0.6 };
    meta.regions = vec![Region { rect, kind: RegionKind::Face, name: Some("Ann".into()), description: None }];
    s.commit("setup", Op::SetMeta { id, meta: Box::new(meta) }).unwrap();
    run(&mut s, "library.filter", json!({"person": "Ann"}));
    assert_eq!(s.visible().len(), 1);
    run(&mut s, "faces.renamePerson", json!({"from": "Ann", "to": "Ada"}));
    assert_eq!(s.filter().person.as_deref(), Some("Ada"), "the filter follows the rename");
    undo(&mut s);
    assert_eq!(s.filter().person.as_deref(), Some("Ann"), "no empty grid under a name nobody has");
    assert_eq!(s.visible().len(), 1);
    redo(&mut s);
    assert_eq!(s.filter().person.as_deref(), Some("Ada"));
    assert_eq!(s.visible().len(), 1);
}

#[test]
fn undoing_a_person_rename_shows_the_one_photo_it_changed_under_the_filter_it_gives_back() {
    // one photo has the person: undo shows it (makes it the active photo) when it is in view, and it is in view
    // under the old name the filter gets back, not under the new one the filter still has while the catalog is undone
    let mut s = session();
    let mut ids: Vec<PhotoId> = s.catalog.photos().map(|p| p.id).collect();
    ids.sort();
    let mut meta = s.catalog.photo(ids[0]).unwrap().meta.clone();
    let rect = Rect { x0: 0.1, y0: 0.2, x1: 0.35, y1: 0.6 };
    meta.regions = vec![Region { rect, kind: RegionKind::Face, name: Some("Ann".into()), description: None }];
    s.commit("setup", Op::SetMeta { id: ids[0], meta: Box::new(meta) }).unwrap();
    run(&mut s, "library.filter", json!({"person": "Ann"}));
    run(&mut s, "faces.renamePerson", json!({"from": "Ann", "to": "Ada"}));
    s.selection = crate::Selection::single(ids[1]);
    undo(&mut s);
    assert_eq!(s.filter().person.as_deref(), Some("Ann"));
    assert_eq!(s.selection.active, Some(ids[0]), "the photo the undo changed is on screen");
    s.selection = crate::Selection::single(ids[1]);
    redo(&mut s);
    assert_eq!(s.selection.active, Some(ids[0]), "and so it is for redo");
}

#[test]
fn undoing_a_step_keeps_what_the_filter_got_since() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat"}));
    delete_cat(&mut s);
    run(&mut s, "library.filter", json!({"rating": 3}));
    undo(&mut s);
    assert_eq!(*s.filter(), filtered(Some("cat"), None, 3), "the stars stay, the keyword returns");
    // a keyword the person chose after the delete is not replaced
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat"}));
    delete_cat(&mut s);
    run(&mut s, "library.filter", json!({"keyword": "dog"}));
    undo(&mut s);
    assert_eq!(s.filter().keyword.as_deref(), Some("dog"));
}

#[test]
fn undoing_a_step_leaves_a_filter_built_after_the_old_one_was_put_aside_alone() {
    // the keyword the step took out of the put-aside filter does not join the filter built since: setting a filter while
    // filters are off forgets the one put aside (`library.toggleFilters`)
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat"}));
    toggle(&mut s);
    delete_cat(&mut s);
    run(&mut s, "library.filter", json!({"rating": 3}));
    undo(&mut s);
    assert!(!s.filters_off(), "the filter the person set after the delete stays in use");
    assert_eq!(*s.filter(), filtered(None, None, 3), "and nothing joins it");
    // the same for a folder
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip"}));
    toggle(&mut s);
    remove_trip(&mut s);
    run(&mut s, "library.filter", json!({"rating": 3}));
    undo(&mut s);
    assert!(!s.filters_off());
    assert_eq!(*s.filter(), filtered(None, None, 3));
    // redo leaves it alone too
    redo(&mut s);
    assert_eq!(*s.filter(), filtered(None, None, 3));
}

#[test]
fn undoing_a_step_leaves_a_filter_built_after_the_old_one_was_emptied_or_replaced_alone() {
    // taking the last criterion off by hand ends the filter
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 3}));
    delete_cat(&mut s);
    run(&mut s, "library.filter", json!({"rating": 0}));
    run(&mut s, "library.filter", json!({"rating": 4}));
    undo(&mut s);
    assert_eq!(*s.filter(), filtered(None, None, 4));
    // so does Clear Filters
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 3}));
    delete_cat(&mut s);
    run(&mut s, "library.clearFilter", json!({}));
    run(&mut s, "library.filter", json!({"rating": 4}));
    undo(&mut s);
    assert_eq!(*s.filter(), filtered(None, None, 4));
    // and a saved preset replaces it
    let mut s = session();
    run(&mut s, "library.filter", json!({"rating": 5}));
    run(&mut s, "filter.savePreset", json!({"name": "five"}));
    run(&mut s, "library.filter", json!({"rating": 0, "keyword": "cat"}));
    delete_cat(&mut s);
    run(&mut s, "filter.applyPreset", json!({"name": "five"}));
    undo(&mut s);
    assert_eq!(*s.filter(), filtered(None, None, 5), "the keyword does not join the preset's filter");
}

#[test]
fn undoing_a_step_follows_the_filter_to_where_it_is_now() {
    // put aside after the delete: the keyword goes back into the put-aside filter, and the live filter stays empty
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 4}));
    delete_cat(&mut s);
    toggle(&mut s);
    undo(&mut s);
    assert!(s.filters_off());
    assert_eq!(*s.filter(), Filter::default(), "nothing is filtered while the filter is put aside");
    toggle(&mut s);
    assert_eq!(*s.filter(), filtered(Some("cat"), None, 4));
    // brought back after the delete: the keyword goes back into the filter in use
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat", "rating": 4}));
    toggle(&mut s);
    delete_cat(&mut s);
    toggle(&mut s);
    undo(&mut s);
    assert!(!s.filters_off());
    assert_eq!(*s.filter(), filtered(Some("cat"), None, 4));
    // and redo after the filter was put aside takes it out of the put-aside filter
    toggle(&mut s);
    redo(&mut s);
    assert!(s.filters_off());
    toggle(&mut s);
    assert_eq!(*s.filter(), filtered(None, None, 4));
}

#[test]
fn undoing_a_folder_removal_brings_the_live_filters_folder_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip"}));
    remove_trip(&mut s);
    assert_eq!(s.filter().library_folder, None);
    undo(&mut s);
    assert_eq!(s.filter().library_folder.as_deref(), Some("/pics/trip"));
    assert_eq!(s.visible().len(), 2);
    redo(&mut s);
    assert_eq!(s.filter().library_folder, None, "redo takes it out again");
}

#[test]
fn undoing_a_folder_removal_brings_the_put_aside_filters_folder_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip", "rating": 4}));
    toggle(&mut s);
    remove_trip(&mut s);
    undo(&mut s);
    toggle(&mut s);
    assert_eq!(*s.filter(), filtered(None, Some("/pics/trip"), 4));
}

#[test]
fn undoing_a_folder_removal_that_dropped_the_put_aside_filter_brings_it_back() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip"}));
    toggle(&mut s);
    remove_trip(&mut s);
    assert!(!s.filters_off(), "the emptied put-aside filter is dropped");
    undo(&mut s);
    assert!(s.filters_off(), "undo puts it aside again");
    toggle(&mut s);
    assert_eq!(s.filter().library_folder.as_deref(), Some("/pics/trip"));
    assert_eq!(s.visible().len(), 2);
}

#[test]
fn redoing_a_folder_removal_drops_the_put_aside_filter_it_emptied_again() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip"}));
    toggle(&mut s);
    remove_trip(&mut s);
    undo(&mut s);
    redo(&mut s);
    assert!(!s.filters_off(), "nothing is left to bring back");
    assert_eq!(*s.filter(), Filter::default());
}

#[test]
fn undoing_a_folder_removal_does_not_put_an_empty_filter_aside_when_another_folder_is_shown() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip"}));
    toggle(&mut s);
    remove_trip(&mut s);
    run(&mut s, "library.source", json!({"kind": "libraryFolder", "path": "/pics/home"}));
    undo(&mut s);
    assert!(!s.filters_off(), "the folder shown wins, so nothing is left to put aside");
    assert_eq!(*s.filter(), Filter::default());
}

#[test]
fn undoing_a_folder_removal_does_not_replace_the_folder_shown_since() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"libraryFolder": "/pics/trip"}));
    remove_trip(&mut s);
    run(&mut s, "library.source", json!({"kind": "libraryFolder", "path": "/pics/home"}));
    undo(&mut s);
    assert_eq!(s.filter().library_folder, None, "one folder at a time: the one shown wins");
    run(&mut s, "library.filter", json!({"rating": 3}));
    assert_eq!(*s.filter(), filtered(None, None, 3), "and filtering goes on, as no folder filter is refused");
}

#[test]
fn undoing_two_steps_puts_an_emptied_put_aside_filter_back_as_it_was() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"keyword": "cat"}));
    toggle(&mut s);
    delete_cat(&mut s);
    remove_trip(&mut s);
    assert!(!s.filters_off(), "the removal dropped the put-aside filter the delete had emptied");
    undo(&mut s);
    undo(&mut s);
    assert!(s.filters_off(), "filtering is still off, as before the two steps");
    assert_eq!(*s.filter(), Filter::default());
    toggle(&mut s);
    assert_eq!(s.filter().keyword.as_deref(), Some("cat"));
}

#[test]
fn a_step_that_never_touched_the_filter_leaves_it_alone_on_undo() {
    let mut s = session();
    run(&mut s, "library.filter", json!({"rating": 2}));
    remove_trip(&mut s);
    undo(&mut s);
    assert_eq!(*s.filter(), filtered(None, None, 2));
    assert!(!s.filters_off());
    assert!(s.undo.iter().chain(s.redo.iter()).all(|e| e.filters.is_none()), "no filter change is kept for it");
}

/// The fields one chip clears move together, or not at all: `only` and `onlyNone` are one constraint.
#[test]
fn a_step_carries_the_fields_of_a_chip_together() {
    let cat = Catalog::new();
    let from = Filter { only: vec![PhotoId(1)], only_none: false, ..Default::default() };
    let to = Filter { only: Vec::new(), only_none: true, ..Default::default() };
    let mut now = from.clone();
    crate::undo_filters::carry(&mut now, &from, &to, &cat);
    assert_eq!(now, to, "both fields move");
    let mut half = Filter { only: vec![PhotoId(1)], only_none: true, ..Default::default() };
    crate::undo_filters::carry(&mut half, &from, &to, &cat);
    assert_eq!(half, Filter { only: vec![PhotoId(1)], only_none: true, ..Default::default() }, "a chip changed since moves not at all");
    // what the step left alone is never touched, whatever it holds now
    let mut other = Filter { only: vec![PhotoId(1)], rating: 5, text: "x".into(), ..Default::default() };
    crate::undo_filters::carry(&mut other, &from, &to, &cat);
    assert_eq!((other.only_none, other.rating, other.text.as_str()), (true, 5, "x"));
}
