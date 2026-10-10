//! The stored form of catalog records ([`crate::stored`]): what is saved is only what was set,
//! and a saved library reads back exactly as it was.

use std::sync::Arc;

use lightcraft_develop::DevelopSettings;
use serde_json::Value;

use crate::journal::{LOG, SNAPSHOT, VERSION, encode_record};
use crate::*;

// ---- fixtures (also used, as they are, to write `fixtures/v6.*` with the format 6 build)

/// Settings with nothing stated, as every saved library reads them: every value its default,
/// on the first rendering process (new photos get the latest one, see
/// `the_rendering_process_survives`). The fixtures start from these, so they stay what the
/// format 6 build wrote when a new process becomes the default.
fn base() -> DevelopSettings {
    serde_json::from_str("{}").unwrap()
}

fn plain(id: u64) -> Photo {
    let mut p = Photo::new(
        PhotoId(id),
        Source::File { path: format!("/pictures/2019/trip/IMG_{id:04}.jpg") },
        &format!("IMG_{id:04}.jpg"),
        "JPEG",
        4032,
        3024,
        "2026-10-10T12:00:00",
    );
    p.develop = Arc::new(base());
    p
}

fn edited(d: impl FnOnce(&mut DevelopSettings)) -> Arc<DevelopSettings> {
    let mut s = base();
    d(&mut s);
    Arc::new(s)
}

/// Settings with lists, choices and optional values: masks of several kinds, curve points,
/// guides, an Upright transform, a locked crop aspect, spots, red eye, point colours.
fn rich() -> DevelopSettings {
    use lightcraft_develop::{Mask, MaskComponent, MaskOp, MaskShape, PointColor, RedEye, SegMask, Spot};
    use lightcraft_geom::{Homography, Point};
    let at = |x: f64, y: f64| Point { x, y };
    let seg = |side: u32| SegMask { side, data: "eJxjYAAAAAEAAQ==".into(), rect: Some([0.1, 0.2, 0.6, 0.7]) };
    let component = |shape: MaskShape| MaskComponent { name: None, op: MaskOp::default(), invert: false, shape };
    let mut s = DevelopSettings { treatment: lightcraft_develop::Treatment::Bw, ..base() };
    s.profile.id = "lc.mono".into();
    s.profile.amount = 150.0;
    s.profile.camera_local_tone = true;
    s.wb.tint = 12.0;
    s.curve.master = vec![at(0.0, 0.0), at(0.4, 0.55), at(1.0, 1.0)];
    s.curve.red = vec![at(0.0, 0.1), at(1.0, 0.9)];
    s.point_colors = vec![PointColor { hue_shift: 20.0, ..Default::default() }];
    s.geometry.guides = vec![(at(0.1, 0.1), at(0.1, 0.9)), (at(0.9, 0.1), at(0.8, 0.9))];
    s.geometry.upright_transform = Some(Homography([1.0, 0.01, 0.0, -0.01, 1.0, 0.0, 0.0, 0.002, 1.0]));
    s.crop.aspect = Some((3, 2));
    s.crop.flip_h = true;
    s.crop.geometry.angle = 1.5;
    s.spots = vec![Spot { points: vec![at(0.3, 0.3), at(0.32, 0.31)], source_offset: Some(at(0.05, 0.0)), ..Default::default() }];
    s.red_eye = vec![RedEye { center: at(0.5, 0.4), catchlight: Some(at(0.1, -0.1)), ..Default::default() }];
    s.masks = vec![
        Mask {
            id: 1,
            name: "Sky".into(),
            components: vec![
                component(MaskShape::Linear { start: at(0.5, 0.0), end: at(0.5, 0.4) }),
                MaskComponent {
                    name: Some("Sun".into()),
                    invert: true,
                    ..component(MaskShape::Radial { center: at(0.7, 0.2), rx: 0.1, ry: 0.1, angle: 0.0, feather: 50.0, invert: false })
                },
                component(MaskShape::LuminanceRange { lo: 0.6, hi: 1.0, lo_feather: 0.1, hi_feather: 0.0 }),
            ],
            refine: 30.0,
            ..Default::default()
        },
        Mask {
            id: 2,
            name: "Boat".into(),
            components: vec![
                component(MaskShape::Object {
                    hint: vec![at(0.4, 0.6)],
                    exclude: vec![at(0.1, 0.1)],
                    seg: Some(seg(256)),
                    detail: vec![seg(128), seg(64)],
                    edge: 2.0,
                }),
                component(MaskShape::Prompt { text: "the red boat".into(), seg: None, detail: Vec::new(), edge: 0.0 }),
                component(MaskShape::ColorRange { samples: vec![[0.5, 0.1, 0.2]], refine: 40.0 }),
                component(MaskShape::People { person: 1, parts: vec!["face".into()] }),
                component(MaskShape::Subject),
            ],
            ..Default::default()
        },
    ];
    s.disabled_sections = vec!["detail".into(), "light".into()];
    s
}

