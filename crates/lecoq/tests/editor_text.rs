//! The text tool (#28, §4.4): placing, typing, finishing, editing again, the fixed width, the
//! style properties, input-method composition, and one History step per editing session.

mod support;

use egui::{Event, ImeEvent, Key};
use egui_kittest::kittest::Queryable;
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationShape, Property, Rect, Rgba};
use support::TestApp;

/// A 320×240 Image, shown at 100% (Image pixels are screen points).
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

/// Type `text` as the keyboard would (one `Text` event), then settle.
fn type_text(app: &mut TestApp, text: &str) {
    app.harness.event(Event::Text(text.to_owned()));
    app.settle();
}

fn ime(app: &mut TestApp, event: ImeEvent) {
    app.harness.event(Event::Ime(event));
    app.settle();
}

/// Two clicks on consecutive frames, 0.1 s apart and well after any earlier click: a
/// double-click. Unset, egui's clock advances one predicted frame per frame, and how many frames
/// `settle()` runs varies, so the clock is set explicitly, far past anything it reached alone
/// (going back in time would turn this into a triple-click).
fn double_click_px(app: &mut TestApp, at: (f32, f32)) {
    let p = app.screen(at.0, at.1);
    app.harness.hover_at(p);
    let t0 = app.harness.input().time.unwrap_or(1.0e6) + 10.0;
    for i in 0..2 {
        app.harness.input_mut().time = Some(t0 + 0.1 * f64::from(i));
        for pressed in [true, false] {
            app.harness.event(Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        app.harness.step();
    }
    app.harness.remove_cursor();
    app.settle();
}

/// The text of Annotation `index`.
fn text_of(app: &TestApp, index: usize) -> String {
    match &app.doc().annotations()[index].shape {
        AnnotationShape::Text(t) => t.text().to_owned(),
        other => panic!("not a text Annotation: {other:?}"),
    }
}

fn bounds_of(app: &TestApp, index: usize) -> Rect {
    app.doc().annotations()[index].bounds()
}

fn editing(app: &TestApp) -> bool {
    app.harness.state().editing_text()
}

/// T, then a click at (50, 60): an empty box in editing mode.
fn app_with_new_box() -> TestApp {
    let mut app = app_with_image();
    app.press(Key::T);
    app.click_px((50.0, 60.0));
    app
}

/// A finished text Annotation "Hello" at (50, 60), with the select tool active.
fn app_with_hello() -> TestApp {
    let mut app = app_with_new_box();
    type_text(&mut app, "Hello");
    app.press(Key::Escape);
    app.press(Key::Escape);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
    app
}

// ---- Fonts ----

#[test]
fn fonts_are_enumerated_on_first_use_of_the_text_tool() {
    let mut app = app_with_image();
    assert!(app.harness.state().font_families().is_none());
    app.press(Key::T);
    let fonts = app.harness.state().font_families().expect("fonts listed");
    assert!(!fonts.is_empty());
}

// ---- Placing and typing ----

#[test]
fn a_click_places_an_empty_box_in_editing_mode() {
    let app = app_with_new_box();

    let doc = app.doc();
    assert_eq!(doc.annotations().len(), 1);
    assert_eq!(doc.selection(), &[doc.annotations()[0].id]);
    assert!(editing(&app));
    assert_eq!(text_of(&app, 0), "");
    let b = bounds_of(&app, 0);
    assert!((b.min.x - 50.0).abs() < 1.0 && (b.min.y - 60.0).abs() < 1.0);
    // One line of the default 20 px font.
    let size = lecoq_core::Style::default().font_size;
    assert!(b.height() > size && b.height() < 2.0 * size, "{b:?}");
}

#[test]
fn the_new_box_is_wide_enough_for_five_characters_and_grows_as_you_type() {
    let mut app = app_with_new_box();
    let empty = bounds_of(&app, 0);

    type_text(&mut app, "0000");
    assert_eq!(bounds_of(&app, 0), empty);
    type_text(&mut app, "0");
    let five = bounds_of(&app, 0);
    assert!(
        (five.width() - empty.width()).abs() < 1.0,
        "{five:?} vs {empty:?}"
    );
    type_text(&mut app, "0");
    let six = bounds_of(&app, 0);
    assert!(six.width() > empty.width() + 5.0, "{six:?} vs {empty:?}");
    assert_eq!(six.height(), empty.height());
    assert_eq!(text_of(&app, 0), "000000");
}

#[test]
fn enter_adds_a_line() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Hello");
    let one = bounds_of(&app, 0);
    app.press(Key::Enter);
    type_text(&mut app, "World");
    let two = bounds_of(&app, 0);

    assert_eq!(text_of(&app, 0), "Hello\nWorld");
    let line = two.height() - one.height();
    let size = lecoq_core::Style::default().font_size;
    assert!(line > size && line < 2.0 * size, "line height {line}");
}

#[test]
fn backspace_and_the_arrow_keys_edit_at_the_cursor() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Helo");
    app.press(Key::ArrowLeft);
    type_text(&mut app, "l");
    app.press(Key::End);
    app.press(Key::Backspace);
    type_text(&mut app, "o!");
    assert_eq!(text_of(&app, 0), "Hello!");
}

