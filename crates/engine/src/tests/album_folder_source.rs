//! A folder of albums as a source (`library.source {kind: "album", id: <folder>}`), as a
//! collection set is in Lightroom Classic: the grid shows the photos of every album and smart
//! album inside it, each once.
//!
//! Scenarios:
//! - Given a folder whose albums share a photo, when the folder is shown, then the grid lists
//!   their photos once each, is titled with the folder's name and counts them.
//! - Given an inner folder and a smart album, then theirs are shown too, and the view follows
//!   what the albums hold.
//! - Given a sort, newest or oldest first, then a folder follows it (it has no order by hand).
//! - Given a filter, then it narrows the folder's photos like any other source's.
//! - When the shown folder is deleted, then the grid goes back to All Photos.
//! - Given a folder shown, when the view is saved as a smart album, then that album shows the
//!   same photos and follows the folder.
//! - A folder still holds no photos of its own: adding to it or removing from it is refused,
//!   and nothing is recorded to undo.
//! - Agents see the folder's count in `albums.list`.
//! - The folder is still the source after the library is closed and opened again.

use lightcraft_catalog::{AlbumId, Op, Photo, PhotoId, Source};
use serde_json::{Value, json};

use crate::{LibrarySource, Session};

/// Photos `1..=n`, one a day from 2026-01-01 (so the newest has the highest id).
fn photos(s: &mut Session, n: u64) -> Vec<u64> {
    (1..=n)
        .map(|day| {
            let id = s.catalog.alloc_photo_id();
            let mut p = Photo::new(id, Source::Demo { scene: 1 }, &format!("p{day}.jpg"), "JPEG", 60, 40, "2026-02-01T10:00:00");
            p.captured = Some(format!("2026-01-{day:02}T10:00:00"));
            // committed, so a library on disk records it
            s.commit("Add Photo", Op::AddPhoto { photo: Box::new(p) }).unwrap();
            id.0
        })
        .collect()
}

fn make(s: &mut Session, params: Value) -> u64 {
    s.execute("album.create", &params).unwrap()["id"].as_u64().unwrap()
}

fn album(s: &mut Session, name: &str, parent: Option<u64>, ids: &[u64]) -> u64 {
    let id = make(s, json!({"name": name, "parent": parent}));
    if !ids.is_empty() {
        // the command wants a selection to be enabled, whatever `ids` says
        s.selection.ids = ids.iter().map(|i| PhotoId(*i)).collect();
        s.execute("album.addPhotos", &json!({"id": id, "ids": ids})).unwrap();
        s.selection.ids.clear();
    }
    id
}

fn rate(s: &mut Session, ids: &[u64], rating: u8) {
    s.selection.ids = ids.iter().map(|i| PhotoId(*i)).collect();
    s.execute("photo.rate", &json!({"ids": ids, "rating": rating})).unwrap();
    s.selection.ids.clear();
}

fn show(s: &mut Session, id: u64) {
    s.execute("library.source", &json!({"kind": "album", "id": id})).unwrap();
}

fn shown(s: &mut Session) -> Vec<u64> {
    s.visible_cloned().iter().map(|p| p.0).collect()
}

/// Trips ▸ { Rome [1, 2], Paris [2, 3] }, and Elsewhere [4] outside it.
fn trips(s: &mut Session) -> u64 {
    photos(s, 5);
    let trips = make(s, json!({"name": "Trips", "folder": true}));
    album(s, "Rome", Some(trips), &[1, 2]);
    album(s, "Paris", Some(trips), &[2, 3]);
    album(s, "Elsewhere", None, &[4]);
    trips
}

#[test]
fn a_folder_shows_the_photos_of_its_albums_once_each() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    show(&mut s, trips);
    assert_eq!(s.source, LibrarySource::Album(AlbumId(trips)));
    assert_eq!(shown(&mut s), [3, 2, 1], "newest first; 2 is in both albums and shows once");
    assert_eq!(s.source.label(&s.catalog), "Trips");
    assert_eq!(s.source_total(), Some(3));
}