/// A photo with every field set to something other than what [`plain`] has.
fn full() -> Photo {
    let look = edited(|s| s.light.contrast = 20.0);
    let now = Arc::new(DevelopSettings { light: lightcraft_develop::Light { exposure: 0.3, ..Default::default() }, ..rich() });
    let mut p = plain(7);
    p.kind = MediaKind::Raw;
    p.file_size = 25_000_000;
    p.captured = Some("2019-07-14T15:21:07".into());
    p.meta = Meta {
        camera: "Camera Model X".into(),
        lens: "24-70mm f/2.8".into(),
        focal_mm: Some(35.0),
        aperture: Some(4.0),
        shutter: "1/250".into(),
        iso: Some(400),
        location: "Old harbour".into(),
        city: "Lisbon".into(),
        state: "Lisboa".into(),
        country: "Portugal".into(),
        gps: Some((38.7, -9.1)),
        title: "Harbour".into(),
        caption: "Boats at dusk".into(),
        alt_text: "Three boats".into(),
        extended_description: "Three boats moored at dusk".into(),
        copyright: "(c) Someone".into(),
        copyright_status: CopyrightStatus::Copyrighted,
        usage_terms: "All rights reserved".into(),
        copyright_url: "https://example.org/rights".into(),
        creator: "Someone".into(),
        keywords: vec!["Places|Lisbon".into(), "boats".into()],
        regions: vec![lightcraft_meta::Region {
            rect: lightcraft_meta::Rect { x0: 0.1, y0: 0.2, x1: 0.3, y1: 0.4 },
            kind: lightcraft_meta::RegionKind::Face,
            name: Some("Ana".into()),
            description: None,
        }],
        // (no format with a fixture had it; see `fields_since_the_fixtures_survive_the_stored_form`)
        focus: None,
    };
    p.rating = 4;
    p.flag = Flag::Pick;
    p.label = Some(ColorLabel::Green);
    p.develop = now.clone();
    p.edited = Some("2026-10-10T13:00:00".into());
    p.versions = vec![Version { name: "Warm".into(), created: "2026-10-10T12:30:00".into(), settings: look.clone(), auto: true }];
    p.history = vec![HistoryStep { label: "Contrast".into(), settings: look.clone() }, HistoryStep { label: "Exposure".into(), settings: now }];
    p.deleted = true;
    p.local = true;
    p.duration = Some(12.5);
    p.as_shot_wb = Some((5200.0, 3.0));
    p.content_hash = Some("00000000000000009e3779b97f4a7c15".into());
    p.embedded_lens = Some(lightcraft_develop::EmbeddedLens { vignette: Some(Default::default()), ..Default::default() });
    p.copy_of = Some(PhotoId(3));
    p.copy_name = Some("Copy 1".into());
    p.import_look = Some(look);
    p.analysis = Some(Analysis { sharpness: 71.0, clipped: 0.02, group: Some(4), best: true });
    p.preview_only = Some("unsupported raw variant".into());
    p.local_baseline = Some(0xdead_beef);
    p
}

/// Changes made after the snapshot of the format 6 fixture (they are its log).
fn later_ops() -> Vec<Op> {
    let warm = edited(|s| s.wb.temp += 500.0);
    vec![
        Op::SetDevelop { id: PhotoId(8), settings: warm.clone(), label: "Temperature".into(), edited: Some("2026-10-10T14:00:00".into()) },
        Op::PushHistory { id: PhotoId(8), step: HistoryStep { label: "Temperature".into(), settings: warm } },
        Op::SetMeta { id: PhotoId(8), meta: Box::new(Meta { title: "Harbour".into(), keywords: vec!["boats".into()], ..Default::default() }) },
        Op::SetRating { id: PhotoId(9), rating: 3 },
        Op::SetVersions {
            id: PhotoId(9),
            versions: vec![Version { name: "Rich".into(), created: "2026-10-10T14:05:00".into(), settings: Arc::new(rich()), auto: false }],
        },
    ]
}