#[test]
fn plain_keys_type_instead_of_switching_tools() {
    let mut app = app_with_new_box();
    // The keyboard sends the key and the character; the key must not pick the rectangle tool.
    app.harness.key_press(Key::R);
    type_text(&mut app, "r");
    assert_eq!(app.harness.state().tool(), ToolId::Text);
    assert_eq!(text_of(&app, 0), "r");
}

// ---- Finishing ----

#[test]
fn esc_finishes_the_text_then_returns_to_select() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Hello");
    app.press(Key::Escape);

    assert!(!editing(&app));
    assert_eq!(app.harness.state().tool(), ToolId::Text);
    assert_eq!(text_of(&app, 0), "Hello");
    app.press(Key::Escape);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
}

#[test]
fn a_click_elsewhere_finishes_and_the_next_click_places_another_box() {
    let mut app = app_with_new_box();
    type_text(&mut app, "One");
    app.click_px((200.0, 180.0));
    assert!(!editing(&app));
    assert_eq!(app.doc().annotations().len(), 1);

    app.click_px((200.0, 180.0));
    assert!(editing(&app));
    type_text(&mut app, "Two");
    app.press(Key::Escape);
    assert_eq!(app.doc().annotations().len(), 2);
    assert_eq!(text_of(&app, 1), "Two");
}

#[test]
fn an_empty_box_is_discarded() {
    let mut app = app_with_new_box();
    app.press(Key::Escape);

    assert!(app.doc().annotations().is_empty());
    assert!(!app.doc().can_undo());
}

#[test]
fn switching_tools_finishes_the_text() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Kept");
    app.control("Rectangle").click();
    app.settle();

    assert!(!editing(&app));
    assert_eq!(text_of(&app, 0), "Kept");
}

// ---- History ----

#[test]
fn one_editing_session_is_one_history_step() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Hello");
    app.press(Key::Enter);
    type_text(&mut app, "World");
    app.press(Key::Escape);

    assert_eq!(app.doc().undo_len(), 1);
    app.key_with(egui::Modifiers::COMMAND, Key::Z);
    assert!(app.doc().annotations().is_empty());
    app.key_with(egui::Modifiers::COMMAND, Key::Y);
    assert_eq!(text_of(&app, 0), "Hello\nWorld");
}

// ---- Editing later ----

#[test]
fn double_click_edits_a_text_annotation_as_one_more_step() {
    let mut app = app_with_hello();
    double_click_px(&mut app, (60.0, 70.0));
    assert!(editing(&app));
    type_text(&mut app, ", world");
    app.press(Key::Escape);

    assert_eq!(app.doc().annotations().len(), 1);
    assert_eq!(text_of(&app, 0), "Hello, world");
    assert_eq!(app.doc().undo_len(), 2);
    app.key_with(egui::Modifiers::COMMAND, Key::Z);
    assert_eq!(text_of(&app, 0), "Hello");
}

#[test]
fn emptying_an_existing_box_deletes_it() {
    let mut app = app_with_hello();
    double_click_px(&mut app, (60.0, 70.0));
    for _ in 0..5 {
        app.press(Key::Backspace);
    }
    app.press(Key::Escape);

    assert!(app.doc().annotations().is_empty());
    app.key_with(egui::Modifiers::COMMAND, Key::Z);
    assert_eq!(text_of(&app, 0), "Hello");
}

#[test]
fn the_text_tool_clicking_on_a_text_annotation_edits_it() {
    let mut app = app_with_hello();
    app.press(Key::T);
    app.click_px((60.0, 70.0));
    assert!(editing(&app));
    type_text(&mut app, "!");
    app.press(Key::Escape);
    assert_eq!(app.doc().annotations().len(), 1);
    assert_eq!(text_of(&app, 0), "Hello!");
}

