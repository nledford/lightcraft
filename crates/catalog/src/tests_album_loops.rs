//! Smart albums that would include themselves: through the albums they test, the smart albums
//! those test, and the folders any of them are in.
//!
//! The rule is the catalog's: a new edit that would put a smart album on such a loop is refused
//! (`Catalog::apply_new`), whoever makes it. Libraries saved before the rule, or damaged, may
//! still hold loops and must open (`Catalog::apply`, `replay` and snapshots take them); there a
//! loop has one meaning: every smart album on it holds nothing.
//!
//! Scenarios:
//! - Given smart albums on a loop, then each holds nothing, whichever album is asked first, one
//!   photo at a time or in a pass; albums that only test them see them as empty.
//! - Given any library, loops or not, then what an album holds doesn't depend on the order the
//!   albums are asked in.
//! - Given many smart albums each limited to the folder they are in, then showing the folder
//!   costs in proportion to their number.
//! - When a new edit (adding a smart album, moving an album or folder, changing rules, alone or
//!   in a batch) would put a smart album on a loop, then it is refused, names the album, and
//!   nothing changed.
//! - Given a library that already holds a loop, then it opens, the albums on the loop can still
//!   be edited and the loop undone, and edits elsewhere go through.

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

/// A smart album as a library saved before the rule could hold it (no check).
fn smart(c: &mut Catalog, name: &str, parent: Option<AlbumId>, rules: Filter) -> AlbumId {
    let id = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { parent, smart: Some(Box::new(rules)), ..Album::new(id, name) } }).unwrap();
    id
}

fn limited_to(album: AlbumId) -> Filter {
    Filter { album: Some(album), ..Default::default() }
}

/// Rules of Album rules: `(op, album)` joined by `all`.
fn tests(rules: &[(&str, AlbumId)]) -> Filter {
    let rules: Vec<serde_json::Value> = rules.iter().map(|(op, a)| serde_json::json!({"field": "album", "op": op, "value": a.0})).collect();
    serde_json::from_value(serde_json::json!({"ruleSet": {"match": "all", "rules": rules}})).unwrap()
}

fn holds(c: &Catalog, a: AlbumId, p: PhotoId) -> bool {
    c.album_contains(a, c.photo(p).unwrap())
}

/// Two folders whose smart albums are limited to each other's folder, an album with a photo, and
/// albums outside that test them.
#[test]
fn albums_on_a_loop_hold_nothing_whoever_asks_first() {
    let mut c = Catalog::new();
    let p = photo(&mut c, "p.jpg");
    let (g, ff) = (folder(&mut c, "G", None), folder(&mut c, "FF", None));
    let s = smart(&mut c, "S", Some(ff), limited_to(g));
    let t = smart(&mut c, "T", Some(g), limited_to(ff));
    album(&mut c, "R", Some(g), &[p]);
    let both = smart(&mut c, "S and T", None, tests(&[("is", s), ("is", t)]));
    let not_s = smart(&mut c, "Not S", None, tests(&[("isNot", s)]));
    let of_g = smart(&mut c, "Of G", None, limited_to(g));
    assert_eq!(c.albums_on_a_loop(), [s, t].into_iter().collect());
    let ask = |order: &[AlbumId]| -> Vec<(AlbumId, bool)> {
        let mut answers: Vec<(AlbumId, bool)> = order.iter().map(|a| (*a, holds(&c, *a, p))).collect();
        answers.sort();
        answers
    };
    let all = [s, t, both, not_s, of_g, g, ff];
    let forwards = ask(&all);
    let mut reversed = all;
    reversed.reverse();
    assert_eq!(forwards, ask(&reversed));
    let answer = |a: AlbumId| forwards.iter().find(|(x, _)| *x == a).unwrap().1;
    assert!(!answer(s) && !answer(t), "on the loop: nothing");
    assert!(!answer(both), "tests two albums that hold nothing");
    assert!(answer(not_s), "S holds nothing, so p isn't in it");
    assert!(answer(of_g) && answer(g), "the folder still shows the album in it");
    assert!(!answer(ff), "only S is in FF");
    // the same in a pass over the library
    for a in all {
        assert_eq!(c.album_count(a), usize::from(answer(a)), "album {a:?}");
        assert_eq!(c.album_photos(a).len(), usize::from(answer(a)), "album {a:?}");
    }
    // and each is reported
    for a in [s, t] {
        assert!(c.smart_album_problems(a).iter().any(|p| p.issue == crate::rules::Issue::AlbumLoop));
    }
}