// ---- end of fixtures

fn catalog_of(photos: impl IntoIterator<Item = Photo>) -> Catalog {
    let mut c = Catalog::new();
    for p in photos {
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    c
}

/// The stored record of the catalog's only photo, as JSON.
fn record(c: &Catalog) -> Value {
    let v: Value = serde_json::from_str(&c.to_snapshot()).unwrap();
    v["photos"].as_object().unwrap().values().next().unwrap().clone()
}

/// The catalog saved and read again (counting changes like the one it was read from). Every
/// photo must also read back with the same public JSON, which tells apart what `==` doesn't
/// (-0.0 and 0.0) and is what a photo's fingerprint and its preview-cache key are made from.
fn reopened(c: &Catalog) -> Catalog {
    let mut r = Catalog::from_snapshot(&c.to_snapshot()).unwrap();
    r.revision = c.revision;
    for (a, b) in c.photos().zip(r.photos()) {
        assert_eq!(serde_json::to_string(a.as_ref()).unwrap(), serde_json::to_string(b.as_ref()).unwrap());
        assert_eq!((a.fingerprint(), a.develop.hash64()), (b.fingerprint(), b.develop.hash64()));
    }
    r
}

/// Given a photo nobody edited or described, when the library is saved, then its record holds
/// what identifies the file and nothing else.
#[test]
fn an_untouched_photo_is_stored_without_defaults() {
    let c = catalog_of([plain(1)]);
    let r = record(&c);
    let keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["develop", "file_name", "format", "height", "id", "imported", "source", "width"]);
    assert_eq!(r["develop"], serde_json::json!({}));
    assert_eq!(reopened(&c), c);
}

/// The stored form leaves out a photo's fields that are unset, so libraries depend on what
/// "unset" reads as. If this fails because the record of an untouched photo changed, or one of
/// these defaults did: that is a new catalog format ([`journal::VERSION`]) whose loader gives
/// the photos of older libraries the old value.
#[test]
fn the_record_of_an_untouched_photo_is_pinned() {
    let c = catalog_of([plain(1)]);
    assert_eq!(
        stored_text(&c),
        r#"{"id":1,"source":{"type":"file","path":"/pictures/2019/trip/IMG_0001.jpg"},"file_name":"IMG_0001.jpg","format":"JPEG","width":4032,"height":3024,"imported":"2026-10-10T12:00:00","develop":{}}"#
    );
    let unset = plain(1);
    assert_eq!(
        (unset.kind, unset.flag, unset.rating, unset.file_size, unset.deleted, unset.local),
        (MediaKind::Image, Flag::None, 0, 0, false, false)
    );
    assert_eq!((MediaKind::default(), Flag::default(), CopyrightStatus::default()), (MediaKind::Image, Flag::None, CopyrightStatus::Unknown));
    assert_eq!(Meta::default().copyright_status, CopyrightStatus::Unknown);
}