// ---- Width ----

#[test]
fn dragging_the_right_handle_sets_a_fixed_width_and_the_text_wraps() {
    let mut app = app_with_new_box();
    type_text(&mut app, "the quick brown fox");
    app.press(Key::Escape);
    app.press(Key::Escape);
    app.click_px((60.0, 70.0));
    let wide = bounds_of(&app, 0);

    let right = (wide.max.x, wide.center().y);
    let to_x = wide.min.x + wide.width() / 2.0;
    app.drag_px(right, (to_x, right.1));
    let narrow = bounds_of(&app, 0);
    assert!((narrow.max.x - to_x).abs() < 1.0, "{narrow:?}");
    assert!((narrow.min.x - wide.min.x).abs() < 0.5);
    assert!(narrow.height() > wide.height() * 1.5, "wraps: {narrow:?}");

    // Typing more keeps the width and adds lines.
    double_click_px(&mut app, (60.0, 70.0));
    type_text(&mut app, " jumps over the lazy dog");
    app.press(Key::Escape);
    let longer = bounds_of(&app, 0);
    assert!((longer.width() - narrow.width()).abs() < 0.5);
    assert!(longer.height() > narrow.height());
}

#[test]
fn dragging_the_left_handle_sets_the_width_and_keeps_the_right_edge() {
    let mut app = app_with_hello();
    app.click_px((60.0, 70.0));
    let before = bounds_of(&app, 0);
    app.drag_px((before.min.x, before.center().y), (20.0, before.center().y));
    let after = bounds_of(&app, 0);
    assert!((after.min.x - 20.0).abs() < 1.0, "{after:?}");
    assert!((after.max.x - before.max.x).abs() < 1.0, "{after:?}");
    assert_eq!(app.doc().undo_len(), 2);
}

// ---- Style ----

#[test]
fn the_property_bar_shows_the_text_controls() {
    let app = app_with_new_box();
    for label in [
        "Font",
        "Size",
        "Text colour",
        "Fill colour",
        "No fill",
        "Shadow",
    ] {
        app.control(label);
    }
    assert!(app.harness.query_by_label("Thickness").is_none());
}

#[test]
fn a_bigger_size_restyles_the_box_and_becomes_the_default() {
    let mut app = app_with_hello();
    app.click_px((60.0, 70.0));
    let before = bounds_of(&app, 0);
    app.harness.state_mut().execute(Command::SetProperty {
        property: Property::FontSize(40.0),
        merge: false,
    });
    app.settle();
    let after = bounds_of(&app, 0);
    assert!(after.height() > before.height() * 1.6, "{after:?}");
    assert!(after.width() > before.width() * 1.6, "{after:?}");
    assert_eq!(app.doc().undo_len(), 2);
}

#[test]
fn restyling_while_editing_is_part_of_the_session_step() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Hi");
    app.harness.state_mut().execute(Command::SetProperty {
        property: Property::TextColour(Rgba::rgb(0, 0, 255)),
        merge: false,
    });
    app.settle();
    assert!(editing(&app));
    type_text(&mut app, "!");
    app.press(Key::Escape);

    let a = &app.doc().annotations()[0];
    assert_eq!(a.style.line, Rgba::rgb(0, 0, 255));
    assert_eq!(text_of(&app, 0), "Hi!");
    assert_eq!(app.doc().undo_len(), 1);
    // And the text tool's default for the next box.
    assert_eq!(
        app.harness.state().tool_style(ToolId::Text).line,
        Rgba::rgb(0, 0, 255)
    );
}

