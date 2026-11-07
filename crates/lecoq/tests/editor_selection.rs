//! Selection and stacking (#27): Shift+click and the rubber band, editing a multi-selection,
//! Ctrl+D, z-order, and each tool remembering its style (§4.1–§4.3, §8.3).

mod support;

use std::sync::Arc;

use egui::{Key, Modifiers};
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationId, Property, Rgba};
use lecoq_platform::{Config, MemoryConfigStore, Platform};
use support::TestApp;

/// Rectangle A from (40, 30) to (140, 90) and B from (180, 120) to (260, 200) on a 320×240 Image,
/// drawn in that order, with the select tool active and nothing selected.
fn app_with_two_rectangles() -> TestApp {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.drag_px((180.0, 120.0), (260.0, 200.0));
    app.press(Key::Escape);
    app.click_px((300.0, 10.0));
    assert!(app.doc().selection().is_empty());
    app
}

fn ids(app: &TestApp) -> Vec<AnnotationId> {
    app.doc().annotations().iter().map(|a| a.id).collect()
}

fn selected(app: &TestApp) -> Vec<AnnotationId> {
    let mut s = app.doc().selection().to_vec();
    s.sort();
    s
}

fn rect_of(app: &TestApp, id: AnnotationId) -> [i32; 4] {
    let r = app.doc().annotation(id).expect("the Annotation").bounds();
    [
        r.min.x.round() as i32,
        r.min.y.round() as i32,
        r.max.x.round() as i32,
        r.max.y.round() as i32,
    ]
}

// ---- Selecting several ----

#[test]
fn shift_click_adds_to_the_selection() {
    let mut app = app_with_two_rectangles();
    let [a, b] = ids(&app)[..] else { panic!() };
    app.click_px((40.0, 60.0));
    app.click_px_with(Modifiers::SHIFT, (180.0, 160.0));

    assert_eq!(selected(&app), vec![a, b]);
    assert_eq!(app.selection_bounds(), Some([40, 30, 260, 200]));
}

#[test]
fn a_plain_click_replaces_the_selection() {
    let mut app = app_with_two_rectangles();
    let [_, b] = ids(&app)[..] else { panic!() };
    app.click_px((40.0, 60.0));
    app.click_px((180.0, 160.0));

    assert_eq!(selected(&app), vec![b]);
}

#[test]
fn shift_click_on_a_selected_annotation_takes_it_out_of_the_selection() {
    let mut app = app_with_two_rectangles();
    let [_, b] = ids(&app)[..] else { panic!() };
    app.click_px((40.0, 60.0));
    app.click_px_with(Modifiers::SHIFT, (180.0, 160.0));
    app.click_px_with(Modifiers::SHIFT, (40.0, 60.0));

    assert_eq!(selected(&app), vec![b]);
}

#[test]
fn shift_click_on_empty_space_keeps_the_selection() {
    let mut app = app_with_two_rectangles();
    let [a, _] = ids(&app)[..] else { panic!() };
    app.click_px((40.0, 60.0));
    app.click_px_with(Modifiers::SHIFT, (300.0, 10.0));

    assert_eq!(selected(&app), vec![a]);
}

#[test]
fn dragging_on_empty_space_selects_what_the_rubber_band_encloses() {
    let mut app = app_with_two_rectangles();
    let [a, b] = ids(&app)[..] else { panic!() };

    // Encloses A only (B is only partly inside).
    app.drag_px((20.0, 10.0), (200.0, 100.0));
    assert_eq!(selected(&app), vec![a]);

    // Encloses both.
    app.drag_px((300.0, 230.0), (10.0, 10.0));
    assert_eq!(selected(&app), vec![a, b]);

    // Encloses nothing: the selection is cleared.
    app.drag_px((290.0, 10.0), (310.0, 20.0));
    assert!(app.doc().selection().is_empty());

    // Selecting is not an edit.
    assert_eq!(app.doc().undo_len(), 2);
}

