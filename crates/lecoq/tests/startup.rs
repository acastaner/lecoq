//! Launching with a path (§1.3, §3.5).

mod support;

use support::TestApp;

#[test]
fn launching_with_a_jpeg_shows_it() {
    let mut app = TestApp::launch(&["small-320x240.jpg"]);
    app.settle();

    assert_eq!(app.status("File name"), "small-320x240.jpg");
    assert_eq!(app.status("Pixel size"), "320×240");
    assert_eq!(app.status("Zoom"), "Fit");
    assert!(app.harness.state().canvas().is_some());
}

#[test]
fn status_bar_is_empty_until_an_image_is_decoded() {
    let app = TestApp::new();

    assert_eq!(app.status_field("File name"), None);
    assert_eq!(app.status_field("Pixel size"), None);
    assert_eq!(app.status_field("Zoom"), None);
}
