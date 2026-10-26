//! Text typed in a Settings box is kept when a command (a menu item, a shortcut or the control
//! channel's `app.settings`) opens another tab while the box is still being typed in.
//!
//! Before: the command changed the tab before the frame was drawn, so the page with the box was
//! never drawn again, the box never saw its focus go, and what was typed in Copyright or Creator
//! was dropped.

use std::time::Duration;

use serde_json::{Value, json};

use crate::headless::Headless;
use crate::state::Dialog;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);

fn demo() -> Headless {
    let mut h = Headless::new(LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default()), [1200.0, 900.0], 1.0);
    // let the first renders finish (a render still running at exit can crash in GPU teardown)
    h.settle(T);
    h
}

fn steps(h: &mut Headless) {
    for _ in 0..4 {
        h.step();
    }
}

/// The command `app.settings` for `tab`, as a menu item or the control channel runs it.
fn show(h: &mut Headless, tab: &str) {
    let r = h.request("engine.execute", json!({"command": "app.settings", "params": {"tab": tab}}), T);
    assert_eq!(r["ok"], true, "app.settings {tab}: {r}");
    h.settle(T);
    steps(h);
}

fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id": id}), T);
    assert_eq!(r["ok"], true, "click {id}: {r}");
    steps(h);
}

fn type_into(h: &mut Headless, id: &str, text: &str) {
    click(h, id);
    h.request("ui.text", json!({"text": text}), T);
    steps(h);
}

fn says(h: &Headless, part: &str) -> bool {
    h.painted_text().iter().any(|text| text.contains(part))
}

fn settings_open(h: &Headless, tab: &str) -> bool {
    h.app.ui.dialog == Some(Dialog::Settings { tab: tab.into() })
}

fn import(h: &mut Headless, key: &str) -> Value {
    h.app.run("library.preferences", json!({})).unwrap()["import"][key].clone()
}

#[test]
fn text_typed_in_copyright_is_saved_when_a_command_opens_another_tab() {
    let mut h = demo();
    show(&mut h, "import");
    type_into(&mut h, "field:settings.copyright", "Example Name");
    assert!(says(&h, "Example Name"), "the typing shows");
    // When a command opens another tab with the text still being typed
    show(&mut h, "general");
    // Then that tab is open and what was typed was saved, as leaving the box any other way saves it
    assert!(settings_open(&h, "general"), "{:?}", h.app.ui.dialog);
    assert_eq!(import(&mut h, "copyright"), json!("Example Name"), "the text typed in Copyright was dropped");
    // and the new tab's page is the one drawn after the one frame
    assert!(!says(&h, "Copyright"), "the Import page is still drawn: {:?}", h.painted_text());
    assert!(says(&h, "Language"), "the General page is drawn: {:?}", h.painted_text());
}

#[test]
fn text_typed_in_creator_is_saved_when_a_command_opens_another_tab() {
    let mut h = demo();
    show(&mut h, "import");
    type_into(&mut h, "field:settings.creator", "Example Creator");
    show(&mut h, "faces");
    assert!(settings_open(&h, "faces"), "{:?}", h.app.ui.dialog);
    assert_eq!(import(&mut h, "creator"), json!("Example Creator"), "the text typed in Creator was dropped");
    assert!(!says(&h, "Copyright"), "the Import page is still drawn: {:?}", h.painted_text());
}

#[test]
fn text_typed_in_copyright_is_saved_when_slideshow_options_is_chosen() {
    let mut h = demo();
    show(&mut h, "import");
    type_into(&mut h, "field:settings.copyright", "Example Name");
    // When Slideshow Options (a menu item) opens its tab
    let r = h.request("engine.execute", json!({"command": "view.slideshowOptions", "params": {}}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(T);
    steps(&mut h);
    // Then that tab is open and the text typed in Copyright was saved
    assert!(settings_open(&h, "slideshow"), "{:?}", h.app.ui.dialog);
    assert_eq!(import(&mut h, "copyright"), json!("Example Name"), "the text typed in Copyright was dropped");
    assert!(!says(&h, "Copyright"), "the Import page is still drawn: {:?}", h.painted_text());
}

#[test]
fn a_command_for_the_tab_already_open_ends_no_edit() {
    let mut h = demo();
    show(&mut h, "import");
    let before = import(&mut h, "copyright");
    type_into(&mut h, "field:settings.copyright", "Example Name");
    // When the command names the tab that is open, nothing changes: the edit goes on
    show(&mut h, "import");
    assert!(settings_open(&h, "import"));
    assert!(says(&h, "Example Name"), "the typing stays in the box");
    assert_eq!(import(&mut h, "copyright"), before, "an unfinished edit is not saved");
}

#[test]
fn two_commands_before_a_frame_keep_the_text_and_end_on_the_last_tab() {
    let mut h = demo();
    show(&mut h, "import");
    type_into(&mut h, "field:settings.copyright", "Example Name");
    // When two commands change the tab before the next frame is drawn
    for tab in ["general", "faces"] {
        h.app.run("app.settings", json!({"tab": tab})).unwrap();
    }
    h.settle(T);
    steps(&mut h);
    assert!(settings_open(&h, "faces"), "{:?}", h.app.ui.dialog);
    assert_eq!(import(&mut h, "copyright"), json!("Example Name"), "the text typed in Copyright was dropped");
    assert!(!says(&h, "Copyright"), "the Import page is still drawn: {:?}", h.painted_text());
}

#[test]
fn with_nothing_typed_a_command_shows_its_tab_in_the_frame_it_runs_in() {
    let mut h = demo();
    // Given Settings closed, a command opens it on a tab: that page is the one drawn
    h.app.run("app.settings", json!({"tab": "import"})).unwrap();
    h.settle(T);
    steps(&mut h);
    assert!(settings_open(&h, "import"));
    assert!(says(&h, "Copyright"), "the Import page is drawn: {:?}", h.painted_text());
    // And with no text box being typed in, a command for another tab shows that page in the very
    // next frame (not one frame later, as with a box being typed in)
    h.app.run("app.settings", json!({"tab": "general"})).unwrap();
    h.step();
    assert!(settings_open(&h, "general"));
    assert!(!says(&h, "Copyright"), "the Import page is still drawn: {:?}", h.painted_text());
}

#[test]
fn only_a_tab_left_in_the_frame_before_while_typing_is_drawn_again() {
    use crate::panels::settings::tab_being_left as left;
    let last = || Some((6, "import".to_string()));
    // the one case: drawn in the frame before, another tab now, a box being typed in
    assert_eq!(left(last(), 7, "general", true), Some("import".to_string()));
    // each condition on its own: no box being typed in
    assert_eq!(left(last(), 7, "general", false), None, "nothing typed: the new tab at once");
    // the same tab: nothing was left
    assert_eq!(left(last(), 7, "import", true), None, "a command for the open tab ends no edit");
    // Settings last drawn long ago (a page left frames ago is not drawn again), or in this very frame
    assert_eq!(left(last(), 9, "general", true), None, "drawn frames ago");
    assert_eq!(left(last(), 6, "general", true), None, "second pass of the same frame");
    // never drawn before
    assert_eq!(left(None, 7, "general", true), None, "just opened");
}
