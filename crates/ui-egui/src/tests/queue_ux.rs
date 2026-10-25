//! The waiting line for imports and folder syncs, as the person sees it: every row says which folder it is, the
//! import and Synchronize dialogs say they will wait, a drop that is refused says why, and what the line does with its
//! tasks is told, never silent.

use std::time::Duration;

use lightcraft_engine::activity::Cancel;
use lightcraft_engine::import::ImportCandidate;
use serde_json::json;

use crate::headless::Headless;
use crate::import::ImportDialog;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(60);

fn app() -> Headless {
    let app = LightcraftApp::new(lightcraft_engine::Session::new(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.settle(T);
    h
}

/// A review of `n` photos from `folder`, as the scan hands it over.
fn review(folder: &str, n: usize) -> ImportDialog {
    let candidates = (0..n)
        .map(|i| ImportCandidate {
            path: format!("{folder}/IMG_{i:03}.jpg"),
            name: format!("IMG_{i:03}.jpg"),
            format: "JPEG".into(),
            ..Default::default()
        })
        .collect();
    let mut d = ImportDialog::new(candidates);
    d.sources = vec![folder.to_string()];
    d
}

fn sync_dialog(name: &str) -> crate::state::Dialog {
    crate::state::Dialog::SynchronizeFolder {
        path: format!("/photos/{name}"),
        name: name.into(),
        disk: false,
        counts: Some(crate::state::SyncCounts { new: 1, ..Default::default() }),
        import_new: true,
        relink_moved: true,
        remove_missing: false,
        read_metadata: false,
    }
}

fn says(h: &Headless, text: &str) -> bool {
    h.painted_text().iter().any(|t| t == text)
}

fn open_review(h: &mut Headless, d: ImportDialog) {
    h.app.ui.dialog = Some(crate::state::Dialog::Import { opts: Box::new(d) });
    for _ in 0..4 {
        h.step();
    }
}

/// Given an import running, when reviews of different folders are confirmed and a folder is dropped, then each waiting
/// task says which folder it is (several sources are counted rather than named) and what else is known: how many
/// photos, and how they come in.
#[test]
fn waiting_imports_say_which_folder_they_are() {
    let mut h = app();
    let _running = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    let mut move_it = review("/photos/Lisbon", 1);
    move_it.copy = true;
    move_it.move_files = true;
    let mut several = review("/photos/Faro", 2);
    several.sources = vec!["/photos/Faro".into(), "/photos/Oslo".into(), "/photos/Evora".into()];
    crate::import::start(&mut h.app, &review("/photos/Rome", 3)).expect("waits");
    crate::import::start(&mut h.app, &move_it).expect("waits");
    crate::import::start(&mut h.app, &several).expect("waits");
    crate::import::start_paths(&mut h.app, vec!["/photos/Porto/".into()]).expect("waits");
    let rows = h.app.session.activity.waiting();
    let said: Vec<(&str, &str)> = rows.iter().map(|r| (r.detail.as_str(), r.facts.as_str())).collect();
    assert_eq!(
        said,
        vec![("Rome", "3 photos · Add in place"), ("Lisbon", "1 photo · Move"), ("3 items", "2 photos · Add in place"), ("Porto", "Add in place")]
    );
}

/// Given a waiting import, then its row in the activity stack names the folder and says what is known beside when it
/// starts.
#[test]
fn a_waiting_row_gives_the_folder_and_what_is_known_beside_when_it_starts() {
    let mut h = app();
    let _running = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    crate::import::start(&mut h.app, &review("/photos/Rome", 3)).expect("waits");
    for _ in 0..4 {
        h.step();
    }
    let text = h.painted_text();
    assert!(text.iter().any(|t| t == "Rome · 3 photos · Add in place"), "{text:?}");
    assert!(text.iter().any(|t| t == " · Starts next"), "{text:?}");
    let said = crate::panels::activity::cross_name(&h.app.session.activity.waiting()[0]);
    assert_eq!(said, "Remove from Queue: Importing · Rome", "✕ is named for the folder");
}

/// Given an import that runs, then its row names its folder beside how far along it is.
#[test]
fn the_running_import_names_its_folder() {
    let dir = std::env::temp_dir().join(format!("lc-queue-ux-{}", std::process::id())).join("Rome");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..30 {
        std::fs::write(dir.join(format!("IMG_{i:03}.jpg")), format!("not really a jpeg {i}")).unwrap();
    }
    let mut s = lightcraft_engine::Session::new();
    s.media.file_probe = Some(std::sync::Arc::new(|p: &str| {
        std::thread::sleep(Duration::from_millis(100));
        Ok(lightcraft_engine::media::ProbeInfo {
            width: 60,
            height: 40,
            format: "JPEG".into(),
            content_hash: Some(p.to_string()),
            ..Default::default()
        })
    }));
    let mut h = Headless::new(LightcraftApp::new(s, Services { png: None, ..Default::default() }), [1200.0, 800.0], 1.0);
    h.settle(T);
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).expect("starts");
    assert!(h.step_until(T, |h| h.app.session.activity.list().iter().any(|t| t.kind == "import" && !t.waiting)), "the row appears");
    let row = h.app.session.activity.list().into_iter().find(|t| t.kind == "import").unwrap();
    assert_eq!(row.detail, "Rome");
    h.app.session.activity.cancel_all();
    h.step_until(T, |h| h.app.import.is_none());
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// Given an import that waited, when it starts, then its running row keeps naming the folder.
#[test]
fn an_import_that_waited_names_its_folder_when_it_runs() {
    let dir = std::env::temp_dir().join(format!("lc-queue-ux-waited-{}", std::process::id())).join("Rome");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..30 {
        std::fs::write(dir.join(format!("IMG_{i:03}.jpg")), format!("not really a jpeg {i}")).unwrap();
    }
    let mut s = lightcraft_engine::Session::new();
    s.media.file_probe = Some(std::sync::Arc::new(|p: &str| {
        std::thread::sleep(Duration::from_millis(100));
        Ok(lightcraft_engine::media::ProbeInfo {
            width: 60,
            height: 40,
            format: "JPEG".into(),
            content_hash: Some(p.to_string()),
            ..Default::default()
        })
    }));
    let mut h = Headless::new(LightcraftApp::new(s, Services { png: None, ..Default::default() }), [1200.0, 800.0], 1.0);
    h.settle(T);
    let hold = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).expect("waits");
    drop(hold);
    assert!(h.step_until(T, |h| h.app.session.activity.list().iter().any(|t| t.kind == "import" && !t.waiting)), "it starts");
    let row = h.app.session.activity.list().into_iter().find(|t| t.kind == "import").unwrap();
    assert_eq!(row.detail, "Rome");
    h.app.session.activity.cancel_all();
    h.step_until(T, |h| h.app.import.is_none());
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// Given an import running, when a Synchronize Folder is confirmed, then its waiting row names the folder.
#[test]
fn a_waiting_synchronize_names_its_folder() {
    let mut h = app();
    let _running = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    let r = crate::panels::dialogs::confirm_dialog(&mut h.app, &sync_dialog("Rome"));
    assert!(r.as_ref().is_ok_and(|v| v.get("queued").is_some()), "{r:?}");
    let rows = h.app.session.activity.waiting();
    assert_eq!((rows[0].kind, rows[0].detail.as_str(), rows[0].facts.as_str()), ("sync", "Rome", ""));
}

/// Given the library lane free, then the review's button says "Import" and nothing says it will wait. Given an
/// import running, then it says "Queue" and names what it starts after; a move is told as one.
#[test]
fn the_review_says_when_the_import_will_wait() {
    let mut h = app();
    open_review(&mut h, review("/photos/Rome", 3));
    assert!(says(&h, "Import 3 Photos"), "{:?}", h.painted_text());
    assert!(!h.painted_text().iter().any(|t| t.starts_with("Starts after")), "{:?}", h.painted_text());
    let running = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    running.detail("Oslo");
    h.step();
    h.step();
    assert!(says(&h, "Queue 3 Photos"), "{:?}", h.painted_text());
    assert!(says(&h, "Starts after: Importing · Oslo"), "{:?}", h.painted_text());
    let mut d = review("/photos/Rome", 1);
    d.copy = true;
    d.move_files = true;
    open_review(&mut h, d);
    assert!(says(&h, "Queue Move 1 Photo"), "{:?}", h.painted_text());
}

/// Given a free lane, then nothing is ahead; given an import running with another waiting behind it, then a task
/// asked for now starts after the running one, as it is named on its row.
#[test]
fn what_a_new_task_starts_after_is_the_running_one() {
    let mut h = app();
    assert_eq!(crate::import::lane_ahead_text(&h.app), None);
    let hold = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    hold.detail("Oslo");
    crate::import::start(&mut h.app, &review("/photos/Lisbon", 1)).expect("waits");
    assert_eq!(crate::import::lane_ahead_text(&h.app).as_deref(), Some("Importing · Oslo"));
}

/// The same for the single-folder Synchronize dialog.
#[test]
fn the_synchronize_dialog_says_when_it_will_wait() {
    let mut h = app();
    h.app.ui.dialog = Some(sync_dialog("Rome"));
    h.step();
    h.step();
    assert!(says(&h, "Synchronize"), "{:?}", h.painted_text());
    let _running = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    h.step();
    h.step();
    assert!(says(&h, "Queue Synchronize"), "{:?}", h.painted_text());
    assert!(says(&h, "Starts after: Importing"), "{:?}", h.painted_text());
}
