//! Save (#34): Ctrl+S and Save As write the Flattened Image safely, keeping metadata (§5.1, §5.3,
//! §6.1, §7.2).

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};
use image::ImageDecoder;
use lecoq_platform::{MemoryConfigStore, MemoryFileDialogs, Platform};
use support::TestApp;

/// A headless App with scripted file dialogs (and an optional `config.toml`).
fn app_with(config: Option<&str>) -> (TestApp, Arc<MemoryFileDialogs>) {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    if let Some(text) = config {
        platform = platform.with_config(Arc::new(MemoryConfigStore::with_text(text)));
    }
    platform.dialogs = dialogs.clone();
    (TestApp::with_platform(platform), dialogs)
}

/// Open `fixture` (copied into the temp dir); returns its path.
fn open(app: &mut TestApp, fixture: &str) -> PathBuf {
    let path = app.add_fixtures(&[fixture]);
    app.open(path.clone());
    path
}

/// Draw a rectangle from (10, 10) to (40, 30) with the rectangle tool, then back to select.
fn draw_rectangle(app: &mut TestApp) {
    app.press(Key::R);
    app.drag_px((10.0, 10.0), (40.0, 30.0));
    app.press(Key::Escape);
}

fn save(app: &mut TestApp) {
    app.key_with(Modifiers::COMMAND, Key::S);
}

fn save_as(app: &mut TestApp) {
    app.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
}

fn file_name_field(app: &TestApp) -> String {
    app.status("File name")
}

/// The file's (ICC profile, EXIF chunk), as read back by the `image` crate.
fn metadata(path: &Path) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let mut decoder = image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .into_decoder()
        .unwrap();
    (
        decoder.icc_profile().unwrap(),
        decoder.exif_metadata().unwrap(),
    )
}

fn format_of(path: &Path) -> image::ImageFormat {
    image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .format()
        .unwrap()
}

/// The WebP chunk holding the bitstream: `VP8 ` (lossy) or `VP8L` (lossless).
fn webp_bitstream(path: &Path) -> String {
    let data = std::fs::read(path).unwrap();
    assert_eq!(&data[0..4], b"RIFF");
    assert_eq!(&data[8..12], b"WEBP");
    let mut at = 12;
    while at + 8 <= data.len() {
        let fourcc = &data[at..at + 4];
        if fourcc == b"VP8 " || fourcc == b"VP8L" {
            return String::from_utf8_lossy(fourcc).into_owned();
        }
        let len = u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap()) as usize;
        at += 8 + len + (len & 1);
    }
    panic!("no bitstream chunk in {}", path.display());
}

fn close(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tolerance)
}

fn rgb_at(path: &Path, x: u32, y: u32) -> [u8; 3] {
    image::open(path).unwrap().into_rgb8().get_pixel(x, y).0
}

// ---- Ctrl+S ----

#[test]
fn ctrl_s_writes_the_flattened_image_back_in_its_own_format_and_clears_the_marker() {
    let (mut app, _) = app_with(None);
    let path = open(&mut app, "small-320x240.jpg");
    let before = rgb_at(&path, 25, 10);
    let away = rgb_at(&path, 200, 200);
    draw_rectangle(&mut app);
    assert_eq!(file_name_field(&app), "small-320x240.jpg •");

    save(&mut app);

    assert_eq!(file_name_field(&app), "small-320x240.jpg");
    assert_eq!(format_of(&path), image::ImageFormat::Jpeg);
    // The rectangle's red outline (default style) is now in the file's pixels.
    let after = rgb_at(&path, 25, 10);
    assert!(
        after[0] > 180 && after[1] < 90 && after[2] < 90,
        "outline pixel {after:?} (was {before:?})"
    );
    // Away from the rectangle the Image is unchanged (within JPEG re-encoding).
    assert!(close(rgb_at(&path, 200, 200), away, 8));
}

#[test]
fn saving_keeps_the_history_and_undo_after_a_save_marks_the_image_unsaved() {
    let (mut app, _) = app_with(None);
    open(&mut app, "small-320x240.jpg");
    draw_rectangle(&mut app);
    save(&mut app);
    assert_eq!(app.doc().undo_len(), 1);
    assert!(!app.doc().is_modified());

    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(file_name_field(&app), "small-320x240.jpg •");
    // Redo comes back to the saved state.
    app.key_with(Modifiers::COMMAND, Key::Y);
    assert_eq!(file_name_field(&app), "small-320x240.jpg");
}

