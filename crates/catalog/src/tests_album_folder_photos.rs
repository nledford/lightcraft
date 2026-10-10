//! What a folder of albums shows (as a collection set does in Lightroom Classic): every photo of
//! the albums and smart albums inside it, at any depth, each photo once.
//!
//! Scenarios:
//! - Given a folder with two albums that share a photo, when the folder is shown, then it lists
//!   the photos of both, the shared one once, and nothing from albums outside it.
//! - Given a smart album in the folder, then its current matches are shown too.
//! - Given a folder inside the folder, then the inner folder's albums count as well, and the
//!   inner folder alone shows only its own.
//! - Given a photo in Recently Deleted, then the folder doesn't count or list it.
//! - Given an empty folder, then it shows nothing.
//! - A folder is not an album a photo is "in": `albums_of` names albums only.
//! - Given a smart album testing the folder it is in, then that is a loop: it is reported, and
//!   asking still answers.
//! - Given a damaged library whose folders contain each other, then asking still answers.
//! - Given a folder of many big albums, then showing it costs a pass over the photos, not one
//!   per album.

use crate::*;

fn photo(c: &mut Catalog, name: &str) -> PhotoId {
    let id = c.alloc_photo_id();
    let p = Photo::new(id, Source::Demo { scene: 1 }, name, "JPEG", 6000, 4000, "2026-09-30T10:00:00");
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    id
}

fn folder(c: &mut Catalog, name: &str, parent: Option<AlbumId>) -> AlbumId {
    let id = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { parent, folder: true, ..Album::new(id, name) } }).unwrap();
    id
}

fn album(c: &mut Catalog, name: &str, parent: Option<AlbumId>, photos: &[PhotoId]) -> AlbumId {
    let id = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { parent, photos: photos.to_vec(), ..Album::new(id, name) } }).unwrap();
    id
}

fn smart(c: &mut Catalog, name: &str, parent: Option<AlbumId>, rules: Filter) -> AlbumId {
    let id = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { parent, smart: Some(Box::new(rules)), ..Album::new(id, name) } }).unwrap();
    id
}

/// What the grid shows for an album or folder, oldest first (these photos share a date, so in the
/// order they were added).
fn shown(c: &Catalog, id: AlbumId) -> Vec<PhotoId> {
    c.query(&Filter { album: Some(id), ..Default::default() }, &Sort { ascending: true, ..Default::default() })
}

#[test]
fn a_folder_shows_the_photos_of_its_albums_once_each() {
    let mut c = Catalog::new();
    let [a, b, d, e] = ["a.jpg", "b.jpg", "d.jpg", "e.jpg"].map(|n| photo(&mut c, n));
    let trips = folder(&mut c, "Trips", None);
    album(&mut c, "Rome", Some(trips), &[a, b]);
    album(&mut c, "Paris", Some(trips), &[b, d]);
    album(&mut c, "Elsewhere", None, &[e]);
    assert_eq!(shown(&c, trips), [a, b, d], "b is in both albums and shows once; e is outside the folder");
    assert_eq!(c.album_photos(trips), [a, b, d]);
    assert_eq!(c.album_count(trips), 3);
    let in_trips = |id: PhotoId| c.album_contains(trips, c.photo(id).unwrap());
    assert!(in_trips(a) && in_trips(b) && in_trips(d) && !in_trips(e));
    assert_eq!(c.album_members(trips).len(), 2);
}

#[test]
fn a_folder_shows_what_its_smart_albums_match_now() {
    let mut c = Catalog::new();
    let [a, b, d] = ["a.jpg", "b.jpg", "d.jpg"].map(|n| photo(&mut c, n));
    let trips = folder(&mut c, "Trips", None);
    album(&mut c, "Rome", Some(trips), &[a]);
    smart(&mut c, "Best", Some(trips), Filter { rating: 4, ..Default::default() });
    assert_eq!(shown(&c, trips), [a]);
    // a smart album is live, and so is the folder holding it
    c.apply(Op::SetRating { id: b, rating: 5 }).unwrap();
    c.apply(Op::SetRating { id: a, rating: 5 }).unwrap();
    assert_eq!(shown(&c, trips), [a, b], "a is in the album and matches the smart album: once");
    assert_eq!(c.album_count(trips), 2);
    assert!(!c.album_contains(trips, c.photo(d).unwrap()));
}

#[test]
fn a_folder_shows_the_albums_of_the_folders_inside_it() {
    let mut c = Catalog::new();
    let [a, b, d] = ["a.jpg", "b.jpg", "d.jpg"].map(|n| photo(&mut c, n));
    let trips = folder(&mut c, "Trips", None);
    let europe = folder(&mut c, "Europe", Some(trips));
    let rome = album(&mut c, "Rome", Some(europe), &[a]);
    let asia = album(&mut c, "Asia", Some(trips), &[b]);
    album(&mut c, "Loose", None, &[d]);
    assert_eq!(shown(&c, trips), [a, b]);
    assert_eq!(shown(&c, europe), [a], "the inner folder shows only its own");
    let mut members = c.album_members(trips);
    members.sort();
    assert_eq!(members, [rome, asia], "albums at any depth, the folders themselves left out");
    // an album is its own only member; nothing else is one
    assert_eq!(c.album_members(rome), [rome]);
    assert!(c.album_members(AlbumId(999)).is_empty());
}