#[test]
fn shift_rubber_band_adds_to_the_selection() {
    let mut app = app_with_two_rectangles();
    let [a, b] = ids(&app)[..] else { panic!() };
    app.click_px((40.0, 60.0));
    app.drag_px_with(Modifiers::SHIFT, (170.0, 110.0), (270.0, 210.0));

    assert_eq!(selected(&app), vec![a, b]);
}

#[test]
fn the_rubber_band_is_drawn_while_dragging() {
    let mut app = app_with_two_rectangles();
    let from = app.screen(20.0, 10.0);
    let to = app.screen(200.0, 100.0);
    let h = &app.harness;
    h.hover_at(from);
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.hover_at(from + (to - from) * 0.5);
    h.hover_at(to);
    app.harness.run();

    let band = app.harness.state().rubber_band().expect("a rubber band");
    assert!((band.min - from).length() < 0.5);
    assert!((band.max - to).length() < 0.5);

    app.harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    app.settle();
    assert_eq!(app.harness.state().rubber_band(), None);
}

// ---- Editing a multi-selection ----

fn app_with_both_selected() -> TestApp {
    let mut app = app_with_two_rectangles();
    app.drag_px((300.0, 230.0), (10.0, 10.0));
    assert_eq!(app.doc().selection().len(), 2);
    app
}

#[test]
fn a_multi_selection_moves_together_as_one_step() {
    let mut app = app_with_both_selected();
    let [a, b] = ids(&app)[..] else { panic!() };
    app.drag_px((40.0, 60.0), (50.0, 80.0));

    assert_eq!(rect_of(&app, a), [50, 50, 150, 110]);
    assert_eq!(rect_of(&app, b), [190, 140, 270, 220]);
    assert_eq!(app.doc().undo_len(), 3);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(rect_of(&app, a), [40, 30, 140, 90]);
    assert_eq!(rect_of(&app, b), [180, 120, 260, 200]);
}

#[test]
fn a_multi_selection_is_deleted_together_as_one_step() {
    let mut app = app_with_both_selected();
    app.press(Key::Delete);

    assert!(app.doc().annotations().is_empty());
    assert_eq!(app.doc().undo_len(), 3);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.doc().annotations().len(), 2);
}

#[test]
fn a_multi_selection_is_restyled_together_as_one_step() {
    let mut app = app_with_both_selected();
    app.control("Thickness").set_value_accesskit(9.0);
    app.settle();

    for a in app.doc().annotations() {
        assert_eq!(a.style.thickness, 9.0);
    }
    assert_eq!(app.doc().undo_len(), 3);
}

// ---- Duplicate ----

#[test]
fn ctrl_d_duplicates_the_selection_offset_by_10_px_as_one_step_and_selects_the_copies() {
    let mut app = app_with_both_selected();
    let [a, b] = ids(&app)[..] else { panic!() };
    app.key_with(Modifiers::COMMAND, Key::D);

    let all = ids(&app);
    assert_eq!(all.len(), 4);
    assert_eq!(&all[..2], &[a, b]);
    let [c, d] = all[2..] else { panic!() };
    // The copies go on top, in the same order, offset by 10 px, and are selected.
    assert_eq!(rect_of(&app, c), [50, 40, 150, 100]);
    assert_eq!(rect_of(&app, d), [190, 130, 270, 210]);
    assert_eq!(selected(&app), vec![c, d]);
    assert_eq!(
        app.doc().annotation(c).unwrap().style,
        app.doc().annotation(a).unwrap().style
    );

    assert_eq!(app.doc().undo_len(), 3);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(ids(&app), vec![a, b]);
    assert_eq!(selected(&app), vec![a, b]);
}

#[test]
fn ctrl_d_with_nothing_selected_does_nothing() {
    let mut app = app_with_two_rectangles();
    app.key_with(Modifiers::COMMAND, Key::D);

    assert_eq!(app.doc().annotations().len(), 2);
    assert_eq!(app.doc().undo_len(), 2);
}

// ---- Stacking ----