/// The stored record of the catalog's only photo (id 1), as it is written.
fn stored_text(c: &Catalog) -> String {
    let snapshot = c.to_snapshot();
    let start = snapshot.find(r#""1":{"#).unwrap() + 4;
    let end = snapshot.find(r#"},"albums""#).unwrap();
    snapshot[start..end].to_string()
}

/// Given a photo with a title only, then the stored metadata is the title only.
#[test]
fn metadata_is_stored_field_by_field() {
    let mut p = plain(1);
    p.meta.title = "Harbour".into();
    let c = catalog_of([p]);
    assert_eq!(record(&c)["meta"], serde_json::json!({"title": "Harbour"}));
    assert_eq!(reopened(&c), c);
}

/// Given an edited photo, then its settings, history steps, versions and import look are stored
/// as what differs from the defaults.
#[test]
fn develop_settings_are_stored_as_their_difference_from_the_defaults() {
    let mut p = plain(1);
    p.develop = edited(|s| s.light.exposure = 0.3);
    p.history = vec![HistoryStep { label: "Exposure".into(), settings: p.develop.clone() }];
    p.versions = vec![Version { name: "V".into(), created: "2026-10-10".into(), settings: p.develop.clone(), auto: false }];
    p.import_look = Some(edited(|s| s.light.contrast = 20.0));
    let c = catalog_of([p]);
    let r = record(&c);
    let exposure = serde_json::json!({"light": {"exposure": 0.3}});
    assert_eq!(r["develop"], exposure);
    assert_eq!(r["history"][0]["settings"], exposure);
    assert_eq!(r["versions"][0]["settings"], exposure);
    assert_eq!(r["import_look"], serde_json::json!({"light": {"contrast": 20.0}}));
    assert_eq!(reopened(&c), c);
}

/// The fixture really sets every field: a field added to [`Photo`] or [`Meta`] must be added to
/// it (and to the stored form) before this passes.
#[test]
fn the_full_fixture_sets_every_field() {
    let (f, b) = (serde_json::to_value(full()).unwrap(), serde_json::to_value(plain(1)).unwrap());
    for (k, v) in f.as_object().unwrap() {
        if !["format", "width", "height", "imported"].contains(&k.as_str()) {
            assert_ne!(Some(v), b.get(k), "the fixture leaves `{k}` at its default");
        }
    }
    let empty = serde_json::to_value(Meta::default()).unwrap();
    for (k, v) in f["meta"].as_object().unwrap() {
        assert_ne!(Some(v), empty.get(k), "the fixture leaves `meta.{k}` at its default");
    }
    // fields left out of the public form when unset are set too
    for k in ["local", "content_hash", "embedded_lens", "copy_of", "copy_name", "import_look", "analysis", "preview_only", "local_baseline"] {
        assert!(f.get(k).is_some(), "`{k}`");
    }
    for k in ["copyright_status", "usage_terms", "copyright_url", "regions"] {
        assert!(f["meta"].get(k).is_some(), "`meta.{k}`");
    }
}

/// Given a photo with the fields no fixture's format has (a focus point, the build that checked a
/// preview-only raw), when the library is saved and reopened, then the stored form keeps each of
/// them.
#[test]
fn fields_since_the_fixtures_survive_the_stored_form() {
    let mut p = plain(1);
    p.meta.focus = Some(lightcraft_meta::FocusPoint {
        rect: lightcraft_meta::Rect { x0: 0.4, y0: 0.4, x1: 0.5, y1: 0.5 },
        mode: Some("AF-C".into()),
        area: Some("Wide".into()),
        tracking: None,
    });
    p.preview_checked = "0.8.0".into();
    let c = catalog_of([p]);
    let r = record(&c);
    assert_eq!(r["meta"]["focus"]["mode"], "AF-C");
    assert_eq!(r["preview_checked"], "0.8.0");
    assert_eq!(reopened(&c), c);
}

/// Given a photo with everything set, when the library is saved and reopened, nothing is lost.
#[test]
fn a_photo_with_everything_set_reads_back_exactly() {
    let c = catalog_of([full(), plain(8)]);
    assert_eq!(reopened(&c), c);
}

/// Every change that carries a photo, settings or metadata reads back from the log as it was.
#[test]
fn log_records_read_back_exactly_and_are_stored_compactly() {
    let p = full();
    let ops = [
        Op::AddPhoto { photo: Box::new(p.clone()) },
        Op::AddPhoto { photo: Box::new(plain(9)) },
        Op::SetDevelop { id: p.id, settings: p.develop.clone(), label: "Exposure".into(), edited: None },
        Op::SetDevelop { id: p.id, settings: Arc::new(DevelopSettings::default()), label: "Reset".into(), edited: None },
        Op::SetMeta { id: p.id, meta: Box::new(p.meta.clone()) },
        Op::SetMeta { id: p.id, meta: Box::default() },
        Op::SetVersions { id: p.id, versions: p.versions.clone() },
        Op::SetHistory { id: p.id, history: p.history.clone() },
        Op::PushHistory { id: p.id, step: p.history[0].clone() },
        Op::Batch { ops: vec![Op::AddPhoto { photo: Box::new(plain(10)) }] },
    ];
    for (i, op) in ops.iter().enumerate() {
        let line = encode_record(i as u64 + 1, op);
        assert_eq!(journal::decode_record(&line), Some((i as u64 + 1, op.clone())), "{line}");
    }
    let reset = encode_record(1, &ops[3]);
    assert!(reset.len() < 200, "a reset stores no settings: {reset}");
    let added = encode_record(1, &ops[1]);
    assert!(added.len() < 320, "{added}");
    let cleared = encode_record(1, &ops[5]);
    assert!(cleared.len() < 80, "{cleared}");
}

/// Settings made before process versions existed leave the process out and read back as the
/// first process; settings on any other process state it. Neither may be confused with the other.
#[test]
fn the_rendering_process_survives() {
    let on = |process: &str| -> Arc<DevelopSettings> { Arc::new(serde_json::from_str(process).unwrap()) };
    for json in ["{}", r#"{"process":1}"#, r#"{"process":7}"#, r#"{"process":7,"light":{"exposure":1.0}}"#] {
        let mut p = plain(1);
        p.develop = on(json);
        let c = catalog_of([p]);
        assert_eq!(reopened(&c), c, "{json}");
    }
    // a new photo: the default settings on the default process, stating only what that needs
    let mut p = plain(1);
    p.develop = Arc::new(DevelopSettings::default());
    let c = catalog_of([p]);
    assert!(record(&c)["develop"].as_object().unwrap().keys().all(|k| k == "process"), "{}", record(&c)["develop"]);
    assert_eq!(reopened(&c), c);
}

/// A value that is read all or nothing is stored whole once any of it differs: a crop rectangle
/// with one edge moved keeps its other edges and its angle (left out one by one, the settings
/// would not read at all).
#[test]
fn a_value_read_all_or_nothing_is_stored_whole() {
    let mut v = serde_json::to_value(base()).unwrap();
    v["crop"]["geometry"]["rect"]["x0"] = serde_json::json!(0.25);
    let mut p = plain(1);
    p.develop = Arc::new(serde_json::from_value(v).unwrap());
    let c = catalog_of([p]);
    assert_eq!(
        record(&c)["develop"],
        serde_json::json!({"crop": {"geometry": {"angle": 0.0, "rect": {"x0": 0.25, "x1": 1.0, "y0": 0.0, "y1": 1.0}}}})
    );
    assert_eq!(reopened(&c), c);
}

/// Changing any one number, switch or text of the develop settings is stored and read back.
#[test]
fn every_develop_setting_survives_on_its_own() {
    fn leaves(v: &Value, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        match v {
            Value::Object(m) if !m.is_empty() => {
                for (k, x) in m {
                    path.push(k.clone());
                    leaves(x, path, out);
                    path.pop();
                }
            }
            _ => out.push(path.clone()),
        }
    }
    fn at<'a>(v: &'a mut Value, path: &[String]) -> &'a mut Value {
        path.iter().fold(v, |v, k| &mut v[k.as_str()])
    }
    let default = serde_json::to_value(DevelopSettings::default()).unwrap();
    let mut paths = Vec::new();
    leaves(&default, &mut Vec::new(), &mut paths);
    assert!(paths.len() > 100, "{}", paths.len());
    let mut tried = 0;
    for path in &paths {
        let mut changed = Vec::new();
        match at(&mut default.clone(), path).clone() {
            Value::Number(n) => {
                let x = n.as_f64().unwrap();
                changed.extend([
                    serde_json::json!(x + 1.0),
                    serde_json::json!(x - 1.0),
                    serde_json::json!(0),
                    serde_json::json!(0.25),
                    serde_json::json!(-0.0),
                ]);
            }
            Value::Bool(b) => changed.push(Value::Bool(!b)),
            Value::String(s) => changed.push(Value::String(format!("{s}x"))),
            // lists and optional values: see `rich_develop_settings_survive`
            Value::Array(_) | Value::Null | Value::Object(_) => {}
        }
        for value in changed {
            let mut v = default.clone();
            *at(&mut v, path) = value;
            // values the settings don't accept there are not settings
            let Ok(s) = serde_json::from_value::<DevelopSettings>(v) else { continue };
            tried += 1;
            let mut p = plain(1);
            p.develop = Arc::new(s);
            p.history = vec![HistoryStep { label: "x".into(), settings: p.develop.clone() }];
            let c = catalog_of([p]);
            assert_eq!(reopened(&c), c, "{}", path.join("."));
        }
    }
    assert!(tried > 150, "{tried}");
}

