//! Photos dragged from the grid step back while the drag lasts: their thumbnails are painted darker,
//! the others stay as they are, and the grid is as before once the drag ends.

use std::time::Duration;

use egui::{Color32, ColorImage, Pos2};
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use serde_json::json;

use crate::headless::Headless;
use crate::state::ViewMode;
use crate::{LightcraftApp, Services};

const SETTLE: Duration = Duration::from_secs(120);

fn app(view: ViewMode) -> Headless {
    let mut s = lightcraft_engine::Session::new();
    for id in 1..=6 {
        let p = Photo::new(PhotoId(id), Source::Demo { scene: 1 }, &format!("{id}.jpg"), "JPEG", 640, 480, "2026-10-08");
        s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    s.execute("library.sort", &json!({"key": "fileName", "ascending": true, "group": "none"})).unwrap();
    // two photos selected: the drag carries both
    s.execute("library.select", &json!({"ids": [1, 2]})).unwrap();
    let mut h = Headless::new(LightcraftApp::new(s, Services { png: None, ..Default::default() }), [1200.0, 800.0], 1.0);
    h.app.ui.view = view;
    h.app.ui.right = crate::state::RightPanel::None;
    h.app.ui.thumb_size = 160.0;
    // the thumbnails' real textures, so a darker pixel is the dimming and not a placeholder
    assert!(h.step_until(SETTLE, |h| h.app.renderer.thumb_textures() == 6), "all fixture thumbnails must be ready");
    assert!(h.settle(SETTLE), "fixture did not settle");
    h
}

fn centre(h: &Headless, id: u64) -> Pos2 {
    h.app.widgets.iter().find(|(w, _)| *w == format!("thumb:{id}")).unwrap().1.center()
}

fn pixel(image: &ColorImage, p: Pos2) -> Color32 {
    image.pixels[p.y as usize * image.size[0] + p.x as usize]
}

fn brightness(c: Color32) -> u32 {
    u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b())
}

fn pointer(h: &mut Headless, at: Pos2, pressed: bool) {
    h.app.synthetic.push(egui::Event::PointerMoved(at));
    h.app.synthetic.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    h.step();
}

#[test]
fn the_photos_being_dragged_are_dimmed_and_only_those() {
    for view in [ViewMode::SquareGrid, ViewMode::PhotoGrid] {
        let mut h = app(view);
        let before = h.paint();
        // When photo 1 (of the selected 1 and 2) is picked up and carried over the grid
        let (from, to) = (centre(&h, 1), centre(&h, 6));
        pointer(&mut h, from, true);
        for i in 1..=8 {
            h.app.synthetic.push(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
            h.step();
        }
        assert_eq!(h.app.ui.dragging_photos, Some(vec![1, 2]), "{view:?}: the drag carries the selection");
        let during = h.paint();
        // Then both are darker, and a photo that isn't in it is untouched
        for dragged in [1, 2] {
            let at = centre(&h, dragged);
            assert!(brightness(pixel(&during, at)) < brightness(pixel(&before, at)), "{view:?}: photo {dragged} is dimmed");
        }
        let other = centre(&h, 3);
        assert_eq!(pixel(&during, other), pixel(&before, other), "{view:?}: a photo that isn't dragged keeps its colours");
        // and a change of the selection in mid-drag moves nothing: what is dimmed is what a drop would carry
        h.app.run("library.select", json!({"ids": [3]})).unwrap();
        h.step();
        let reselected = h.paint();
        assert_eq!(h.app.ui.dragging_photos, Some(vec![1, 2]), "{view:?}: the drag still carries photos 1 and 2");
        for dragged in [1, 2] {
            let at = centre(&h, dragged);
            assert!(brightness(pixel(&reselected, at)) < brightness(pixel(&before, at)), "{view:?}: photo {dragged} is still dimmed");
        }
        assert_eq!(pixel(&reselected, other), pixel(&before, other), "{view:?}: photo 3 is selected but not dragged, and isn't dimmed");
        // and once the button is let go, they are as they were (with the selection they had, which tints a thumbnail itself)
        h.app.run("library.select", json!({"ids": [1, 2]})).unwrap();
        pointer(&mut h, to, false);
        // (the frame that sees the release ends the drag after the grid is painted, so the next one paints it undimmed)
        h.step();
        h.step();
        assert!(h.app.ui.dragging_photos.is_none(), "{view:?}: the drag ended");
        let after = h.paint();
        for id in [1, 2, 3] {
            let at = centre(&h, id);
            assert_eq!(pixel(&after, at), pixel(&before, at), "{view:?}: photo {id} is back to normal");
        }
    }
}