#[test]
fn a_folder_leaves_out_deleted_photos() {
    let mut c = Catalog::new();
    let [a, b] = ["a.jpg", "b.jpg"].map(|n| photo(&mut c, n));
    let trips = folder(&mut c, "Trips", None);
    album(&mut c, "Rome", Some(trips), &[a, b]);
    c.apply(Op::SetDeleted { id: b, deleted: true }).unwrap();
    assert_eq!(shown(&c, trips), [a]);
    assert_eq!(c.album_photos(trips), [a]);
    assert_eq!(c.album_count(trips), 1);
}

#[test]
fn an_empty_folder_shows_nothing() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg");
    let empty = folder(&mut c, "Empty", None);
    let inner = folder(&mut c, "Inner", Some(empty));
    assert!(shown(&c, empty).is_empty() && shown(&c, inner).is_empty());
    assert_eq!((c.album_count(empty), c.album_photos(empty).len()), (0, 0));
    assert!(!c.album_contains(empty, c.photo(a).unwrap()));
    assert!(c.album_members(empty).is_empty());
}

#[test]
fn a_photo_is_in_albums_not_in_their_folders() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg");
    let trips = folder(&mut c, "Trips", None);
    let rome = album(&mut c, "Rome", Some(trips), &[a]);
    assert_eq!(c.albums_of(a), [rome]);
}

/// A smart album inside a folder that is limited to that folder (by its album filter, or by an
/// Album rule saved before the check refused it) would include itself: reported as a loop, and it
/// holds nothing of its own, so the question still ends.
#[test]
fn a_smart_album_testing_its_own_folder_is_a_loop_that_ends() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg");
    let trips = folder(&mut c, "Trips", None);
    let europe = folder(&mut c, "Europe", Some(trips));
    album(&mut c, "Rome", Some(europe), &[a]);
    let by_filter = smart(&mut c, "In Trips", Some(europe), Filter { album: Some(trips), ..Default::default() });
    let outside = smart(&mut c, "Trips, outside", None, Filter { album: Some(trips), ..Default::default() });
    let loops = |c: &Catalog, id: AlbumId| c.smart_album_problems(id).iter().any(|p| p.issue == crate::rules::Issue::AlbumLoop);
    assert!(c.album_reaches(by_filter, by_filter) && loops(&c, by_filter), "it is inside the folder it is limited to");
    assert!(!c.album_reaches(outside, outside) && c.album_reaches(outside, by_filter));
    assert!(!loops(&c, outside), "limited to a folder it isn't in: no loop");
    // both answer; the one outside shows the folder's photos
    assert_eq!(shown(&c, outside), [a]);
    assert_eq!(shown(&c, trips), [a]);
    assert_eq!(shown(&c, by_filter), [a], "what the folder holds without it");
    // the same through an Album rule
    let rule: Filter = serde_json::from_value(serde_json::json!({"ruleSet": {"rules": [{"field": "album", "op": "is", "value": trips.0}]}})).unwrap();
    let by_rule = smart(&mut c, "Rule", Some(trips), rule.clone());
    assert!(c.album_reaches(by_rule, by_rule));
    let problems = rule.rule_set.unwrap().check_for(&c, Some(by_rule));
    assert!(!problems.is_empty(), "an Album rule still can't name a folder");
    let _ = shown(&c, by_rule); // returns
    let _ = shown(&c, trips);
}

/// Folders that contain each other can't be made here, but a damaged library file can say so.
#[test]
fn folders_containing_each_other_still_answer() {
    let mut c = Catalog::new();
    let a = photo(&mut c, "a.jpg");
    let x = folder(&mut c, "X", None);
    let y = folder(&mut c, "Y", Some(x));
    let inside = album(&mut c, "Inside", Some(y), &[a]);
    c.albums.get_mut(&x).unwrap().parent = Some(y);
    assert_eq!(c.album_members(x), [inside]);
    assert_eq!(c.album_members(y), [inside]);
    assert_eq!(shown(&c, x), [a]);
    assert_eq!(c.album_count(y), 1);
    // a folder that is its own parent
    c.albums.get_mut(&x).unwrap().parent = Some(x);
    assert_eq!(c.album_members(x), [inside]);
    assert_eq!(shown(&c, y), [a]);
    // and a ring the album asked about is not in
    let other = folder(&mut c, "Other", None);
    assert!(c.album_members(other).is_empty() && shown(&c, other).is_empty());
}

/// 100 albums of 2,000 photos each in one folder: testing every photo against every album's list
/// would be a billion comparisons. The folder's photos are gathered once per question.
#[test]
fn a_folder_of_big_albums_is_shown_in_one_pass() {
    let mut c = Catalog::new();
    let photos: Vec<PhotoId> = (0..4000).map(|i| photo(&mut c, &format!("p{i}.jpg"))).collect();
    let big = folder(&mut c, "Big", None);
    for k in 0..100usize {
        let list: Vec<PhotoId> = photos.iter().copied().skip(k).step_by(2).collect();
        album(&mut c, &format!("A{k}"), Some(big), &list);
    }
    let start = std::time::Instant::now();
    assert_eq!(shown(&c, big).len(), 4000);
    assert_eq!(c.album_count(big), 4000);
    assert_eq!(c.album_photos(big).len(), 4000);
    assert!(start.elapsed() < std::time::Duration::from_secs(2), "took {:?}", start.elapsed());
}