#[test]
fn the_saved_file_matches_the_screen() {
    // Flatten (§7.2) is the screen composite's own Skia code at full resolution. The 320×240
    // Image is shown at 100%, so the composite and the file have the same pixels.
    let (mut app, dialogs) = app_with(None);
    let path = open(&mut app, "small-320x240.jpg");
    draw_rectangle(&mut app);
    app.press(Key::R);
    app.drag_px((100.0, 60.0), (300.0, 220.0));
    let target = path.with_file_name("flattened.png");
    dialogs.answer_next(Some(target.clone()));
    save_as(&mut app);

    let saved = image::open(&target).unwrap().into_rgba8();
    support::assert_golden_image_against(&saved, &app.composite_image());
}

#[test]
fn ctrl_s_with_nothing_edited_still_writes_the_file() {
    let (mut app, _) = app_with(None);
    let path = open(&mut app, "png-64x48.png");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    save(&mut app);
    assert!(std::fs::metadata(&path).unwrap().modified().unwrap() > modified);
    assert_eq!(format_of(&path), image::ImageFormat::Png);
}

#[test]
fn ctrl_s_on_a_read_only_format_acts_as_save_as_defaulting_to_png() {
    for fixture in ["bmp-64x48.bmp", "tiff-64x48.tif", "ico-48x48.ico"] {
        let (mut app, dialogs) = app_with(None);
        let path = open(&mut app, fixture);
        draw_rectangle(&mut app);
        let suggested = path.with_extension("png");
        dialogs.answer_next(Some(suggested.clone()));

        save(&mut app);

        assert_eq!(dialogs.opened(), 1, "{fixture}");
        assert_eq!(
            dialogs.last_suggested(),
            Some(suggested.clone()),
            "{fixture}"
        );
        assert_eq!(format_of(&suggested), image::ImageFormat::Png, "{fixture}");
        // The original is untouched; the window now shows the saved file, unmodified.
        assert_eq!(
            std::fs::read(&path).unwrap(),
            std::fs::read(support::fixture_path(fixture)).unwrap()
        );
        let name = suggested
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(file_name_field(&app), name, "{fixture}");

        // Ctrl+S again now writes the PNG in place, without a dialog.
        draw_rectangle(&mut app);
        save(&mut app);
        assert_eq!(dialogs.opened(), 1, "{fixture}");
        assert_eq!(file_name_field(&app), name, "{fixture}");
    }
}

#[test]
fn cancelling_save_as_writes_nothing_and_keeps_the_marker() {
    let (mut app, dialogs) = app_with(None);
    open(&mut app, "bmp-64x48.bmp");
    draw_rectangle(&mut app);
    dialogs.answer_next(None);
    save(&mut app);
    assert_eq!(dialogs.opened(), 1);
    assert_eq!(file_name_field(&app), "bmp-64x48.bmp •");
    let files = std::fs::read_dir(app.dir.path()).unwrap().count();
    assert_eq!(files, 1);
}

// ---- Save As ----

#[test]
fn save_as_writes_png_jpeg_or_webp_by_the_chosen_extension() {
    for (name, format) in [
        ("out.png", image::ImageFormat::Png),
        ("out.jpg", image::ImageFormat::Jpeg),
        ("out.jpeg", image::ImageFormat::Jpeg),
        ("out.webp", image::ImageFormat::WebP),
    ] {
        let (mut app, dialogs) = app_with(None);
        let path = open(&mut app, "small-320x240.jpg");
        draw_rectangle(&mut app);
        let target = path.with_file_name(name);
        dialogs.answer_next(Some(target.clone()));
        save_as(&mut app);
        assert_eq!(format_of(&target), format, "{name}");
        assert_eq!(image::open(&target).unwrap().width(), 320, "{name}");
        assert_eq!(file_name_field(&app), name);
    }
}

#[test]
fn save_as_suggests_the_current_file_and_adds_png_to_a_name_without_a_known_extension() {
    let (mut app, dialogs) = app_with(None);
    let path = open(&mut app, "small-320x240.jpg");
    dialogs.answer_next(Some(path.with_file_name("plain")));
    save_as(&mut app);
    assert_eq!(dialogs.last_suggested(), Some(path.clone()));
    assert_eq!(
        dialogs.last_title().as_deref(),
        Some(lecoq::strings::SAVE_AS_DIALOG_TITLE)
    );
    let written = path.with_file_name("plain.png");
    assert_eq!(format_of(&written), image::ImageFormat::Png);
}

