//! The editor tracer bullet (#25): the rectangle tool, select, move, resize, restyle, delete and
//! the History (§4.1–§4.3, §4.8), the tool strip and property bar (§2), the Skia composite (§7.2).

mod support;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{Property, Rgba};
use support::TestApp;

/// A 320×240 Image, shown at 100% (Image pixels are screen points).
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

/// An Image with one rectangle from (40, 30) to (140, 90), drawn with the rectangle tool, selected.
fn app_with_rectangle() -> TestApp {
    let mut app = app_with_image();
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app
}

fn rect_of(app: &TestApp, index: usize) -> [i32; 4] {
    let r = app.doc().annotations()[index].bounds();
    [
        r.min.x.round() as i32,
        r.min.y.round() as i32,
        r.max.x.round() as i32,
        r.max.y.round() as i32,
    ]
}

// ---- Tool strip ----

#[test]
fn tool_strip_lists_every_tool_in_order_with_the_built_ones_enabled() {
    let app = app_with_image();
    let names = [
        "Select",
        "Rectangle",
        "Ellipse",
        "Line",
        "Arrow",
        "Freehand",
        "Text",
        "Step counter",
        "Highlight",
        "Obfuscate",
        "Crop",
    ];
    let buttons: Vec<_> = app
        .harness
        .get_all_by_role(Role::Button)
        .map(|n| n.accesskit_node())
        .filter(|n| n.label().is_some_and(|l| names.contains(&l.as_str())))
        .map(|n| (n.label().unwrap(), !n.is_disabled()))
        .collect();
    let shown: Vec<&str> = buttons.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(shown, names);
    for (label, enabled) in &buttons {
        assert!(*enabled, "{label} enabled");
    }
}

#[test]
fn clicking_a_tool_button_selects_the_tool_and_esc_returns_to_select() {
    let mut app = app_with_image();
    assert_eq!(app.harness.state().tool(), ToolId::Select);
    app.control("Rectangle").click();
    app.settle();
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);
    app.press(Key::Escape);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
}

// ---- Drawing ----

#[test]
fn dragging_with_the_rectangle_tool_draws_a_selected_rectangle_and_keeps_the_tool() {
    let app = app_with_rectangle();

    let doc = app.doc();
    assert_eq!(doc.annotations().len(), 1);
    assert_eq!(doc.selection(), &[doc.annotations()[0].id]);
    assert_eq!(app.selection_bounds(), Some([40, 30, 140, 90]));
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);
    app.assert_golden("rectangle-drawn");
}

#[test]
fn the_tool_stays_active_for_several_rectangles_in_a_row() {
    let mut app = app_with_rectangle();
    app.drag_px((200.0, 150.0), (160.0, 120.0));

    assert_eq!(app.doc().annotations().len(), 2);
    assert_eq!(rect_of(&app, 1), [160, 120, 200, 150]);
    assert_eq!(app.selection_bounds(), Some([160, 120, 200, 150]));
}

#[test]
fn shift_makes_a_square() {
    let mut app = app_with_image();
    app.press(Key::R);
    app.drag_px_with(Modifiers::SHIFT, (50.0, 50.0), (130.0, 90.0));

    assert_eq!(rect_of(&app, 0), [50, 50, 130, 130]);
}

#[test]
fn a_click_with_the_rectangle_tool_draws_nothing() {
    let mut app = app_with_image();
    app.press(Key::R);
    app.click_px((50.0, 50.0));

    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());
}

// ---- Selecting, moving, resizing, deleting ----