/// Settings with lists, choices and optional values (masks, curve points, guides, the Upright
/// transform, a locked crop aspect, spots, red eye) read back exactly, in a photo and in the log.
#[test]
fn rich_develop_settings_survive() {
    let rich = Arc::new(rich());
    let v = serde_json::to_value(rich.as_ref()).unwrap();
    for list in ["masks", "spots", "red_eye", "point_colors", "disabled_sections"] {
        assert!(v[list].as_array().is_some_and(|a| !a.is_empty()), "`{list}`");
    }
    assert!(!v["geometry"]["upright_transform"].is_null() && !v["crop"]["aspect"].is_null());
    let mut p = plain(1);
    p.develop = rich.clone();
    p.history = vec![HistoryStep { label: "Masks".into(), settings: rich.clone() }];
    let c = catalog_of([p]);
    assert_eq!(reopened(&c), c);
    let op = Op::SetDevelop { id: PhotoId(1), settings: rich, label: "Masks".into(), edited: None };
    assert_eq!(journal::decode_record(&encode_record(1, &op)), Some((1, op)));
}

/// A negative zero is a different number in the saved JSON (a camera's as-shot tint of -0.3
/// rounds to it), and a photo's fingerprint and preview-cache key are made from that JSON: it is
/// stored, also where everything else is the default. Otherwise an untouched Local record would
/// count as changed once the library was reopened.
#[test]
fn a_negative_zero_survives() {
    // on its own, where the default is 0.0
    let mut p = plain(1);
    p.develop = edited(|s| s.wb.tint = -0.0);
    assert_ne!(p.develop.hash64(), base().hash64());
    let c = catalog_of([p]);
    assert_eq!(record(&c)["develop"].to_string(), r#"{"wb":{"tint":-0.0}}"#);
    assert_eq!(reopened(&c), c);

    // a browsed raw whose as-shot tint rounded to -0.0: still untouched after reopening
    let mut p = plain(2);
    p.kind = MediaKind::Raw;
    p.format = "DNG".into();
    p.as_shot_wb = Some((5200.0, (-0.3f64).round()));
    p.develop = edited(|s| {
        s.wb.temp = 5200.0;
        s.wb.tint = (-0.3f64).round();
        s.detail.sharpen_amount = 40.0;
    });
    p.local = true;
    p.set_local_baseline();
    assert!(p.untouched_local());
    let c = catalog_of([p.clone()]);
    let r = reopened(&c);
    assert!(r.photos().next().unwrap().untouched_local());

    // settings equal to the ones just stored, but for the sign of a zero, are not taken for them
    let mut q = plain(3);
    q.develop = edited(|s| {
        s.wb.temp = 5200.0;
        s.wb.tint = 0.0;
        s.detail.sharpen_amount = 40.0;
    });
    let c = catalog_of([q, p.clone(), plain(4)]);
    assert_eq!(reopened(&c), c);

    // and in the log
    let op = Op::SetDevelop { id: PhotoId(2), settings: p.develop.clone(), label: "x".into(), edited: None };
    let (_, back) = journal::decode_record(&encode_record(1, &op)).unwrap();
    let Op::SetDevelop { settings, .. } = back else { panic!() };
    assert_eq!(settings.hash64(), p.develop.hash64());
}

/// What other parts of the program read (the photo as JSON, and the Local browse fingerprint
/// made from it) is not the stored form: it still states every field.
#[test]
fn the_public_form_of_a_photo_still_states_every_field() {
    let v = serde_json::to_value(plain(1)).unwrap();
    for k in
        ["kind", "file_size", "captured", "meta", "rating", "flag", "label", "edited", "versions", "history", "deleted", "duration", "as_shot_wb"]
    {
        assert!(v.get(k).is_some(), "`{k}`");
    }
    assert_eq!(v["develop"], serde_json::to_value(base()).unwrap());
    assert_eq!(v["meta"], serde_json::to_value(Meta::default()).unwrap());
}

/// A Local record browsed by an earlier version still counts as untouched: its fingerprint is
/// made from the public form, which did not change. (The number is what the format 6 build
/// computes for this record.)
#[test]
fn local_fingerprints_are_unchanged() {
    let mut p = plain(1);
    p.local = true;
    assert_eq!(p.fingerprint(), FINGERPRINT_OF_PLAIN_LOCAL_1);
}
const FINGERPRINT_OF_PLAIN_LOCAL_1: u64 = 18_354_247_549_712_272_181;

/// Given a library saved by the format 6 build (every field of every record written out; the
/// files in `fixtures/` were written by that build from the fixtures above, with a log of later
/// changes), when it is opened, then it reads as it was and is rewritten in the compact form.
#[test]
fn a_v6_library_loads_and_is_upgraded() {
    let old = include_str!("fixtures/v6-catalog.snap.json");
    let log = include_str!("fixtures/v6-catalog.log.jsonl");
    assert!(old.starts_with(r#"{"format":"lightcraft-catalog","version":6,"#) && log.lines().count() == later_ops().len());
    let mut rich_photo = plain(9);
    rich_photo.develop = Arc::new(rich());
    rich_photo.local = true;
    rich_photo.set_local_baseline();
    let mut expected = catalog_of([full(), plain(8), rich_photo]);
    for op in later_ops() {
        expected.apply(op).unwrap();
    }

    let m = MemStore::new();
    m.set(SNAPSHOT, old.as_bytes().to_vec());
    m.set(LOG, log.as_bytes().to_vec());
    let (_, mut loaded, r) = Journal::open(Box::new(m.clone())).unwrap();
    assert_eq!((r.replayed, r.failed, r.upgraded_from), (later_ops().len(), 0, Some(6)));
    loaded.revision = expected.revision;
    assert_eq!(loaded, expected);
    let public = |c: &Catalog| c.photos().map(|p| serde_json::to_string(p.as_ref()).unwrap()).collect::<Vec<_>>();
    assert_eq!(public(&loaded), public(&expected));
    // the Local record browsed with format 6 is still untouched (rating it since made it touched
    // in both: compare the fingerprint of its state)
    assert_eq!(loaded.photo(PhotoId(9)).unwrap().fingerprint(), expected.photo(PhotoId(9)).unwrap().fingerprint());

    // rewritten in this format, compactly, and it reads the same again
    let new = m.get(SNAPSHOT).unwrap();
    let head: Value = serde_json::from_slice(&new).unwrap();
    assert_eq!(head["version"].as_u64(), Some(u64::from(VERSION)));
    assert!(m.get(LOG).unwrap().is_empty());
    assert!(new.len() * 2 < old.len() + log.len(), "{} + {} -> {}", old.len(), log.len(), new.len());
    let (_, mut again, r) = Journal::open(Box::new(m)).unwrap();
    again.revision = expected.revision;
    assert_eq!((again, r.upgraded_from), (expected, None));
}

/// The stored form leaves out values equal to the defaults, so a library depends on what a
/// missing value reads as: if that changed, every photo relying on it would silently change.
/// `develop_defaults.json` is what settings with nothing stated (`{}`) read as.
///
/// If this fails because a setting was **added**, add it to the file (the test prints what it
/// reads now). If what a missing value reads as **changed**, or a setting was removed or renamed:
/// that is a new catalog format ([`journal::VERSION`]) whose loader gives the photos of older
/// libraries the old value; change the file with it. (A new default rendering process changes
/// nothing here: settings without a process stay on the first one, and photos on the default
/// one state it.)
#[test]
fn what_missing_develop_settings_read_as_is_unchanged() {
    let pinned: Value = serde_json::from_str(include_str!("develop_defaults.json")).unwrap();
    let now = serde_json::to_value(base()).unwrap();
    // (as text too: `Value`'s own comparison takes -0.0 for 0.0)
    assert_eq!(now.to_string(), pinned.to_string(), "see this test's comment");
    assert_eq!(
        now,
        pinned,
        "what missing develop settings read as changed; see this test's comment. They read as:\n{}",
        serde_json::to_string_pretty(&now).unwrap()
    );
}
