//! Step counter (#29, §4.1 I, §4.3, §4.5): a click places the next number; numbers follow creation
//! order, deleting one renumbers the rest, undo restores the old numbering.

mod support;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{Property, Rgba};
use support::TestApp;

/// A 320×240 Image, shown at 100% (Image pixels are screen points), with the step counter tool.
fn app_with_tool() -> TestApp {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::I);
    app
}

/// The numbers shown, bottom to top.
fn numbers(app: &TestApp) -> Vec<u32> {
    let doc = app.doc();
    doc.annotations()
        .iter()
        .map(|a| doc.step_number(a.id).expect("a Step counter"))
        .collect()
}

/// Each counter's centre, rounded, bottom to top.
fn centres(app: &TestApp) -> Vec<(i32, i32)> {
    app.doc()
        .annotations()
        .iter()
        .map(|a| {
            let c = a.bounds().center();
            (c.x.round() as i32, c.y.round() as i32)
        })
        .collect()
}

#[test]
fn a_click_places_the_next_number_selected_and_keeps_the_tool() {
    let mut app = app_with_tool();
    assert_eq!(app.harness.state().tool(), ToolId::StepCounter);

    app.click_px((60.0, 50.0));
    assert_eq!(numbers(&app), [1]);
    assert_eq!(centres(&app), [(60, 50)]);
    let doc = app.doc();
    assert_eq!(doc.selection(), &[doc.annotations()[0].id]);
    assert_eq!(doc.undo_len(), 1);

    app.click_px((160.0, 120.0));
    app.click_px((260.0, 190.0));
    assert_eq!(numbers(&app), [1, 2, 3]);
    assert_eq!(centres(&app), [(60, 50), (160, 120), (260, 190)]);
    assert_eq!(app.doc().undo_len(), 3);
    assert_eq!(app.harness.state().tool(), ToolId::StepCounter);
    app.assert_golden("step-counters-placed");
}

/// Three counters at (60, 50), (160, 120), (260, 190), numbered 1, 2, 3.
fn app_with_three() -> TestApp {
    let mut app = app_with_tool();
    for at in [(60.0, 50.0), (160.0, 120.0), (260.0, 190.0)] {
        app.click_px(at);
    }
    app
}

/// Change a property as the property bar does (one step).
fn set(app: &mut TestApp, property: Property) {
    app.harness.state_mut().execute(Command::SetProperty {
        property,
        merge: false,
    });
    app.settle();
}

// ---- Numbering (§4.5) ----

#[test]
fn deleting_one_renumbers_the_rest_and_undo_restores_the_old_numbering() {
    let mut app = app_with_three();
    app.press(Key::Escape);
    app.click_px((160.0, 120.0));
    let middle = app.doc().annotations()[1].id;
    assert_eq!(app.doc().selection(), &[middle]);

    app.press(Key::Delete);
    assert_eq!(numbers(&app), [1, 2]);
    assert_eq!(centres(&app), [(60, 50), (260, 190)]);
    app.assert_golden("step-counters-renumbered");

    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(numbers(&app), [1, 2, 3]);
    assert_eq!(centres(&app), [(60, 50), (160, 120), (260, 190)]);
    assert_eq!(app.doc().step_number(middle), Some(2));
    app.assert_golden("step-counters-placed");

    app.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert_eq!(numbers(&app), [1, 2]);
}

#[test]
fn a_counter_placed_after_a_deletion_takes_the_next_number() {
    let mut app = app_with_three();
    app.press(Key::Escape);
    app.click_px((60.0, 50.0));
    app.press(Key::Delete);
    app.press(Key::I);
    app.click_px((100.0, 200.0));

    assert_eq!(numbers(&app), [1, 2, 3]);
    assert_eq!(centres(&app), [(160, 120), (260, 190), (100, 200)]);
}

#[test]
fn undoing_a_placement_removes_its_number() {
    let mut app = app_with_three();
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(numbers(&app), [1, 2]);
    app.click_px((100.0, 200.0));
    assert_eq!(numbers(&app), [1, 2, 3]);
    assert_eq!(centres(&app), [(60, 50), (160, 120), (100, 200)]);
}

#[test]
fn a_drag_places_the_counter_where_it_is_released_in_one_step() {
    let mut app = app_with_tool();
    app.drag_px((60.0, 50.0), (120.0, 90.0));
    assert_eq!(numbers(&app), [1]);
    assert_eq!(centres(&app), [(120, 90)]);
    assert_eq!(app.doc().undo_len(), 1);
}

