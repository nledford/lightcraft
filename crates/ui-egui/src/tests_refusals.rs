//! A command the interface runs and the library refuses says why, where the person looks: a
//! toast. `LightcraftApp::act` is how the interface runs a command for someone's click, drop or
//! key; the reason is shown without each widget having to. `run` stays the raw entry (agents and
//! code that handles the answer itself get the error back and nothing is shown), and `quiet` is
//! for the few places where a refusal is expected and means nothing to the person.
//!
//! Scenarios:
//! - Given a command the library refuses (a smart album into the folder it shows), when the
//!   interface runs it for the person, then a toast gives the reason in words, without the
//!   command's id, and stays long enough to read.
//! - Given a command that isn't available right now (rating with nothing selected), then the
//!   toast says why.
//! - Given a command that goes through, then nothing is said that the command didn't say itself.
//! - Given the same refusals through `run` or `quiet`, then nothing is shown; `run` gives the
//!   error back in full and agents can still read it as the status.
//! - Given the Albums sidebar, when a smart album is dropped where it can't go by a path the
//!   interface didn't rule out beforehand, then the person is told (the widget passes nothing on
//!   by hand).

use serde_json::json;

use crate::{LightcraftApp, Services};

fn app() -> LightcraftApp {
    LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() })
}

/// A folder and a smart album that shows it: moving the album into the folder is refused.
fn refused_move(app: &mut LightcraftApp) -> serde_json::Value {
    let trips = app.session.execute("album.create", &json!({"name": "Trips", "folder": true})).unwrap()["id"].as_u64().unwrap();
    let view = app.session.execute("album.createSmart", &json!({"name": "Trips view", "rules": {"album": trips}})).unwrap()["id"].as_u64().unwrap();
    json!({"id": view, "parent": trips})
}

fn toast(app: &LightcraftApp) -> Option<&str> {
    app.ui.toast.as_ref().map(|t| t.0.as_str())
}

#[test]
fn a_refused_command_says_why_in_a_toast() {
    let mut app = app();
    let params = refused_move(&mut app);
    assert_eq!(app.act("album.move", params), None);
    let said = toast(&app).expect("a toast");
    assert!(said.contains("Trips view") && said.contains("include itself"), "{said}");
    assert!(!said.contains("album.move") && !said.contains("invalid parameters"), "words for a person, not the command's id: {said}");
    let (_, until, _) = app.ui.toast.clone().unwrap();
    assert!(until - app.last_time >= 5.0, "long enough to read a sentence");
    // agents still find the full error as the status
    assert!(app.ui.status.contains("album.move"), "{}", app.ui.status);
}

#[test]
fn a_command_that_isnt_available_says_why() {
    let mut app = app();
    app.session.selection = Default::default();
    assert_eq!(app.act("photo.rate", json!({"rating": 3})), None);
    let said = toast(&app).expect("a toast");
    assert!(said.to_lowercase().contains("no photo"), "{said}");
    assert!(!said.contains("photo.rate"), "{said}");
}

#[test]
fn a_command_that_goes_through_adds_nothing() {
    let mut app = app();
    let made = app.act("album.create", json!({"name": "Made"}));
    assert!(made.is_some_and(|v| v["id"].is_u64()), "its result comes back");
    assert_eq!(toast(&app), None);
}

#[test]
fn run_and_quiet_show_nothing() {
    let mut app = app();
    let params = refused_move(&mut app);
    let e = app.run("album.move", params.clone()).unwrap_err();
    assert!(e.contains("album.move") && e.contains("include itself"), "the caller gets the whole error: {e}");
    assert_eq!(toast(&app), None, "run: the caller decides what to show");
    assert_eq!(app.quiet("album.move", params), None);
    assert_eq!(toast(&app), None, "quiet: a refusal expected here");
    assert!(app.ui.status.contains("include itself"), "both leave the status for agents");
}

/// A click on a menu item goes the same way, whatever the item (before, only an export that
/// couldn't start said so).
#[test]
fn a_refused_menu_item_says_why() {
    let mut app = app();
    let params = refused_move(&mut app);
    assert_eq!(crate::menubar::act_item(&mut app, "album.move", params.clone()), None);
    assert!(toast(&app).is_some_and(|t| t.contains("include itself")), "{:?}", toast(&app));
    // `run_item` stays the raw form: the error back, nothing shown
    app.ui.toast = None;
    assert!(crate::menubar::run_item(&mut app, "album.move", params).is_err());
    assert_eq!(toast(&app), None);
    // an item that goes through answers as before
    assert!(crate::menubar::act_item(&mut app, "album.create", json!({"name": "From the menu"})).is_some());
    assert_eq!(toast(&app), None);
}

#[test]
fn an_unknown_command_is_said_too() {
    let mut app = app();
    assert_eq!(app.act("no.suchCommand", json!({})), None);
    assert!(toast(&app).is_some_and(|t| t.contains("no.suchCommand")), "{:?}", toast(&app));
}