#[test]
fn the_text_and_its_fill_are_drawn_in_their_colours() {
    let mut app = app_with_image();
    app.press(Key::T);
    for property in [
        Property::TextColour(Rgba::rgb(0, 0, 255)),
        Property::FillColour(Some(Rgba::rgb(255, 255, 0))),
        Property::Shadow(false),
    ] {
        app.harness.state_mut().execute(Command::SetProperty {
            property,
            merge: false,
        });
    }
    app.click_px((50.0, 60.0));
    type_text(&mut app, "WWW");
    app.press(Key::Escape);

    let b = bounds_of(&app, 0);
    let composite = app.harness.state().composite().expect("a composite");
    let canvas = app.harness.state().canvas().unwrap();
    // The composite covers the visible Image; at 100% one Image pixel is one composite pixel.
    let offset = canvas.image.min - canvas.area.min.max(canvas.image.min);
    assert_eq!(offset, egui::Vec2::ZERO);
    let px = |x: f32, y: f32| {
        let i = (y as usize * composite.width as usize + x as usize) * 4;
        &composite.rgba_premultiplied[i..i + 3]
    };
    // The padding inside the box is the fill.
    assert_eq!(px(b.min.x + 2.0, b.min.y + 2.0), &[255, 255, 0]);
    // Some pixels inside are the text colour.
    let mut blue = 0;
    for y in b.min.y as usize..b.max.y as usize {
        for x in b.min.x as usize..b.max.x as usize {
            if px(x as f32, y as f32) == [0, 0, 255] {
                blue += 1;
            }
        }
    }
    assert!(blue > 20, "{blue} text pixels");
}

// ---- Input-method composition ----

#[test]
fn input_method_composition_shows_the_preedit_and_inserts_the_commit() {
    let mut app = app_with_new_box();
    let ime_output = app.harness.output().platform_output.ime;
    assert!(ime_output.is_some(), "the IME is enabled while editing");

    type_text(&mut app, "A");
    ime(
        &mut app,
        ImeEvent::Preedit {
            text: "にほ".into(),
            active_range_chars: None,
        },
    );
    assert_eq!(text_of(&app, 0), "Aにほ");
    ime(
        &mut app,
        ImeEvent::Preedit {
            text: "日本".into(),
            active_range_chars: None,
        },
    );
    assert_eq!(text_of(&app, 0), "A日本");
    ime(&mut app, ImeEvent::Commit("日本".into()));
    type_text(&mut app, "!");
    app.press(Key::Escape);

    assert_eq!(text_of(&app, 0), "A日本!");
    assert_eq!(app.doc().undo_len(), 1);
    assert!(app.harness.output().platform_output.ime.is_none());
}

#[test]
fn finishing_mid_composition_drops_the_preedit() {
    let mut app = app_with_new_box();
    type_text(&mut app, "A");
    ime(
        &mut app,
        ImeEvent::Preedit {
            text: "にほ".into(),
            active_range_chars: None,
        },
    );
    app.click_px((250.0, 200.0));
    assert_eq!(text_of(&app, 0), "A");
}

// ---- Image operations (§4.7) ----

#[test]
fn rotating_the_image_moves_the_text_but_keeps_it_upright() {
    let mut app = app_with_hello();
    let before = bounds_of(&app, 0);
    app.harness
        .state_mut()
        .execute(Command::Rotate { clockwise: true });
    app.settle();

    let after = bounds_of(&app, 0);
    // Same box, not turned: width and height unchanged.
    assert!((after.width() - before.width()).abs() < 0.5, "{after:?}");
    assert!((after.height() - before.height()).abs() < 0.5, "{after:?}");
    // Its centre turned with the 320×240 Image: (x, y) → (240 - y, x).
    let c = before.center();
    let expected = (240.0 - c.y, c.x);
    assert!((after.center().x - expected.0).abs() < 1.0, "{after:?}");
    assert!((after.center().y - expected.1).abs() < 1.0, "{after:?}");
    assert_eq!(text_of(&app, 0), "Hello");
}

#[test]
fn resizing_the_image_scales_the_font_size() {
    let mut app = app_with_hello();
    let before = bounds_of(&app, 0);
    app.harness.state_mut().execute(Command::Resize {
        width: 640,
        height: 480,
    });
    app.settle();

    let a = &app.doc().annotations()[0];
    assert_eq!(
        a.style.font_size,
        2.0 * lecoq_core::Style::default().font_size
    );
    let after = a.bounds();
    assert!((after.min.x - 2.0 * before.min.x).abs() < 1.0, "{after:?}");
    assert!((after.min.y - 2.0 * before.min.y).abs() < 1.0, "{after:?}");
    assert!(
        (after.height() - 2.0 * before.height()).abs() < 3.0,
        "{after:?}"
    );
}

// ---- Saving ----

#[test]
fn ctrl_s_while_editing_finishes_the_text_and_saves_it() {
    let mut app = app_with_new_box();
    type_text(&mut app, "Saved");
    app.key_with(egui::Modifiers::COMMAND, Key::S);

    assert!(!editing(&app));
    assert_eq!(text_of(&app, 0), "Saved");
    assert_eq!(app.doc().undo_len(), 1);
    assert!(!app.doc().is_modified());
}