#[test]
fn webp_is_lossless_from_a_lossless_source_and_lossy_from_a_lossy_one() {
    for (fixture, chunk) in [("png-64x48.png", "VP8L"), ("small-320x240.jpg", "VP8 ")] {
        let (mut app, dialogs) = app_with(None);
        let path = open(&mut app, fixture);
        let target = path.with_file_name("out.webp");
        dialogs.answer_next(Some(target.clone()));
        save_as(&mut app);
        assert_eq!(webp_bitstream(&target), chunk, "{fixture}");
    }
}

#[test]
fn ctrl_s_on_a_webp_keeps_it_webp_with_its_own_compression_and_alpha() {
    let (mut app, dialogs) = app_with(None);
    let path = open(&mut app, "rgba-64x48.png");
    let webp = path.with_file_name("alpha.webp");
    dialogs.answer_next(Some(webp.clone()));
    save_as(&mut app);
    draw_rectangle(&mut app);
    save(&mut app);
    assert_eq!(format_of(&webp), image::ImageFormat::WebP);
    assert_eq!(webp_bitstream(&webp), "VP8L");
    let original = image::open(&path).unwrap().into_rgba8();
    let saved = image::open(&webp).unwrap().into_rgba8();
    // Transparency survives, outside the rectangle.
    assert_eq!(saved.get_pixel(60, 45), original.get_pixel(60, 45));
}

#[test]
fn jpeg_uses_the_configured_quality() {
    let size_at = |config: Option<&str>| {
        let (mut app, _) = app_with(config);
        let path = open(&mut app, "large-2400x1600.jpg");
        draw_rectangle(&mut app);
        save(&mut app);
        std::fs::metadata(&path).unwrap().len()
    };
    let default = size_at(None);
    let explicit = size_at(Some("version = 1\njpeg_quality = 92\n"));
    let low = size_at(Some("version = 1\njpeg_quality = 30\n"));
    assert_eq!(default, explicit);
    assert!(
        low * 4 < default * 3,
        "quality 30: {low} bytes, 92: {default}"
    );
}

// ---- Metadata (§5.3) ----

#[test]
fn exif_is_kept_with_the_orientation_set_to_1() {
    for target in ["exif-orientation-6.jpg", "out.png", "out.webp"] {
        let (mut app, dialogs) = app_with(None);
        let path = open(&mut app, "exif-orientation-6.jpg");
        let (_, original_exif) = metadata(&path);
        let original_exif = original_exif.expect("the fixture has EXIF");
        let shown = app.pixel(5, 5).unwrap();
        let out = path.with_file_name(target);
        if target == "exif-orientation-6.jpg" {
            save(&mut app);
        } else {
            dialogs.answer_next(Some(out.clone()));
            save_as(&mut app);
        }

        let (_, exif) = metadata(&out);
        let exif = exif.unwrap_or_else(|| panic!("{target}: EXIF dropped"));
        assert_eq!(
            image::metadata::Orientation::from_exif_chunk(&exif),
            Some(image::metadata::Orientation::NoTransforms),
            "{target}"
        );
        // Everything else in the EXIF is as it was.
        assert_eq!(exif.len(), original_exif.len(), "{target}");
        let differing = exif
            .iter()
            .zip(&original_exif)
            .filter(|(a, b)| a != b)
            .count();
        assert!(differing <= 2, "{target}: {differing} EXIF bytes changed");
        // Orientation was baked into the pixels: reopened, it looks the same.
        app.open(out.clone());
        let again = app.pixel(5, 5).unwrap();
        assert!(
            close(
                [again.r(), again.g(), again.b()],
                [shown.r(), shown.g(), shown.b()],
                12
            ),
            "{target}: {again:?} vs {shown:?}"
        );
    }
}

#[test]
fn an_srgb_icc_profile_is_kept_as_it_was() {
    let (mut app, _) = app_with(None);
    let path = open(&mut app, "red-srgb-icc.jpg");
    let (icc, _) = metadata(&path);
    save(&mut app);
    assert_eq!(metadata(&path).0, icc);
    assert!(icc.is_some());
}