/// Three rectangles 1, 2, 3 (bottom to top) and their ids.
fn app_with_three_rectangles() -> (TestApp, [AnnotationId; 3]) {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((20.0, 20.0), (100.0, 100.0));
    app.drag_px((60.0, 60.0), (140.0, 140.0));
    app.drag_px((100.0, 100.0), (180.0, 180.0));
    app.press(Key::Escape);
    let [one, two, three] = ids(&app)[..] else {
        panic!()
    };
    (app, [one, two, three])
}

/// Select only this one of the three rectangles, by clicking its bottom edge's right end (which
/// no other rectangle covers).
fn select_only(app: &mut TestApp, id: AnnotationId) {
    let r = app.doc().annotation(id).unwrap().bounds();
    app.click_px((300.0, 10.0));
    app.click_px((r.max.x - 10.0, r.max.y));
    assert_eq!(selected(app), vec![id]);
}

#[test]
fn new_annotations_go_on_top() {
    let (app, [one, two, three]) = app_with_three_rectangles();
    assert_eq!(ids(&app), vec![one, two, three]);
}

#[test]
fn bring_to_front_and_send_to_back_are_in_the_property_bar() {
    let (mut app, [one, two, three]) = app_with_three_rectangles();
    select_only(&mut app, one);
    app.control("Bring to front").click();
    app.settle();
    assert_eq!(ids(&app), vec![two, three, one]);

    select_only(&mut app, three);
    app.control("Send to back").click();
    app.settle();
    assert_eq!(ids(&app), vec![three, two, one]);

    assert_eq!(app.doc().undo_len(), 5);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(ids(&app), vec![two, three, one]);
}

#[test]
fn ctrl_brackets_move_the_selection_one_level() {
    let (mut app, [one, two, three]) = app_with_three_rectangles();
    select_only(&mut app, one);
    app.key_with(Modifiers::COMMAND, Key::CloseBracket);
    assert_eq!(ids(&app), vec![two, one, three]);
    app.key_with(Modifiers::COMMAND, Key::CloseBracket);
    assert_eq!(ids(&app), vec![two, three, one]);
    // Already on top: no change, and no step.
    let steps = app.doc().undo_len();
    app.key_with(Modifiers::COMMAND, Key::CloseBracket);
    assert_eq!(ids(&app), vec![two, three, one]);
    assert_eq!(app.doc().undo_len(), steps);

    app.key_with(Modifiers::COMMAND, Key::OpenBracket);
    assert_eq!(ids(&app), vec![two, one, three]);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(ids(&app), vec![two, three, one]);
}

// ---- Each tool remembers its style ----

fn app_with_config(text: Option<&str>) -> (TestApp, Arc<MemoryConfigStore>) {
    let store = Arc::new(match text {
        Some(text) => MemoryConfigStore::with_text(text),
        None => MemoryConfigStore::default(),
    });
    let mut app = TestApp::with_platform(Platform::in_memory().with_config(store.clone()));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    (app, store)
}

#[test]
fn restyling_a_selection_becomes_that_tools_default_and_is_saved_in_the_config() {
    let (mut app, store) = app_with_config(None);
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::Escape);
    app.click_px((40.0, 60.0));
    app.control("Thickness").set_value_accesskit(7.0);
    app.settle();
    app.control("Shadow").click();
    app.settle();

    // The next rectangle is drawn with the new style.
    app.press(Key::R);
    app.drag_px((180.0, 120.0), (260.0, 200.0));
    let style = &app.doc().annotations()[1].style;
    assert_eq!(style.thickness, 7.0);
    assert!(!style.shadow);

    let saved = Config::parse(&store.text().expect("config.toml written")).unwrap();
    let rectangle = saved.tools.get("rectangle").expect("tools.rectangle");
    assert_eq!(rectangle.get("thickness"), Some(&toml::Value::Float(7.0)));
    assert_eq!(rectangle.get("shadow"), Some(&toml::Value::Boolean(false)));
}

#[test]
fn restyling_with_nothing_selected_is_saved_too() {
    let (mut app, store) = app_with_config(None);
    app.press(Key::R);
    app.harness.state_mut().execute(Command::SetProperty {
        property: Property::LineColour(Rgba::rgb(0, 0, 255)),
        merge: false,
    });
    app.settle();

    let text = store.text().expect("config.toml written");
    assert!(text.contains("[tools.rectangle]"), "{text}");
    assert!(text.contains("line = \"#0000ff\""), "{text}");
}

