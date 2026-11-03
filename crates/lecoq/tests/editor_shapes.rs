//! The other shape tools (#26): ellipse, line, arrow and freehand (§4.1, §4.3).

mod support;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationShape, Arrowheads, Point};
use support::TestApp;

/// A 320×240 Image, shown at 100% (Image pixels are screen points).
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

fn bounds_of(app: &TestApp, index: usize) -> [i32; 4] {
    let r = app.doc().annotations()[index].bounds();
    [
        r.min.x.round() as i32,
        r.min.y.round() as i32,
        r.max.x.round() as i32,
        r.max.y.round() as i32,
    ]
}

fn assert_controls(app: &TestApp, present: &[&str], absent: &[&str]) {
    for label in present {
        app.control(label);
    }
    for label in absent {
        assert!(
            app.harness.query_by_label(label).is_none(),
            "{label} should not be in the property bar"
        );
    }
}

fn assert_near(actual: Point, expected: (f32, f32)) {
    assert!(
        (actual.x - expected.0).abs() < 0.5 && (actual.y - expected.1).abs() < 0.5,
        "{actual:?} is not near {expected:?}"
    );
}

// ---- Ellipse ----

#[test]
fn dragging_with_the_ellipse_tool_draws_a_selected_ellipse_and_keeps_the_tool() {
    let mut app = app_with_image();
    app.press(Key::E);
    app.drag_px((40.0, 30.0), (200.0, 130.0));

    let doc = app.doc();
    assert_eq!(doc.annotations().len(), 1);
    assert_eq!(doc.selection(), &[doc.annotations()[0].id]);
    assert_eq!(app.selection_bounds(), Some([40, 30, 200, 130]));
    assert_eq!(doc.undo_len(), 1);
    assert_eq!(app.harness.state().tool(), ToolId::Ellipse);
    app.assert_golden("ellipse-drawn");
}

#[test]
fn shift_makes_a_circle() {
    let mut app = app_with_image();
    app.press(Key::E);
    app.drag_px_with(Modifiers::SHIFT, (50.0, 50.0), (130.0, 90.0));

    assert_eq!(bounds_of(&app, 0), [50, 50, 130, 130]);
}

#[test]
fn the_ellipse_property_bar_has_line_fill_thickness_and_shadow() {
    let mut app = app_with_image();
    app.press(Key::E);
    assert_controls(
        &app,
        &[
            "Line colour",
            "Fill colour",
            "No fill",
            "Thickness",
            "Shadow",
        ],
        &["Arrowheads"],
    );
}

#[test]
fn an_unfilled_ellipse_is_hit_on_its_line_and_a_filled_one_inside() {
    let mut app = app_with_image();
    app.press(Key::E);
    app.drag_px((40.0, 30.0), (200.0, 130.0));
    app.press(Key::Escape);
    app.click_px((300.0, 220.0));

    // Inside an unfilled ellipse is not on it, nor is the corner of its box.
    app.click_px((120.0, 80.0));
    assert!(app.doc().selection().is_empty());
    app.click_px((42.0, 32.0));
    assert!(app.doc().selection().is_empty());
    // On the line: its leftmost point.
    app.click_px((40.0, 80.0));
    assert_eq!(app.doc().selection().len(), 1);

    app.control("No fill").click();
    app.settle();
    app.click_px((300.0, 220.0));
    app.click_px((120.0, 80.0));
    assert_eq!(app.doc().selection().len(), 1);
    app.assert_golden("ellipse-filled");
}

// ---- Line ----

fn segment(app: &TestApp, index: usize) -> (Point, Point) {
    match &app.doc().annotations()[index].shape {
        AnnotationShape::Line(l) => (l.from, l.to),
        AnnotationShape::Arrow(a) => (a.from, a.to),
        other => panic!("not a line or arrow: {other:?}"),
    }
}

#[test]
fn dragging_with_the_line_tool_draws_a_selected_line_as_one_step() {
    let mut app = app_with_image();
    app.press(Key::L);
    app.drag_px((40.0, 200.0), (260.0, 60.0));

    assert!(matches!(
        app.doc().annotations()[0].shape,
        AnnotationShape::Line(_)
    ));
    let (from, to) = segment(&app, 0);
    assert_near(from, (40.0, 200.0));
    assert_near(to, (260.0, 60.0));
    assert_eq!(app.selection_bounds(), Some([40, 60, 260, 200]));
    assert_eq!(app.doc().undo_len(), 1);
    assert_eq!(app.harness.state().tool(), ToolId::Line);
    app.assert_golden("line-drawn");
}