#[test]
fn moving_a_counter_keeps_its_number() {
    let mut app = app_with_three();
    app.press(Key::Escape);
    app.drag_px((60.0, 50.0), (300.0, 220.0));
    assert_eq!(numbers(&app), [1, 2, 3]);
    assert_eq!(centres(&app)[0], (300, 220));
}

// ---- Properties (§4.3) ----

#[test]
fn the_property_bar_shows_fill_colour_size_and_shadow_and_no_number() {
    let app = app_with_tool();
    for label in ["Fill colour", "Size", "Shadow"] {
        app.control(label);
    }
    for label in ["Line colour", "Thickness", "Number"] {
        assert!(
            app.harness.query_by_label(label).is_none(),
            "{label} should not be shown"
        );
    }
}

#[test]
fn the_default_counter_is_a_red_disc_with_a_shadow() {
    let mut app = app_with_tool();
    app.click_px((60.0, 50.0));
    let style = &app.doc().annotations()[0].style;
    assert_eq!(style.fill, Some(Rgba::RED));
    assert!(style.shadow);
    assert_eq!(app.selection_bounds(), Some([45, 35, 75, 65]));
}

#[test]
fn fill_size_and_shadow_restyle_the_selected_counter_one_step_each() {
    let mut app = app_with_three();
    set(&mut app, Property::FillColour(Some(Rgba::rgb(255, 230, 0))));
    app.control("Size").set_value_accesskit(50.0);
    app.settle();
    app.control("Shadow").click();
    app.settle();

    let style = &app.doc().annotations()[2].style;
    assert_eq!(style.fill, Some(Rgba::rgb(255, 230, 0)));
    assert_eq!(style.counter_size, 50.0);
    assert!(!style.shadow);
    assert_eq!(app.doc().undo_len(), 6);
    assert_eq!(app.selection_bounds(), Some([235, 165, 285, 215]));
    assert_eq!(numbers(&app), [1, 2, 3]);
    app.assert_golden("step-counter-restyled");
}

#[test]
fn a_size_set_with_nothing_selected_applies_to_the_next_counter() {
    let mut app = app_with_tool();
    app.control("Size").set_value_accesskit(20.0);
    app.settle();
    app.click_px((60.0, 50.0));
    assert_eq!(app.selection_bounds(), Some([50, 40, 70, 60]));
    assert_eq!(app.doc().undo_len(), 1);
}

#[test]
fn duplicating_counters_numbers_the_copies_next_in_order() {
    let mut app = app_with_three();
    app.press(Key::Escape);
    app.click_px((60.0, 50.0));
    app.harness.state_mut().execute(Command::Duplicate);
    app.settle();
    assert_eq!(numbers(&app), [1, 2, 3, 4]);
    assert_eq!(centres(&app)[3], (70, 60));

    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(numbers(&app), [1, 2, 3]);
}

// ---- Image operations (§4.7) ----

#[test]
fn counters_follow_rotate_upright_and_scale_with_resize() {
    let mut app = app_with_tool();
    app.click_px((60.0, 50.0));
    app.harness
        .state_mut()
        .execute(Command::Rotate { clockwise: true });
    app.settle();
    // 320×240 turned clockwise: (x, y) → (240 − y, x) in continuous Image coordinates.
    assert_eq!(centres(&app), [(190, 60)]);
    assert_eq!(app.selection_bounds(), Some([175, 45, 205, 75]));

    app.harness.state_mut().execute(Command::Resize {
        width: 480,
        height: 640,
    });
    app.settle();
    assert_eq!(centres(&app), [(380, 120)]);
    assert_eq!(app.doc().annotations()[0].style.counter_size, 60.0);
    assert_eq!(numbers(&app), [1]);
}

#[test]
fn a_rectangle_does_not_take_a_number_and_counters_skip_it() {
    let mut app = app_with_tool();
    app.click_px((60.0, 50.0));
    app.press(Key::R);
    app.drag_px((100.0, 100.0), (150.0, 150.0));
    app.press(Key::I);
    app.click_px((260.0, 190.0));

    let doc = app.doc();
    let ids: Vec<_> = doc.annotations().iter().map(|a| a.id).collect();
    assert_eq!(doc.step_number(ids[0]), Some(1));
    assert_eq!(doc.step_number(ids[1]), None);
    assert_eq!(doc.step_number(ids[2]), Some(2));
}