#[test]
fn clicking_selects_and_clicking_empty_space_deselects() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    app.click_px((250.0, 200.0));
    assert!(app.doc().selection().is_empty());
    assert_eq!(app.harness.state().selection_box(), None);

    // On the line of an unfilled rectangle.
    app.click_px((40.0, 60.0));
    assert_eq!(app.doc().selection().len(), 1);
    let shown = app
        .harness
        .state()
        .selection_box()
        .expect("a selection box");
    let top_left = app.screen(40.0, 30.0);
    let bottom_right = app.screen(140.0, 90.0);
    assert!((shown.min - top_left).length() < 0.5);
    assert!((shown.max - bottom_right).length() < 0.5);

    // Inside an unfilled rectangle is not on it.
    app.click_px((90.0, 60.0));
    assert!(app.doc().selection().is_empty());
}

#[test]
fn dragging_a_selected_rectangle_moves_it_as_one_step() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    app.drag_px((40.0, 50.0), (70.0, 90.0));

    assert_eq!(rect_of(&app, 0), [70, 70, 170, 130]);
    assert_eq!(app.doc().undo_len(), 2);
    app.assert_golden("rectangle-moved");
}

#[test]
fn dragging_an_unselected_rectangle_selects_and_moves_it() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    app.click_px((250.0, 200.0));
    app.drag_px((140.0, 60.0), (150.0, 70.0));

    assert_eq!(rect_of(&app, 0), [50, 40, 150, 100]);
    assert_eq!(app.doc().selection().len(), 1);
}

#[test]
fn handles_resize_the_selection() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    // Bottom-right corner handle.
    app.drag_px((140.0, 90.0), (200.0, 160.0));
    assert_eq!(rect_of(&app, 0), [40, 30, 200, 160]);
    // Left edge handle: only the left edge moves.
    app.drag_px((40.0, 95.0), (10.0, 95.0));
    assert_eq!(rect_of(&app, 0), [10, 30, 200, 160]);
    // Top edge handle.
    app.drag_px((105.0, 30.0), (105.0, 60.0));
    assert_eq!(rect_of(&app, 0), [10, 60, 200, 160]);

    assert_eq!(app.doc().undo_len(), 4);
    app.assert_golden("rectangle-resized");
}

#[test]
fn delete_removes_the_selection() {
    let mut app = app_with_rectangle();
    app.press(Key::Delete);

    assert!(app.doc().annotations().is_empty());
    assert!(app.doc().selection().is_empty());
    assert_eq!(app.doc().undo_len(), 2);
}

#[test]
fn the_property_bar_delete_button_removes_the_selection() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    app.click_px((40.0, 60.0));
    app.control("Delete").click();
    app.settle();

    assert!(app.doc().annotations().is_empty());
}

// ---- Restyling ----

#[test]
fn the_property_bar_shows_the_rectangle_controls() {
    let app = app_with_rectangle();
    for label in [
        "Line colour",
        "Fill colour",
        "No fill",
        "Thickness",
        "Shadow",
    ] {
        app.control(label);
    }
}

#[test]
fn the_select_tool_with_nothing_selected_shows_a_hint() {
    let mut app = app_with_image();
    app.press(Key::Escape);
    assert!(app.harness.query_by_label("Thickness").is_none());
    app.harness
        .get_by_label_contains("Click an Annotation to select it");
}

#[test]
fn property_bar_controls_restyle_the_selection_one_step_each() {
    let mut app = app_with_rectangle();
    app.control("Shadow").click();
    app.settle();
    app.control("Thickness").set_value_accesskit(8.0);
    app.settle();
    app.control("No fill").click();
    app.settle();
    app.harness.state_mut().execute(Command::SetProperty {
        property: Property::LineColour(Rgba::rgb(0, 0, 255)),
        merge: false,
    });
    app.settle();

    let style = &app.doc().annotations()[0].style;
    assert!(!style.shadow);
    assert_eq!(style.thickness, 8.0);
    assert_eq!(style.fill, Some(Rgba::WHITE));
    assert_eq!(style.line, Rgba::rgb(0, 0, 255));
    assert_eq!(app.doc().undo_len(), 5);
    app.assert_golden("rectangle-restyled");
}