#[test]
fn a_folder_follows_its_inner_folders_and_smart_albums() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    let europe = make(&mut s, json!({"name": "Europe", "folder": true, "parent": trips}));
    let oslo = album(&mut s, "Oslo", Some(europe), &[5]);
    show(&mut s, trips);
    assert_eq!(shown(&mut s), [5, 3, 2, 1]);
    show(&mut s, europe);
    assert_eq!(shown(&mut s), [5], "the inner folder shows only its own");
    // a smart album inside is live
    show(&mut s, trips);
    s.execute("album.createSmart", &json!({"name": "Best", "parent": europe, "rules": {"rating": 5}})).unwrap();
    rate(&mut s, &[4], 5);
    assert_eq!(shown(&mut s), [5, 4, 3, 2, 1]);
    // and so is the folder: what leaves its albums leaves it, what is moved out of it too
    s.selection.ids = vec![PhotoId(5)];
    s.execute("album.removePhotos", &json!({"id": oslo, "ids": [5]})).unwrap();
    assert_eq!(shown(&mut s), [4, 3, 2, 1]);
    s.execute("album.move", &json!({"id": europe, "parent": null})).unwrap();
    assert_eq!(shown(&mut s), [3, 2, 1]);
}

#[test]
fn a_folder_follows_the_sort() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    show(&mut s, trips);
    assert!(!s.sort.ascending, "newest first to begin with");
    assert_eq!(shown(&mut s), [3, 2, 1]);
    s.sort.ascending = true;
    assert_eq!(shown(&mut s), [1, 2, 3]);
}

#[test]
fn a_filter_narrows_a_folder() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    rate(&mut s, &[2, 4], 4);
    show(&mut s, trips);
    s.execute("library.filter", &json!({"rating": 4})).unwrap();
    assert_eq!(shown(&mut s), [2], "4 is rated too, but in no album of the folder");
    assert_eq!(s.source_total(), Some(3), "the folder's own count, whatever the filter hides");
}

#[test]
fn deleting_the_shown_folder_goes_back_to_all_photos() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    show(&mut s, trips);
    s.execute("album.delete", &json!({"id": trips})).unwrap();
    assert_eq!(s.source, LibrarySource::All);
    assert_eq!(shown(&mut s).len(), 5);
    // back with undo, and showable again with what it held
    s.execute("edit.undo", &json!({})).unwrap();
    show(&mut s, trips);
    assert_eq!(shown(&mut s), [3, 2, 1]);
}

#[test]
fn a_folder_view_saves_as_a_smart_album() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    show(&mut s, trips);
    let r = s.execute("album.createSmart", &json!({"name": "From Trips"})).unwrap();
    assert_eq!(r["count"], 3);
    let saved = r["id"].as_u64().unwrap();
    show(&mut s, saved);
    assert_eq!(shown(&mut s), [3, 2, 1]);
    assert!(s.catalog.smart_album_problems(AlbumId(saved)).is_empty());
    // it follows the folder
    album(&mut s, "Oslo", Some(trips), &[5]);
    assert_eq!(shown(&mut s), [5, 3, 2, 1]);
}

#[test]
fn a_folder_holds_no_photos_of_its_own() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    show(&mut s, trips);
    s.selection.ids = vec![PhotoId(1)];
    s.selection.active = Some(PhotoId(1));
    let steps = s.undo.len();
    for cmd in ["album.addPhotos", "album.removePhotos"] {
        let e = s.execute(cmd, &json!({"id": trips, "ids": [1]})).unwrap_err().to_string();
        assert!(e.contains("folder"), "{cmd}: {e}");
    }
    assert_eq!(s.undo.len(), steps, "nothing to undo");
    assert_eq!(shown(&mut s), [3, 2, 1]);
}

#[test]
fn an_unknown_album_is_no_source() {
    let mut s = Session::new();
    trips(&mut s);
    let e = s.execute("library.source", &json!({"kind": "album", "id": 999})).unwrap_err().to_string();
    assert!(e.contains("no such album"), "{e}");
    assert!(s.execute("library.source", &json!({"kind": "album"})).is_err());
    assert_eq!(s.source, LibrarySource::All);
}

#[test]
fn agents_see_a_folders_count() {
    let mut s = Session::new();
    let trips = trips(&mut s);
    let list = s.execute("albums.list", &json!({})).unwrap();
    let row = list.as_array().unwrap().iter().find(|a| a["id"] == trips).unwrap();
    assert_eq!((row["folder"].as_bool(), row["count"].as_u64()), (Some(true), Some(3)));
    assert_eq!(row["children"].as_array().unwrap().len(), 2);
}

#[test]
fn a_folder_is_still_the_source_after_reopening() {
    let dir = std::env::temp_dir().join(format!("lc-album-folder-source-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    let trips = trips(&mut s);
    show(&mut s, trips);
    let want = shown(&mut s);
    s.save_view();
    drop(s);
    let mut back = Session::new();
    back.open_library(&dir, false).unwrap();
    assert_eq!(back.source, LibrarySource::Album(AlbumId(trips)));
    assert_eq!(shown(&mut back), want);
    drop(back);
    let _ = std::fs::remove_dir_all(&dir);
}