#[test]
fn a_profile_converted_on_decode_is_replaced_by_srgb() {
    for target in ["red-swapped-icc.jpg", "out.png", "out.webp"] {
        let (mut app, dialogs) = app_with(None);
        let path = open(&mut app, "red-swapped-icc.jpg");
        let (original_icc, _) = metadata(&path);
        let shown = app.pixel(10, 10).unwrap();
        let out = path.with_file_name(target);
        if target == "red-swapped-icc.jpg" {
            save(&mut app);
        } else {
            dialogs.answer_next(Some(out.clone()));
            save_as(&mut app);
        }
        let (icc, _) = metadata(&out);
        let icc = icc.unwrap_or_else(|| panic!("{target}: no ICC profile written"));
        assert_ne!(Some(&icc), original_icc.as_ref(), "{target}");
        // The file now says sRGB, so reopening doesn't convert again: the colours are the same.
        app.open(out.clone());
        let again = app.pixel(10, 10).unwrap();
        assert!(
            close(
                [again.r(), again.g(), again.b()],
                [shown.r(), shown.g(), shown.b()],
                12
            ),
            "{target}: {again:?} vs {shown:?}"
        );
    }
}

// ---- Safe write ----

#[test]
fn a_save_leaves_no_temporary_file_behind() {
    let (mut app, _) = app_with(None);
    open(&mut app, "small-320x240.jpg");
    draw_rectangle(&mut app);
    save(&mut app);
    let names: Vec<_> = std::fs::read_dir(app.dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["small-320x240.jpg"]);
}

#[test]
fn a_failed_save_says_why_and_keeps_the_marker() {
    let (mut app, dialogs) = app_with(None);
    let path = open(&mut app, "small-320x240.jpg");
    draw_rectangle(&mut app);
    let target = path.with_file_name("missing-dir").join("out.png");
    dialogs.answer_next(Some(target.clone()));
    save_as(&mut app);
    assert!(!target.exists());
    assert_eq!(file_name_field(&app), "small-320x240.jpg •");
    let message = app.status("Message");
    assert!(message.starts_with("Can't save out.png: "), "{message}");
}

#[test]
fn a_webp_the_encoder_refuses_says_so_with_its_code() {
    // WebP is limited to 16383 px a side: libwebp refuses a wider Image.
    let (mut app, dialogs) = app_with(None);
    let path = app.dir.path().join("wide.png");
    image::RgbImage::new(16_400, 1).save(&path).unwrap();
    app.open(path.clone());
    let target = path.with_file_name("wide.webp");
    dialogs.answer_next(Some(target.clone()));
    save_as(&mut app);
    assert!(!target.exists());
    let message = app.status("Message");
    assert!(
        message.starts_with("Can't save wide.webp: WebP encoding failed (VP8_ENC_ERROR_"),
        "{message}"
    );
}

// ---- With other features ----

#[test]
fn saving_after_a_crop_writes_the_cropped_image() {
    // After an Image operation the Document's pixels are the Image (#32): Flatten uses them.
    let (mut app, _) = app_with(None);
    let path = open(&mut app, "small-320x240.jpg");
    let original = image::open(&path).unwrap().into_rgb8();
    app.press(Key::C);
    app.drag_px((40.0, 30.0), (240.0, 180.0));
    app.press(Key::Enter);
    save(&mut app);

    let saved = image::open(&path).unwrap().into_rgb8();
    assert_eq!(saved.dimensions(), (200, 150));
    let (a, b) = (saved.get_pixel(100, 75).0, original.get_pixel(140, 105).0);
    assert!(close(a, b, 12), "{a:?} vs {b:?}");
    assert_eq!(file_name_field(&app), "small-320x240.jpg");
}

#[test]
fn saving_a_file_deleted_from_outside_writes_it_back_and_clears_the_marker() {
    let mut app = TestApp::open_in_folder("small-320x240.jpg", &["a.jpg", "b.jpg"], "b.jpg");
    let path = app.dir.path().join("b.jpg");
    std::fs::remove_file(&path).unwrap();
    app.wait_for_status("File name", "b.jpg •");

    save(&mut app);

    assert!(path.exists());
    assert_eq!(file_name_field(&app), "b.jpg");
    assert!(!app.harness.state().is_modified());
}