/// A small generator: the same sequence every run.
struct Rng(u64);
impl Rng {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

/// Random libraries of folders, albums and smart albums that test each other at random (many end
/// up with loops): what each album holds is the same asked forwards, backwards, alone or in a pass.
#[test]
fn what_an_album_holds_never_depends_on_who_asks_first() {
    let mut rng = Rng(7);
    let mut with_loops = 0;
    for _ in 0..300 {
        let mut c = Catalog::new();
        let photos: Vec<PhotoId> = (0..4).map(|i| photo(&mut c, &format!("p{i}.jpg"))).collect();
        let n = 3 + rng.next(8);
        let mut ids: Vec<AlbumId> = Vec::new();
        let mut folders: Vec<AlbumId> = Vec::new();
        for i in 0..n {
            let parent = (!folders.is_empty() && rng.next(2) == 0).then(|| folders[rng.next(folders.len() as u64) as usize]);
            let id = match rng.next(4) {
                0 => {
                    let f = folder(&mut c, &format!("F{i}"), parent);
                    folders.push(f);
                    f
                }
                1 => {
                    let held: Vec<PhotoId> = photos.iter().copied().filter(|_| rng.next(2) == 0).collect();
                    album(&mut c, &format!("A{i}"), parent, &held)
                }
                // rules are set once every album exists, so they can name later ones
                _ => smart(&mut c, &format!("S{i}"), parent, Filter::default()),
            };
            ids.push(id);
        }
        for id in ids.clone() {
            if c.album(id).is_some_and(|a| a.smart.is_some()) {
                let pick = |rng: &mut Rng| ids[rng.next(ids.len() as u64) as usize];
                let rules = match rng.next(3) {
                    0 => limited_to(pick(&mut rng)),
                    1 => tests(&[(if rng.next(2) == 0 { "is" } else { "isNot" }, pick(&mut rng))]),
                    _ => tests(&[("is", pick(&mut rng)), ("isNot", pick(&mut rng))]),
                };
                // unchecked, as an old library could hold them (a filter naming a smart album
                // directly is the one thing `apply` itself refuses)
                let _ = c.apply(Op::SetAlbumRules { id, rules: Box::new(rules) });
            }
        }
        with_loops += usize::from(!c.albums_on_a_loop().is_empty());
        let table = |order: &[AlbumId]| -> Vec<(AlbumId, PhotoId, bool)> {
            let mut t: Vec<(AlbumId, PhotoId, bool)> = Vec::new();
            for a in order {
                for p in &photos {
                    t.push((*a, *p, holds(&c, *a, *p)));
                }
            }
            t.sort();
            t
        };
        let forwards = table(&ids);
        let mut reversed = ids.clone();
        reversed.reverse();
        assert_eq!(forwards, table(&reversed), "{}", c.to_snapshot());
        let in_a_pass = crate::gathering(|| table(&reversed));
        assert_eq!(forwards, in_a_pass, "{}", c.to_snapshot());
        for a in &ids {
            let want: Vec<PhotoId> = forwards.iter().filter(|(x, _, yes)| x == a && *yes).map(|(_, p, _)| *p).collect();
            assert_eq!(c.album_photos(*a).into_iter().filter(|p| !c.photo(*p).unwrap().deleted).collect::<Vec<_>>(), want, "{}", c.to_snapshot());
        }
        for a in c.albums_on_a_loop() {
            assert!(photos.iter().all(|p| !holds(&c, a, *p)), "on a loop, so empty: {}", c.to_snapshot());
        }
    }
    assert!(with_loops > 50, "the generator makes loops: {with_loops} of 300");
}

/// `k` smart albums each limited to the folder they are all in, 200 photos: showing the folder
/// asked each of them about all the others (cubic in `k`). Four times the albums now costs a few
/// times more, not sixty.
#[test]
fn a_folder_of_looping_albums_costs_in_proportion() {
    let build = |k: usize| {
        let mut c = Catalog::new();
        let photos: Vec<PhotoId> = (0..200).map(|i| photo(&mut c, &format!("p{i}.jpg"))).collect();
        let f = folder(&mut c, "F", None);
        album(&mut c, "Plain", Some(f), &photos[..100]);
        for i in 0..k {
            smart(&mut c, &format!("S{i}"), Some(f), limited_to(f));
        }
        (c, f)
    };
    let timed = |c: &Catalog, f: AlbumId| {
        (0..5)
            .map(|_| {
                let start = std::time::Instant::now();
                assert_eq!(c.album_count(f), 100);
                start.elapsed()
            })
            .min()
            .unwrap()
    };
    let (small, f_small) = build(10);
    let (big, f_big) = build(40);
    let (t10, t40) = (timed(&small, f_small), timed(&big, f_big));
    assert!(t40 < t10 * 20 + std::time::Duration::from_millis(2), "10 albums: {t10:?}; 40 albums: {t40:?}");
}

fn album_names(c: &Catalog) -> Vec<(AlbumId, Option<AlbumId>, String)> {
    c.albums().map(|a| (a.id, a.parent, serde_json::to_string(&a.smart).unwrap())).collect()
}

#[test]
fn a_new_edit_that_would_make_a_loop_is_refused() {
    let mut c = Catalog::new();
    let trips = folder(&mut c, "Trips", None);
    let europe = folder(&mut c, "Europe", Some(trips));
    let view = smart(&mut c, "Trips view", None, limited_to(trips));
    let plain_rules = smart(&mut c, "Rated", Some(trips), Filter { rating: 3, ..Default::default() });
    let before = album_names(&c);
    let next = AlbumId(99);
    let new_smart = |parent, rules| Op::AddAlbum { album: Album { parent, smart: Some(Box::new(rules)), ..Album::new(next, "New one") } };
    let refused: Vec<(&str, Op, &str)> = vec![
        ("made in the folder it shows", new_smart(Some(trips), limited_to(trips)), "New one"),
        ("made in a folder inside it", new_smart(Some(europe), limited_to(trips)), "New one"),
        ("moved into it", Op::MoveAlbum { id: view, parent: Some(europe) }, "Trips view"),
        ("rules changed to show its own folder", Op::SetAlbumRules { id: plain_rules, rules: Box::new(limited_to(trips)) }, "Rated"),
        ("rules changed to test an album that tests it", Op::SetAlbumRules { id: view, rules: Box::new(tests(&[("is", view)])) }, "Trips view"),
        (
            "in a batch",
            Op::Batch { ops: vec![Op::RenameAlbum { id: view, name: "Renamed".into() }, Op::MoveAlbum { id: view, parent: Some(trips) }] },
            "Trips view",
        ),
    ];
    for (what, op, name) in refused {
        let e = c.apply_new(op).unwrap_err().to_string();
        assert!(e.contains(name) && e.contains("include itself"), "{what}: {e}");
        assert_eq!(album_names(&c), before, "{what}: nothing changed");
        assert_eq!(c.album(view).unwrap().name, "Trips view", "{what}");
        assert!(c.albums_on_a_loop().is_empty(), "{what}");
    }
    // what makes no loop goes through, and is undone by what it returns
    let allowed = vec![
        new_smart(None, limited_to(trips)),
        new_smart(Some(trips), limited_to(europe)),
        Op::MoveAlbum { id: plain_rules, parent: Some(europe) },
        Op::SetAlbumRules { id: view, rules: Box::new(tests(&[("isNot", plain_rules)])) },
    ];
    for op in allowed {
        let undo = c.apply_new(op.clone()).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        assert!(c.albums_on_a_loop().is_empty());
        c.apply(undo).unwrap();
        assert_eq!(album_names(&c), before);
    }
}

#[test]
fn a_library_that_already_holds_a_loop_opens_and_can_be_mended() {
    // as written by a version without the rule: the op log makes a loop
    let mut old = Catalog::new();
    let mut log = String::new();
    let mut record = |c: &mut Catalog, op: Op| {
        log.push_str(&Catalog::op_to_log_line(&op));
        c.apply(op).unwrap();
    };
    let trips = AlbumId(1);
    let (inside, other) = (AlbumId(2), AlbumId(3));
    record(&mut old, Op::AddAlbum { album: Album { folder: true, ..Album::new(trips, "Trips") } });
    record(&mut old, Op::AddAlbum { album: Album { parent: Some(trips), smart: Some(Box::new(limited_to(trips))), ..Album::new(inside, "Inside") } });
    record(&mut old, Op::AddAlbum { album: Album { smart: Some(Box::new(Filter::default())), ..Album::new(other, "Other") } });
    let mut c = Catalog::new();
    assert_eq!(c.replay(&log).unwrap(), 3, "the log replays");
    assert_eq!(c.albums_on_a_loop(), [inside].into_iter().collect());
    let from_snapshot = Catalog::from_snapshot(&c.to_snapshot()).unwrap();
    assert_eq!(from_snapshot.albums_on_a_loop(), [inside].into_iter().collect());
    // edits elsewhere, and edits to the album on the loop that leave it there, go through
    c.apply_new(Op::RenameAlbum { id: inside, name: "Still inside".into() }).unwrap();
    c.apply_new(Op::SetAlbumRules { id: other, rules: Box::new(Filter { rating: 2, ..Default::default() }) }).unwrap();
    c.apply_new(Op::SetAlbumRules { id: inside, rules: Box::new(Filter { rating: 4, album: Some(trips), ..Default::default() }) }).unwrap();
    // but it doesn't get to take another album onto a loop
    let e = c.apply_new(Op::MoveAlbum { id: other, parent: Some(trips) }).map(|_| ());
    assert!(e.is_ok(), "Other tests no album: moving it in makes no loop");
    let e = c.apply_new(Op::SetAlbumRules { id: other, rules: Box::new(limited_to(trips)) }).unwrap_err().to_string();
    assert!(e.contains("Other"), "{e}");
    // mended by moving it out
    c.apply_new(Op::MoveAlbum { id: inside, parent: None }).unwrap();
    assert!(c.albums_on_a_loop().is_empty());
}