#[test]
fn shift_snaps_a_line_to_15_degree_steps_keeping_its_length() {
    let mut app = app_with_image();
    app.press(Key::L);
    // 11.3° below horizontal, length √(100² + 20²) ≈ 101.98: snaps to 15°.
    app.drag_px_with(Modifiers::SHIFT, (50.0, 100.0), (150.0, 120.0));
    // 2.3°: snaps to horizontal.
    app.drag_px_with(Modifiers::SHIFT, (50.0, 150.0), (150.0, 154.0));
    // Up and to the left at 129.8°, length 78.1: snaps to 135°.
    app.drag_px_with(Modifiers::SHIFT, (200.0, 200.0), (150.0, 140.0));

    let (from, to) = segment(&app, 0);
    assert_near(from, (50.0, 100.0));
    assert_near(to, (148.51, 126.39));
    assert_near(segment(&app, 1).1, (150.08, 150.0));
    assert_near(segment(&app, 2).1, (144.77, 144.77));
}

#[test]
fn the_line_property_bar_has_line_thickness_and_shadow() {
    let mut app = app_with_image();
    app.press(Key::L);
    assert_controls(
        &app,
        &["Line colour", "Thickness", "Shadow"],
        &["Fill colour", "No fill", "Arrowheads"],
    );
}

#[test]
fn a_line_is_selected_by_clicking_on_it_and_moves_with_the_select_tool() {
    let mut app = app_with_image();
    app.press(Key::L);
    app.drag_px((40.0, 40.0), (240.0, 140.0));
    app.press(Key::Escape);
    app.click_px((300.0, 30.0));
    assert!(app.doc().selection().is_empty());

    // Near the line, but 18 px off it.
    app.click_px((140.0, 110.0));
    assert!(app.doc().selection().is_empty());
    app.drag_px((140.0, 90.0), (150.0, 120.0));

    let (from, to) = segment(&app, 0);
    assert_near(from, (50.0, 70.0));
    assert_near(to, (250.0, 170.0));
    assert_eq!(app.doc().undo_len(), 2);
}

// ---- Arrow ----

#[test]
fn an_arrow_has_its_head_at_the_end_by_default() {
    let mut app = app_with_image();
    app.press(Key::A);
    app.drag_px((40.0, 200.0), (260.0, 60.0));

    let arrow = &app.doc().annotations()[0];
    assert!(matches!(arrow.shape, AnnotationShape::Arrow(_)));
    assert_eq!(arrow.style.arrowheads, Arrowheads::End);
    assert_eq!(app.doc().undo_len(), 1);
    assert_eq!(app.harness.state().tool(), ToolId::Arrow);
    app.assert_golden("arrow-drawn");
}

#[test]
fn shift_snaps_an_arrow_to_15_degree_steps() {
    let mut app = app_with_image();
    app.press(Key::A);
    // 84.3° (almost straight down), length 100.5: snaps to 90°.
    app.drag_px_with(Modifiers::SHIFT, (100.0, 40.0), (110.0, 140.0));

    assert_near(segment(&app, 0).1, (100.0, 140.5));
}

#[test]
fn the_arrow_property_bar_has_line_thickness_arrowheads_and_shadow() {
    let mut app = app_with_image();
    app.press(Key::A);
    assert_controls(
        &app,
        &["Line colour", "Thickness", "Arrowheads", "Shadow"],
        &["Fill colour", "No fill"],
    );
}

fn choose_arrowheads(app: &mut TestApp, option: &str) {
    app.control("Arrowheads").click();
    app.settle();
    app.control(option).click();
    app.settle();
}

#[test]
fn arrowheads_can_be_at_the_end_the_start_both_ends_or_none() {
    let mut app = app_with_image();
    for (i, option) in ["End", "Start", "Both", "None"].into_iter().enumerate() {
        // With nothing selected, the choice is the tool's default for the next arrow.
        app.press(Key::Escape);
        app.press(Key::A);
        choose_arrowheads(&mut app, option);
        let y = 40.0 + 50.0 * i as f32;
        app.drag_px((40.0, y), (280.0, y + 20.0));
    }

    let heads: Vec<_> = app
        .doc()
        .annotations()
        .iter()
        .map(|a| a.style.arrowheads)
        .collect();
    assert_eq!(
        heads,
        [
            Arrowheads::End,
            Arrowheads::Start,
            Arrowheads::Both,
            Arrowheads::None
        ]
    );
    app.assert_golden("arrow-arrowheads");
}

#[test]
fn changing_a_selected_arrows_arrowheads_is_one_undoable_step() {
    let mut app = app_with_image();
    app.press(Key::A);
    app.drag_px((40.0, 100.0), (280.0, 120.0));
    choose_arrowheads(&mut app, "Both");

    assert_eq!(
        app.doc().annotations()[0].style.arrowheads,
        Arrowheads::Both
    );
    assert_eq!(app.doc().undo_len(), 2);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.doc().annotations()[0].style.arrowheads, Arrowheads::End);
}