#[test]
fn tool_styles_from_the_config_are_used_for_new_annotations() {
    let (mut app, _) = app_with_config(Some(
        "version = 1\n\
         [tools.rectangle]\n\
         line = \"#00ff0080\"\n\
         fill = \"#ffff00\"\n\
         thickness = 5.0\n\
         shadow = false\n",
    ));
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));

    let style = &app.doc().annotations()[0].style;
    assert_eq!(style.line, Rgba::new(0, 255, 0, 128));
    assert_eq!(style.fill, Some(Rgba::rgb(255, 255, 0)));
    assert_eq!(style.thickness, 5.0);
    assert!(!style.shadow);
}

#[test]
fn missing_tool_style_keys_take_the_greenshot_defaults() {
    let (mut app, _) = app_with_config(Some(
        "version = 1\n\
         [tools.rectangle]\n\
         thickness = 6.0\n",
    ));
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));

    let style = &app.doc().annotations()[0].style;
    assert_eq!(style.thickness, 6.0);
    assert_eq!(style.line, Rgba::RED);
    assert_eq!(style.fill, None);
    assert!(style.shadow);
}

#[test]
fn an_unreadable_tool_style_falls_back_to_that_tools_defaults_only() {
    let (mut app, store) = app_with_config(Some(
        "version = 1\n\
         jpeg_quality = 80\n\
         [tools.rectangle]\n\
         thickness = 6.0\n\
         line = \"not a colour\"\n",
    ));
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));

    assert_eq!(app.doc().annotations()[0].style, Default::default());
    // The rest of the config is kept: the file is not treated as corrupt.
    assert_eq!(app.harness.state().config().jpeg_quality, 80);
    assert!(store.bad_text().is_none());
}

#[test]
fn a_multi_selection_moves_one_level_together() {
    let (mut app, [one, two, three]) = app_with_three_rectangles();
    // Select 1 and 2.
    app.drag_px((10.0, 10.0), (150.0, 150.0));
    assert_eq!(selected(&app), vec![one, two]);
    app.key_with(Modifiers::COMMAND, Key::CloseBracket);
    assert_eq!(ids(&app), vec![three, one, two]);
    app.key_with(Modifiers::COMMAND, Key::OpenBracket);
    assert_eq!(ids(&app), vec![one, two, three]);
}

#[test]
fn changing_one_tool_leaves_the_other_tools_tables_untouched() {
    // The config already holds a partial `arrow` table (just its thickness). Changing an
    // unrelated tool (the rectangle) must persist the rectangle only; the arrow table stays the
    // partial table that was loaded, not re-serialised to every key (#61).
    let (mut app, store) = app_with_config(Some(
        "version = 1\n\
         [tools.arrow]\n\
         thickness = 7.0\n",
    ));
    app.press(Key::R);
    app.harness.state_mut().execute(Command::SetProperty {
        property: Property::Thickness(12.0),
        merge: false,
    });
    app.settle();

    let saved = Config::parse(&store.text().expect("config.toml written")).unwrap();

    // The arrow table is untouched: exactly the one key that was loaded.
    let arrow = saved.tools.get("arrow").expect("tools.arrow kept");
    assert_eq!(
        arrow.get("thickness"),
        Some(&toml::Value::Float(7.0)),
        "arrow thickness kept"
    );
    assert_eq!(
        arrow.len(),
        1,
        "the arrow table must not be re-serialised to every key: {arrow:?}"
    );

    // The rectangle is written identically to how the whole-config serialise would have: the
    // editor's current rectangle style, as a full table, with the new thickness.
    let rectangle = saved
        .tools
        .get("rectangle")
        .expect("tools.rectangle written");
    let expected =
        toml::Table::try_from(app.harness.state().tool_style(ToolId::Rectangle)).unwrap();
    assert_eq!(rectangle, &expected, "rectangle written in full, as before");
    assert_eq!(rectangle.get("thickness"), Some(&toml::Value::Float(12.0)));
}
