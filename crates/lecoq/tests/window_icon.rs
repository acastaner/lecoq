//! The window / title-bar icon embedded by the startup path (§1.3): it must be a valid, non-empty
//! square PNG so eframe uses lecoq's own brand art instead of its default icon.

#[test]
fn the_embedded_window_icon_decodes_to_a_non_empty_square() {
    let icon = eframe::icon_data::from_png_bytes(lecoq::WINDOW_ICON_PNG)
        .expect("the embedded window icon decodes as PNG");

    assert!(icon.width > 0, "the icon has a non-zero width");
    assert_eq!(icon.width, icon.height, "the icon is square");
    assert_eq!(
        icon.rgba.len(),
        (icon.width * icon.height * 4) as usize,
        "the icon pixels are RGBA",
    );
}
