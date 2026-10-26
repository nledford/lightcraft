//! `library.filter` and `filter.applyPreset` while an album is shown: the album shown is the source,
//! so an album in the filter would narrow the next source unseen (the filter bar has no album chip).
use lightcraft_catalog::{Flag, Op, Photo, PhotoId, Source};
use serde_json::json;

use crate::{Result, Selection, Session};

/// Photos 1 to 3 (rated 5, 5 and 2). Album X holds photo 1 and album A holds photo 3.
fn session_with_two_albums() -> Result<(Session, u64, u64)> {
    let mut s = Session::new();
    for (id, rating) in [(1, 5), (2, 5), (3, 2)] {
        let mut photo = Photo::new(
            PhotoId(id),
            Source::File { path: format!("/lightcraft-album-shown/{id}.jpg") },
            &format!("{id}.jpg"),
            "JPEG",
            40,
            30,
            "2026-10-26",
        );
        photo.rating = rating;
        photo.flag = Flag::None;
        s.catalog.apply(Op::AddPhoto { photo: Box::new(photo) })?;
    }
    let mut make = |name: &str, id: u64| {
        s.selection = Selection { ids: vec![PhotoId(id)], active: None };
        s.execute("album.create", &json!({"name": name, "addSelected": true})).unwrap()["id"].as_u64().unwrap()
    };
    let (x, a) = (make("X", 1), make("A", 3));
    Ok((s, x, a))
}

#[test]
fn library_filter_refuses_an_album_while_an_album_is_shown() {
    let (mut s, x, a) = session_with_two_albums().unwrap();
    s.execute("library.source", &json!({"kind": "album", "id": a})).unwrap();
    s.execute("library.filter", &json!({"rating": 4})).unwrap();
    let before = s.filter.clone();
    let e = s.execute("library.filter", &json!({"album": x, "rating": 1})).unwrap_err().to_string();
    assert!(e.contains("album is already shown"), "the refusal says why: {e}");
    assert_eq!(s.filter, before, "a refused call changes nothing, the rating included");
    s.execute("library.source", &json!({"kind": "all"})).unwrap();
    assert_eq!(s.visible().len(), 2, "no hidden album narrows All Photos (photos 1 and 2 have four stars or more)");
}

#[test]
fn library_filter_clearing_the_album_is_fine_while_an_album_is_shown() {
    let (mut s, _, a) = session_with_two_albums().unwrap();
    s.execute("library.source", &json!({"kind": "album", "id": a})).unwrap();
    s.execute("library.filter", &json!({"album": null, "rating": 1})).unwrap();
    assert_eq!(s.filter.rating, 1, "a null album is not an album: the rest applies");
}

#[test]
fn applying_a_preset_with_an_album_skips_the_album_while_an_album_is_shown() {
    let (mut s, x, a) = session_with_two_albums().unwrap();
    s.execute("library.filter", &json!({"album": x, "rating": 4})).unwrap();
    s.execute("filter.savePreset", &json!({"name": "In X"})).unwrap();
    s.execute("library.source", &json!({"kind": "album", "id": a})).unwrap();
    let r = s.execute("filter.applyPreset", &json!({"name": "In X"})).unwrap();
    assert_eq!(s.filter.album, None, "the album the preset held is not left in the filter");
    assert_eq!(s.filter.rating, 4, "the rest of the preset applies");
    assert_eq!(r["skipped"], json!(["album"]), "the result names the skip: {r}");
    assert_eq!(r["photos"].as_u64(), Some(0), "photo 3 (album A) has two stars");
    s.execute("library.source", &json!({"kind": "all"})).unwrap();
    assert_eq!(s.visible().len(), 2, "no hidden album narrows All Photos");
}

#[test]
fn a_preset_with_an_album_applies_whole_while_no_album_is_shown() {
    let (mut s, x, _) = session_with_two_albums().unwrap();
    s.execute("library.filter", &json!({"album": x})).unwrap();
    s.execute("filter.savePreset", &json!({"name": "In X"})).unwrap();
    s.execute("library.filter", &json!({"album": null})).unwrap();
    let r = s.execute("filter.applyPreset", &json!({"name": "In X"})).unwrap();
    assert_eq!(s.filter.album.map(|a| a.0), Some(x), "the album stays when none is shown");
    assert_eq!(r.get("skipped"), None, "nothing skipped: {r}");
    assert_eq!(r["photos"].as_u64(), Some(1));
}

#[test]
fn library_filter_sets_an_album_while_no_album_is_shown() {
    let (mut s, x, _) = session_with_two_albums().unwrap();
    s.execute("library.source", &json!({"kind": "picks"})).unwrap();
    s.execute("library.filter", &json!({"album": x})).unwrap();
    assert_eq!(s.filter.album.map(|a| a.0), Some(x), "unchanged: an album can still be a filter over another source");
}