#[test]
fn a_filled_rectangle_is_selected_by_clicking_inside() {
    let mut app = app_with_rectangle();
    app.control("No fill").click();
    app.settle();
    app.press(Key::Escape);
    app.click_px((250.0, 200.0));
    app.click_px((90.0, 60.0));

    assert_eq!(app.doc().selection().len(), 1);
}

#[test]
fn changes_in_one_widget_interaction_merge_into_one_step() {
    let mut app = app_with_rectangle();
    for (merge, t) in [(false, 4.0), (true, 5.0), (true, 6.0)] {
        app.harness.state_mut().execute(Command::SetProperty {
            property: Property::Thickness(t),
            merge,
        });
    }
    app.settle();

    assert_eq!(app.doc().undo_len(), 2);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.doc().annotations()[0].style.thickness, 3.0);
}

#[test]
fn with_nothing_selected_a_property_sets_the_tools_default() {
    let mut app = app_with_image();
    app.press(Key::R);
    app.control("Thickness").set_value_accesskit(12.0);
    app.settle();
    app.drag_px((40.0, 30.0), (140.0, 90.0));

    assert_eq!(app.doc().annotations()[0].style.thickness, 12.0);
    assert_eq!(app.doc().undo_len(), 1);
}

// ---- History ----

#[test]
fn undo_and_redo_step_through_each_gesture() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    app.drag_px((40.0, 50.0), (70.0, 90.0)); // move
    app.drag_px((170.0, 130.0), (200.0, 150.0)); // resize
    app.press(Key::Delete);
    assert!(app.doc().annotations().is_empty());

    let ctrl = Modifiers::COMMAND;
    app.key_with(ctrl, Key::Z);
    assert_eq!(rect_of(&app, 0), [70, 70, 200, 150]);
    app.key_with(ctrl, Key::Z);
    assert_eq!(rect_of(&app, 0), [70, 70, 170, 130]);
    app.key_with(ctrl, Key::Z);
    assert_eq!(rect_of(&app, 0), [40, 30, 140, 90]);
    app.key_with(ctrl, Key::Z);
    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());

    app.key_with(ctrl | Modifiers::SHIFT, Key::Z);
    assert_eq!(rect_of(&app, 0), [40, 30, 140, 90]);
    app.key_with(ctrl, Key::Y);
    assert_eq!(rect_of(&app, 0), [70, 70, 170, 130]);
    assert_eq!(app.doc().redo_len(), 2);
}

#[test]
fn a_new_step_after_undo_drops_the_redo_steps() {
    let mut app = app_with_rectangle();
    app.key_with(Modifiers::COMMAND, Key::Z);
    app.drag_px((10.0, 10.0), (20.0, 20.0));

    assert!(!app.doc().can_redo());
    assert_eq!(app.doc().undo_len(), 1);
}

#[test]
fn opening_another_image_clears_the_history() {
    let mut app = app_with_rectangle();
    let other = app.add_fixtures(&["large-2400x1600.jpg"]);
    app.open(other);
    // The unsaved-changes prompt (#35): drop the edits.
    app.control("Discard").click();
    app.settle();

    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());
    assert_eq!(app.status("File name"), "large-2400x1600.jpg");
}

#[test]
fn moving_to_another_sibling_clears_the_history() {
    let mut app = TestApp::new();
    let small = app.add_fixtures(&["large-2400x1600.jpg", "small-320x240.jpg"]);
    let small = small.with_file_name("small-320x240.jpg");
    app.open(small);
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    assert!(app.doc().can_undo());

    app.press(Key::ArrowLeft);
    // The unsaved-changes prompt (#35): drop the edits.
    app.control("Discard").click();
    app.settle();
    assert_eq!(app.status("File name"), "large-2400x1600.jpg");
    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());

    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "small-320x240.jpg");
    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());
}

