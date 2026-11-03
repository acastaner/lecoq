//! Images over 100 MP open view-only (§4.7).

mod support;

use std::path::PathBuf;

use egui::Key;
use egui_kittest::kittest::{NodeT, Queryable};
use lecoq::tools::ToolId;
use support::TestApp;

/// A uniform JPEG just over 100 MP (10 000 × 10 001), generated once per target directory: too
/// big to commit, and slow enough to encode that it's worth keeping between runs.
fn huge_jpeg() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("huge-10000x10001.jpg");
    if !path.exists() {
        let (w, h) = (10_000u32, 10_001u32);
        let pixels = image::GrayImage::from_pixel(w, h, image::Luma([128]));
        let tmp = path.with_extension("tmp");
        pixels
            .save_with_format(&tmp, image::ImageFormat::Jpeg)
            .expect("write the huge fixture");
        std::fs::rename(&tmp, &path).expect("move the huge fixture in place");
    }
    path
}

#[test]
fn an_image_over_100_megapixels_opens_view_only_with_the_tools_disabled() {
    let mut app = TestApp::new();
    let path = app.dir.path().join("huge.jpg");
    std::fs::copy(huge_jpeg(), &path).unwrap();
    app.open(path);

    assert_eq!(app.status("Pixel size"), "10000×10001");
    assert!(app.harness.state().is_view_only());
    assert_eq!(app.status("Message"), "View only: image too large to edit");
    let rectangle = app.control("Rectangle");
    assert!(rectangle.accesskit_node().is_disabled());
    assert!(app.harness.query_by_label("Thickness").is_none());

    app.press(Key::R);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
    app.drag_px((1000.0, 1000.0), (5000.0, 5000.0));
    assert!(app.doc().annotations().is_empty());
    assert!(app.doc().full_resolution().is_none());
}

#[test]
fn an_image_up_to_100_megapixels_is_editable() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    assert!(!app.harness.state().is_view_only());
    assert_eq!(app.status_field("Message"), None);
    app.press(Key::R);
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);
}