// ---- Freehand ----

/// The polyline through `corners`, with a point every `step` px, as a pointer would report it.
fn dense(corners: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
    let mut path = vec![corners[0]];
    for w in corners.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / step).ceil().max(1.0) as usize;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            path.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    path
}

fn stroke(app: &TestApp, index: usize) -> Vec<Point> {
    match &app.doc().annotations()[index].shape {
        AnnotationShape::Freehand(f) => f.points.clone(),
        other => panic!("not a freehand stroke: {other:?}"),
    }
}

#[test]
fn dragging_with_the_freehand_tool_draws_a_selected_stroke_as_one_step() {
    let mut app = app_with_image();
    app.press(Key::F);
    let corners = [
        (40.0, 60.0),
        (80.0, 40.0),
        (120.0, 60.0),
        (160.0, 120.0),
        (200.0, 160.0),
        (240.0, 150.0),
        (270.0, 110.0),
    ];
    app.drag_path_px(&dense(&corners, 3.0));

    let points = stroke(&app, 0);
    assert_near(points[0], (40.0, 60.0));
    assert_near(*points.last().unwrap(), (270.0, 110.0));
    assert_eq!(app.doc().selection().len(), 1);
    assert_eq!(app.doc().undo_len(), 1);
    assert_eq!(app.harness.state().tool(), ToolId::Freehand);
    app.assert_golden("freehand-drawn");
}

#[test]
fn the_freehand_stroke_is_lightly_smoothed() {
    let mut app = app_with_image();
    app.press(Key::F);
    // A horizontal stroke with ±1.5 px of hand tremor every 3 px, ending in a sharp turn down.
    let mut path: Vec<(f32, f32)> = (0..=60)
        .map(|i| {
            let jitter = match i {
                0 | 60 => 0.0,
                _ if i % 2 == 0 => -1.5,
                _ => 1.5,
            };
            (40.0 + 3.0 * i as f32, 100.0 + jitter)
        })
        .collect();
    path.extend(
        dense(&[(220.0, 100.0), (220.0, 200.0)], 3.0)
            .into_iter()
            .skip(1),
    );
    app.drag_path_px(&path);

    let points = stroke(&app, 0);
    // The ends stay where the pointer started and stopped.
    assert_near(points[0], (40.0, 100.0));
    assert_near(*points.last().unwrap(), (220.0, 200.0));
    // The tremor is smoothed out of the horizontal part…
    let tremor = points
        .iter()
        .filter(|p| p.x > 45.0 && p.x < 200.0)
        .map(|p| (p.y - 100.0).abs())
        .fold(0.0, f32::max);
    assert!(tremor < 0.5, "{tremor} px of tremor left");
    // …but only lightly: the turn is still close to the pointer's corner.
    let corner = points
        .iter()
        .map(|p| p.distance(Point::new(220.0, 100.0)))
        .fold(f32::MAX, f32::min);
    assert!(corner < 3.0, "the corner moved {corner} px");
    app.assert_golden("freehand-smoothed");
}

#[test]
fn the_freehand_property_bar_has_line_thickness_and_shadow() {
    let mut app = app_with_image();
    app.press(Key::F);
    assert_controls(
        &app,
        &["Line colour", "Thickness", "Shadow"],
        &["Fill colour", "No fill", "Arrowheads"],
    );
}

#[test]
fn a_freehand_stroke_is_selected_by_clicking_on_it() {
    let mut app = app_with_image();
    app.press(Key::F);
    app.drag_path_px(&dense(&[(40.0, 40.0), (100.0, 100.0), (160.0, 40.0)], 3.0));
    app.press(Key::Escape);
    app.click_px((300.0, 220.0));
    assert!(app.doc().selection().is_empty());

    // Between the arms of the V: not on it.
    app.click_px((100.0, 50.0));
    assert!(app.doc().selection().is_empty());
    // On the left arm.
    app.click_px((60.0, 60.0));
    assert_eq!(app.doc().selection().len(), 1);
}

// ---- All of them ----

#[test]
fn a_click_with_any_shape_tool_draws_nothing() {
    let mut app = app_with_image();
    for key in [Key::E, Key::L, Key::A, Key::F] {
        app.press(key);
        app.click_px((50.0, 50.0));
    }
    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());
}

#[test]
fn every_shape_transforms_with_a_handle_resize() {
    let mut app = app_with_image();
    app.press(Key::L);
    app.drag_px((40.0, 40.0), (140.0, 90.0));
    app.press(Key::Escape);
    // The bottom-right handle of the line's box, dragged out to double its size.
    app.drag_px((140.0, 90.0), (240.0, 140.0));

    let (from, to) = segment(&app, 0);
    assert_near(from, (40.0, 40.0));
    assert_near(to, (240.0, 140.0));
}