#[test]
fn the_file_name_shows_a_dot_while_there_are_unsaved_changes() {
    let mut app = app_with_image();
    assert_eq!(app.status("File name"), "small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    assert_eq!(app.status("File name"), "small-320x240.jpg •");
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.status("File name"), "small-320x240.jpg");
}

// ---- Rendering and decoding ----

#[test]
fn the_first_rectangle_shows_on_the_canvas_over_the_image() {
    let mut app = app_with_image();
    app.press(Key::R);
    let inside = app.pixel(90, 60).expect("the Image on screen");

    app.drag_px((40.0, 30.0), (140.0, 90.0));
    let canvas = app.composite_image();
    // The visible region at physical resolution: the whole 320×240 Image at 100%.
    assert_eq!(canvas.dimensions(), (320, 240));
    // A red line along the left edge; inside, the Image shows through unchanged.
    let [r, g, b, _] = canvas.get_pixel(40, 60).0;
    assert!(r > 200 && g < 60 && b < 60, "edge {:?}", [r, g, b]);
    let [r, g, b, _] = canvas.get_pixel(90, 60).0;
    let close = |a: u8, b: u8| a.abs_diff(b) <= 6;
    assert!(
        close(r, inside.r()) && close(g, inside.g()) && close(b, inside.b()),
        "inside {:?}, Image {inside:?}",
        [r, g, b]
    );
}

#[test]
fn the_composite_is_at_physical_resolution_on_a_scaled_image() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    app.press(Key::R);
    app.drag_px((300.0, 300.0), (1500.0, 1100.0));

    let canvas = app.harness.state().canvas().unwrap();
    let composite = app.harness.state().composite().expect("a composite");
    assert_eq!(composite.width, canvas.image.width().round() as u32);
    assert_eq!(composite.height, canvas.image.height().round() as u32);
    // 1200×800 Image pixels at the canvas scale.
    assert_eq!(rect_of(&app, 0), [300, 300, 1500, 1100]);
    app.assert_golden("rectangle-on-scaled-image");
}

#[test]
fn the_first_tool_use_starts_the_full_resolution_decode() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    assert!(app.doc().full_resolution().is_none());
    app.press(Key::R);

    let full = app.doc().full_resolution().expect("full-resolution pixels");
    assert_eq!((full.width, full.height), (2400, 1600));
}

// ---- With zoom and fullscreen ----

#[test]
fn fullscreen_hides_the_tool_strip_and_the_property_bar() {
    let mut app = app_with_rectangle();
    app.press(Key::F11);
    assert!(app.harness.query_by_label("Rectangle").is_none());
    assert!(app.harness.query_by_label("Thickness").is_none());
    app.press(Key::F11);
    app.control("Rectangle");
}

#[test]
fn panning_drags_leave_the_rectangle_tool_alone() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    app.press(Key::R);
    for _ in 0..4 {
        app.press(Key::Plus);
    }
    let (a, b) = (egui::pos2(500.0, 300.0), egui::pos2(400.0, 250.0));
    app.drag(egui::PointerButton::Middle, a, b, false);
    app.drag(egui::PointerButton::Primary, a, b, true);

    assert!(app.doc().annotations().is_empty());
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);
}

#[test]
fn zoomed_in_the_rectangle_keeps_its_image_pixels_and_the_composite_its_resolution() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    app.press(Key::R);
    app.drag_px((1000.0, 700.0), (1400.0, 900.0));
    for _ in 0..6 {
        app.press(Key::Plus);
    }
    app.press(Key::Escape);
    app.drag_px((1000.0, 750.0), (1020.0, 770.0));
    assert_eq!(rect_of(&app, 0), [1020, 720, 1420, 920]);

    let composite = app.harness.state().composite().expect("a composite");
    let canvas = app.harness.state().canvas().unwrap();
    let visible = canvas.image.intersect(canvas.area);
    assert_eq!(composite.width, visible.width().round() as u32);
    assert_eq!(composite.height, visible.height().round() as u32);
    app.assert_golden("rectangle-zoomed");
}
