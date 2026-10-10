//! The view never points at something that is gone. What the session shows and acts on (the
//! source, the album a filter names, the target album, the selection) refers to albums and photos
//! of the library; after anything changes the library, by whatever way, those references are
//! checked in one place (`Session::reconcile_view`).
//!
//! Scenarios:
//! - Given an album, a smart album or a folder being shown, when it goes away (deleted, its
//!   creation undone, its deletion redone, the folder around it deleted, or removed by an edit
//!   that is no command), then the grid shows All Photos, titled so.
//! - Given the shown album went away by undo, when another album is made (and takes its id),
//!   then the grid still shows All Photos, not the newcomer.
//! - Given a library closed while showing an album that is gone when it is opened again, then
//!   it opens on All Photos.
//! - Given the filter names an album, or an album is the target, when that album goes away, then
//!   the filter no longer names it and there is no target.
//! - Given photos are selected, when they go away (their import undone), then they are no
//!   longer selected, and the active photo is one that exists.
//! - Given the shown album is only changed (renamed, moved, emptied), then it is still shown.

use lightcraft_catalog::{AlbumId, Op, Photo, PhotoId, Source};
use serde_json::{Value, json};

use crate::{LibrarySource, Session};

fn photos(s: &mut Session, n: u64) {
    for day in 1..=n {
        let id = s.catalog.alloc_photo_id();
        let mut p = Photo::new(id, Source::Demo { scene: 1 }, &format!("p{day}.jpg"), "JPEG", 60, 40, "2026-02-01T10:00:00");
        p.captured = Some(format!("2026-01-{day:02}T10:00:00"));
        s.commit("Add Photo", Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
}

fn run(s: &mut Session, cmd: &str, params: Value) -> Value {
    s.execute(cmd, &params).unwrap()
}

fn show(s: &mut Session, id: u64) {
    run(s, "library.source", json!({"kind": "album", "id": id}));
    assert_eq!(s.source, LibrarySource::Album(AlbumId(id)));
}

fn shows_all(s: &mut Session, why: &str) {
    assert_eq!(s.source, LibrarySource::All, "{why}");
    assert_eq!(s.source.label(&s.catalog), "All Photos", "{why}");
    assert_eq!(s.visible_cloned().len(), 3, "{why}");
}

/// What can be shown: an album, a smart album, a folder of albums.
fn kinds() -> [(&'static str, &'static str, Value); 3] {
    [
        ("album", "album.create", json!({"name": "Shown"})),
        ("smart album", "album.createSmart", json!({"name": "Shown", "rules": {"rating": 3}})),
        ("folder", "album.create", json!({"name": "Shown", "folder": true})),
    ]
}

#[test]
fn a_shown_album_that_goes_away_gives_way_to_all_photos() {
    for (kind, create, params) in kinds() {
        // deleted
        let mut s = Session::new();
        photos(&mut s, 3);
        let id = run(&mut s, create, params.clone())["id"].as_u64().unwrap();
        show(&mut s, id);
        run(&mut s, "album.delete", json!({"id": id}));
        shows_all(&mut s, &format!("{kind} deleted"));
        // back by undo: not shown again by itself, and its deletion redone while shown
        run(&mut s, "edit.undo", json!({}));
        shows_all(&mut s, &format!("{kind} back by undo"));
        show(&mut s, id);
        run(&mut s, "edit.redo", json!({}));
        shows_all(&mut s, &format!("{kind}'s deletion redone"));

        // its creation undone
        let mut s = Session::new();
        photos(&mut s, 3);
        let id = run(&mut s, create, params.clone())["id"].as_u64().unwrap();
        show(&mut s, id);
        run(&mut s, "edit.undo", json!({}));
        assert!(s.catalog.album(AlbumId(id)).is_none());
        shows_all(&mut s, &format!("{kind}'s creation undone"));

        // removed by an edit that is no command (a background task, a merge): seen by the next look
        let mut s = Session::new();
        photos(&mut s, 3);
        let id = run(&mut s, create, params.clone())["id"].as_u64().unwrap();
        show(&mut s, id);
        s.commit("Remove", Op::RemoveAlbum { id: AlbumId(id) }).unwrap();
        assert_eq!(s.visible_cloned().len(), 3, "{kind} removed outside a command");
        shows_all(&mut s, &format!("{kind} removed outside a command"));
    }
}

#[test]
fn deleting_the_folder_around_the_shown_album_gives_way_too() {
    let mut s = Session::new();
    photos(&mut s, 3);
    let folder = run(&mut s, "album.create", json!({"name": "Trips", "folder": true}))["id"].as_u64().unwrap();
    let inner = run(&mut s, "album.create", json!({"name": "Rome", "parent": folder}))["id"].as_u64().unwrap();
    show(&mut s, inner);
    run(&mut s, "album.delete", json!({"id": folder}));
    shows_all(&mut s, "the folder around it deleted");
}

#[test]
fn a_new_album_doesnt_take_the_place_of_one_that_went_away() {
    let mut s = Session::new();
    photos(&mut s, 3);
    let id = run(&mut s, "album.create", json!({"name": "First"}))["id"].as_u64().unwrap();
    show(&mut s, id);
    run(&mut s, "album.setTarget", json!({"id": id}));
    run(&mut s, "library.filter", json!({"album": id}));
    // the target and the filter are no undo steps: this undoes the album
    run(&mut s, "edit.undo", json!({}));
    assert!(s.catalog.album(AlbumId(id)).is_none(), "the album's creation was undone");
    let second = run(&mut s, "album.create", json!({"name": "Second"}))["id"].as_u64().unwrap();
    shows_all(&mut s, "another album was made");
    assert_eq!(s.target_album, None, "Second (id {second}) is no target because First (id {id}) was");
    assert_eq!(s.filter.album, None);
}

#[test]
fn a_filter_and_a_target_let_go_of_an_album_that_went_away() {
    let mut s = Session::new();
    photos(&mut s, 3);
    let id = run(&mut s, "album.create", json!({"name": "Named"}))["id"].as_u64().unwrap();
    let other = run(&mut s, "album.create", json!({"name": "Other"}))["id"].as_u64().unwrap();
    run(&mut s, "album.setTarget", json!({"id": id}));
    run(&mut s, "library.filter", json!({"album": id, "rating": 0}));
    assert_eq!((s.target_album, s.filter.album), (Some(AlbumId(id)), Some(AlbumId(id))));
    // another album going away changes nothing
    run(&mut s, "album.delete", json!({"id": other}));
    assert_eq!((s.target_album, s.filter.album), (Some(AlbumId(id)), Some(AlbumId(id))));
    run(&mut s, "album.delete", json!({"id": id}));
    assert_eq!((s.target_album, s.filter.album), (None, None));
    assert_eq!(s.visible_cloned().len(), 3, "the filter hides nothing behind an album that is gone");
}

#[test]
fn photos_that_went_away_are_no_longer_selected() {
    let mut s = Session::new();
    photos(&mut s, 3);
    s.selection.ids = vec![PhotoId(1), PhotoId(3)];
    s.selection.active = Some(PhotoId(3));
    // the last photo's import undone
    run(&mut s, "edit.undo", json!({}));
    assert!(s.catalog.photo(PhotoId(3)).is_none());
    assert_eq!(s.selection.ids, [PhotoId(1)]);
    assert_eq!(s.selection.active, Some(PhotoId(1)), "the active photo is one that is there");
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.selection.ids.is_empty() && s.selection.active.is_none());
}

#[test]
fn a_shown_album_that_only_changes_stays_shown() {
    let mut s = Session::new();
    photos(&mut s, 3);
    let folder = run(&mut s, "album.create", json!({"name": "Trips", "folder": true}))["id"].as_u64().unwrap();
    let id = run(&mut s, "album.create", json!({"name": "Rome"}))["id"].as_u64().unwrap();
    show(&mut s, id);
    s.selection.ids = vec![PhotoId(1)];
    run(&mut s, "album.addPhotos", json!({"id": id, "ids": [1]}));
    run(&mut s, "album.rename", json!({"id": id, "name": "Roma"}));
    run(&mut s, "album.move", json!({"id": id, "parent": folder}));
    run(&mut s, "album.removePhotos", json!({"id": id, "ids": [1]}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.source, LibrarySource::Album(AlbumId(id)));
    assert_eq!(s.source.label(&s.catalog), "Roma");
    assert_eq!(s.selection.ids, [PhotoId(1)]);
}

#[test]
fn a_library_opens_on_all_photos_when_its_last_album_is_gone() {
    /// Gone with the test, however it ends.
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch = Scratch(std::env::temp_dir().join(format!("lc-view-reconcile-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    let mut s = Session::new();
    s.open_library(&scratch.0, false).unwrap();
    photos(&mut s, 3);
    let id = run(&mut s, "album.create", json!({"name": "Shown"}))["id"].as_u64().unwrap();
    show(&mut s, id);
    s.save_view();
    // the view file still names the album; the library no longer has it
    s.source = LibrarySource::All;
    run(&mut s, "album.delete", json!({"id": id}));
    let view = scratch.0.join("view.json");
    let saved = std::fs::read_to_string(&view).unwrap();
    assert!(saved.contains("album"), "{saved}");
    drop(s);
    std::fs::write(&view, saved).unwrap();
    let mut back = Session::new();
    back.open_library(&scratch.0, false).unwrap();
    assert!(back.catalog.album(AlbumId(id)).is_none());
    shows_all(&mut back, "opened with a view of an album that is gone");
}
