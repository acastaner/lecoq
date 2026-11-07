//! The headless App harness (spec §9): the real lecoq app under `egui_kittest`, with the platform
//! adapters replaced by in-memory fakes and fixtures copied into a temporary directory.
//!
//! Each integration test file does `mod support;` and drives the app through [`TestApp`].
#![allow(dead_code)] // Not every test file uses every helper.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lecoq::{Command, Launch, LecoqApp};
use lecoq_platform::Platform;

/// Logical window size of the headless App, in points (1 point = 1 pixel).
pub const WINDOW: egui::Vec2 = egui::vec2(1000.0, 700.0);

/// A file dropped onto the window, as the windowing layer hands it to egui.
#[derive(Debug)]
struct DroppedPath(PathBuf);

impl egui::DroppedFile for DroppedPath {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

pub struct TestApp {
    pub harness: Harness<'static, LecoqApp>,
    /// Holds the copied fixtures; dropped (deleted) with the TestApp.
    pub dir: tempfile::TempDir,
}

/// Path of a fixture in the repository's `tests/fixtures/`.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Copy fixtures into `dir`; returns the path of the first copy.
fn copy_fixtures(dir: &Path, names: &[&str]) -> PathBuf {
    let copies: Vec<PathBuf> = names
        .iter()
        .map(|name| {
            let to = dir.join(name);
            std::fs::copy(fixture_path(name), &to).expect("copy fixture");
            to
        })
        .collect();
    copies[0].clone()
}

/// Path of a golden image in the repository's `tests/golden/`.
pub fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(format!("{name}.png"))
}

/// A channel differing by more than this counts as a differing pixel.
const GOLDEN_CHANNEL_TOLERANCE: u8 = 10;
/// At most this fraction of pixels may differ.
const GOLDEN_PIXEL_TOLERANCE: f64 = 0.002;

/// Compare `actual` with a golden PNG (see [`TestApp::assert_golden`]). Pixels are compared as
/// stored (the canvas is opaque, so premultiplied and straight alpha agree).
pub fn assert_golden_image(actual: &image::RgbaImage, name: &str) {
    let path = golden_path(name);
    if std::env::var_os("LECOQ_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        actual.save(&path).expect("write golden");
        return;
    }
    let expected = image::open(&path)
        .unwrap_or_else(|e| {
            panic!(
                "golden {} unreadable ({e}); run with LECOQ_UPDATE_GOLDEN=1 to create it",
                path.display()
            )
        })
        .into_rgba8();
    assert_similar(actual, &expected, name);
}

/// Compare two images within the golden tolerance (see [`assert_golden_image`]).
pub fn assert_golden_image_against(actual: &image::RgbaImage, expected: &image::RgbaImage) {
    assert_similar(actual, expected, "comparison");
}

fn assert_similar(actual: &image::RgbaImage, expected: &image::RgbaImage, name: &str) {
    assert_eq!(
        expected.dimensions(),
        actual.dimensions(),
        "golden {name}: size differs"
    );
    let differing = expected
        .pixels()
        .zip(actual.pixels())
        .filter(|(e, a)| {
            e.0.iter()
                .zip(a.0.iter())
                .any(|(x, y)| x.abs_diff(*y) > GOLDEN_CHANNEL_TOLERANCE)
        })
        .count();
    let total = (expected.width() * expected.height()) as f64;
    if differing as f64 > total * GOLDEN_PIXEL_TOLERANCE {
        let out = std::env::temp_dir().join(format!("lecoq-golden-{name}.actual.png"));
        let _ = actual.save(&out);
        panic!(
            "golden {name}: {differing} of {total} pixels differ (actual written to {})",
            out.display()
        );
    }
}

/// A [`Launch`] with the Sibling order pinned to GLib's in `en_US.UTF-8`, the order the ordering
/// tables were measured in, so the tests don't depend on the desktop or locale they run under.
fn launch_for_tests(platform: Platform) -> Launch {
    let mut launch = Launch::new(platform);
    launch.sibling_order = test_sibling_order;
    launch
}

fn test_sibling_order() -> Box<dyn lecoq_siblings::order::NameOrder> {
    Box::new(
        lecoq_siblings::order::GlibOrder::with_locale("en_US.UTF-8")
            .expect("the en_US.UTF-8 locale must be installed for the headless App tests"),
    )
}

impl TestApp {
    /// A headless App with no Image open, in-memory platform fakes and an empty temporary directory.
    pub fn new() -> Self {
        Self::with_platform(Platform::in_memory())
    }

    pub fn with_platform(platform: Platform) -> Self {
        Self::from_launch(launch_for_tests(platform))
    }

    /// A headless App with no Image open, its [`Launch`] (in-memory platform) adjusted by `adjust`.
    pub fn with_launch(adjust: impl FnOnce(&mut Launch)) -> Self {
        let mut launch = launch_for_tests(Platform::in_memory());
        adjust(&mut launch);
        Self::from_launch(launch)
    }

    fn from_launch(launch: Launch) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let harness = Harness::builder()
            .with_size(WINDOW)
            .build_eframe(move |cc| LecoqApp::new(cc, launch));
        Self { harness, dir }
    }

    /// Open `name` in a headless App with a custom logical window size (1 point = 1 pixel), for the
    /// #55 pan-frame measurement, which wants a ~2560×1440 canvas. The harness is built empty (no
    /// texture uploaded yet), so the default 2048 texture-side cap can be raised before the display
    /// Image and composite — both wider than 2048 at this size — are uploaded (a real GPU reports
    /// 8k+); then the Image is opened.
    pub fn open_fixture_sized(name: &str, size: egui::Vec2) -> Self {
        let launch = launch_for_tests(Platform::in_memory());
        let dir = tempfile::tempdir().expect("temp dir");
        let mut harness = Harness::builder()
            .with_size(size)
            .build_eframe(move |cc| LecoqApp::new(cc, launch));
        harness.input_mut().max_texture_side = Some(16384);
        let mut app = Self { harness, dir };
        let path = app.add_fixtures(&[name]);
        app.open(path);
        app
    }

    /// Start a headless App the way `lecoq <path>` starts: the first-image decode is already
    /// running when the app is created (§1.3). Copies the fixtures first; the first one is launched.
    /// Doesn't wait: call [`Self::settle`] (or step the harness) yourself.
    pub fn launch(names: &[&str]) -> Self {
        Self::launch_scaled(names, 1.0)
    }

    /// Like [`Self::launch`], on a HiDPI screen of `pixels_per_point` physical pixels per point. The
    /// window is still [`WINDOW`] points.
    pub fn launch_scaled(names: &[&str], pixels_per_point: f32) -> Self {
        Self::launch_scaled_on(Platform::in_memory(), names, pixels_per_point)
    }

    /// Like [`Self::launch`], with these platform adapters.
    pub fn launch_on(platform: Platform, names: &[&str]) -> Self {
        Self::launch_scaled_on(platform, names, 1.0)
    }

    fn launch_scaled_on(platform: Platform, names: &[&str], pixels_per_point: f32) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = copy_fixtures(dir.path(), names);
        let mut launch = launch_for_tests(platform);
        launch.decode_first_image(path);
        let harness = Harness::builder()
            .with_size(WINDOW)
            .with_pixels_per_point(pixels_per_point)
            .build_eframe(move |cc| LecoqApp::new(cc, launch));
        Self { harness, dir }
    }

    /// Like [`Self::launch`], but the first-image decode doesn't finish until the returned sender
    /// sends (or is dropped), so a test can step frame by frame from the first image on. (Building
    /// the harness runs frames until the app is idle, which would otherwise race the decode.)
    pub fn launch_held(names: &[&str]) -> (Self, std::sync::mpsc::Sender<()>) {
        Self::launch_held_on(Platform::in_memory(), names)
    }

    /// Like [`Self::launch_held`], with these platform adapters.
    pub fn launch_held_on(
        platform: Platform,
        names: &[&str],
    ) -> (Self, std::sync::mpsc::Sender<()>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = copy_fixtures(dir.path(), names);
        let mut launch = launch_for_tests(platform);
        let (release, held) = std::sync::mpsc::channel::<()>();
        launch.decode_first_image_after(path, move || {
            let _ = held.recv();
        });
        let harness = Harness::builder()
            .with_size(WINDOW)
            .build_eframe(move |cc| LecoqApp::new(cc, launch));
        (Self { harness, dir }, release)
    }

    /// Copy the named fixtures into the temporary directory and return the path of the first copy.
    pub fn add_fixtures(&self, names: &[&str]) -> PathBuf {
        copy_fixtures(self.dir.path(), names)
    }

    /// Copy one fixture into the temporary directory under another name (to build a folder of
    /// Siblings); returns the copy's path.
    pub fn add_fixture_as(&self, fixture: &str, name: &str) -> PathBuf {
        let to = self.dir.path().join(name);
        std::fs::copy(fixture_path(fixture), &to).expect("copy fixture");
        to
    }

    /// A headless App whose temporary directory holds a copy of `fixture` under each of `names`,
    /// with `open` opened and the Sibling scan finished.
    pub fn open_in_folder(fixture: &str, names: &[&str], open: &str) -> Self {
        Self::open_in_folder_on(Platform::in_memory(), fixture, names, open)
    }

    /// Like [`Self::open_in_folder`], with these platform adapters (fakes the test keeps a handle
    /// on, such as a `MemoryTrash`).
    pub fn open_in_folder_on(
        platform: Platform,
        fixture: &str,
        names: &[&str],
        open: &str,
    ) -> Self {
        let mut app = Self::with_platform(platform);
        for name in names {
            app.add_fixture_as(fixture, name);
        }
        app.open(app.dir.path().join(open));
        app
    }

    /// Turn the mouse wheel by `lines` notches over the canvas (negative: down) and wait until idle.
    pub fn wheel(&mut self, lines: f32) {
        self.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, lines),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        self.settle();
    }

    /// Start a headless App, copy the fixture into its temporary directory, open it (the same
    /// command Ctrl+O and a drop end in), and wait until it is on screen.
    pub fn open_fixture(name: &str) -> Self {
        let mut app = Self::new();
        let path = app.add_fixtures(&[name]);
        app.open(path);
        app
    }

    /// Open a path (the same command a launch argument, Ctrl+O or a drop end in) and wait until idle.
    pub fn open(&mut self, path: impl Into<PathBuf>) {
        self.harness.state_mut().execute(Command::Open(path.into()));
        self.settle();
    }

    /// Run frames until the app has no background work pending (decodes, …) and egui stops asking for
    /// repaints. Panics after 10 s.
    pub fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // Background work (prefetch decodes) keeps asking for repaints while busy, so more
            // than `max_steps` frames are fine here; the final `run` checks the app settles.
            let _ = self.harness.try_run();
            if !self.harness.state().is_busy() {
                if self.harness.state().animation_frames() > 1 {
                    // A playing animation (§3.4) repaints for ever: its frames are shorter than
                    // kittest's 250 ms step, so egui never settles. One more frame will do.
                    self.harness.step();
                } else {
                    self.harness.run();
                }
                return;
            }
            assert!(Instant::now() < deadline, "app still busy after 10 s");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Run frames until `done` holds (something outside the app, like the directory watcher, has to
    /// catch up), then settle. Panics after 10 s, naming `what`.
    pub fn wait_until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let _ = self.harness.try_run();
            if done(self) {
                self.settle();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "still waiting for {what} after 10 s"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Wait until the status bar's `label` field reads `text`.
    pub fn wait_for_status(&mut self, label: &str, text: &str) {
        let what = format!("{label:?} to read {text:?}");
        self.wait_until(&what, |app| {
            app.status_field(label).as_deref() == Some(text)
        });
    }

    /// Press and release a key (no modifiers), then settle.
    pub fn press(&mut self, key: egui::Key) {
        self.harness.key_press(key);
        self.settle();
    }

    /// Hover `pos` (points) and return the cursor the editor asks the platform to show this frame:
    /// the single per-frame cursor decider writes it to the frame's platform output (#48, §4). Read
    /// like [`editor_text`]'s `platform_output.ime`; what the user actually sees over the canvas.
    pub fn cursor_at(&mut self, pos: egui::Pos2) -> egui::CursorIcon {
        self.harness.hover_at(pos);
        self.harness.run();
        self.harness.output().platform_output.cursor_icon
    }

    /// Ctrl+wheel with the pointer at `pos` (points): `notches` up (positive, zoom in) or down.
    pub fn ctrl_wheel_at(&mut self, pos: egui::Pos2, notches: f32) {
        self.harness.hover_at(pos);
        self.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, notches),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::COMMAND,
        });
        self.settle();
    }

    /// Drag with `button` from `from` to `to` (points), in a few pointer moves, holding Space
    /// throughout when `space` is set; then settle.
    pub fn drag(
        &mut self,
        button: egui::PointerButton,
        from: egui::Pos2,
        to: egui::Pos2,
        space: bool,
    ) {
        self.harness.hover_at(from);
        if space {
            self.harness.key_down(egui::Key::Space);
        }
        self.harness.run();
        self.harness.event(egui::Event::PointerButton {
            pos: from,
            button,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        self.harness.run();
        for step in 1..=4 {
            self.harness.hover_at(from.lerp(to, step as f32 / 4.0));
            self.harness.run();
        }
        self.harness.event(egui::Event::PointerButton {
            pos: to,
            button,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        if space {
            self.harness.key_up(egui::Key::Space);
        }
        self.settle();
    }

    /// Turn the mouse wheel `notches` with `modifiers` held and the pointer at `pos`; then settle.
    pub fn wheel_at(&mut self, pos: egui::Pos2, notches: f32, modifiers: egui::Modifiers) {
        self.harness.hover_at(pos);
        self.harness.event(egui::Event::ModifiersChanged(modifiers));
        self.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, notches),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers,
        });
        // `try_run`: a prefetch decode landing mid-run asks for more frames than `max_steps`.
        let _ = self.harness.try_run();
        self.harness
            .event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        self.settle();
    }

    /// A trackpad two-finger scroll over `pos`: a point-unit wheel carrying `delta` points on both
    /// axes, no modifiers. Unlike [`Self::wheel_at`] (notched line scrolling), this is the smooth
    /// gesture a laptop user pans a zoomed Image with.
    pub fn scroll_at(&mut self, pos: egui::Pos2, delta: egui::Vec2) {
        self.harness.hover_at(pos);
        self.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Trackpad,
            modifiers: egui::Modifiers::NONE,
        });
        self.settle();
    }

    /// Resize (or maximise) the window the way the OS does: the new size and maximised state arrive
    /// as viewport info with the next frame, which this runs.
    pub fn resize_window(&mut self, size: egui::Vec2, maximised: bool) {
        self.harness.set_size(size);
        let viewport = self
            .harness
            .input_mut()
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default();
        viewport.inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        viewport.maximized = Some(maximised);
        self.harness.run();
    }

    /// Press Ctrl (Cmd on macOS) + `key`, as the user does. Takes effect on the next frame.
    pub fn press_ctrl(&mut self, key: egui::Key) {
        self.harness
            .key_press_modifiers(egui::Modifiers::COMMAND, key);
    }

    /// Drop a file onto the window. Takes effect on the next frame.
    pub fn drop_file(&mut self, path: impl Into<PathBuf>) {
        let file = DroppedPath(path.into());
        self.harness
            .input_mut()
            .dropped_files
            .push(std::sync::Arc::new(file));
    }

    /// The text of a status-bar field, found by its accessibility label (e.g. "File name").
    ///
    /// Status-bar fields are `Role::Label` nodes whose accessible name is the field's label and
    /// whose value is the text shown. (kittest's `get_by_label` reads a `Role::Label` node's value
    /// as its label, hence the predicate.)
    pub fn status(&self, label: &str) -> String {
        self.status_field(label)
            .unwrap_or_else(|| panic!("no status-bar field labelled {label:?}"))
    }

    // ---- Editor input (in Image pixels, mapped to the screen through the canvas) ----

    /// The open Document. Panics when no Image is open.
    pub fn doc(&self) -> &lecoq_core::Document {
        self.harness.state().document().expect("an open Document")
    }

    /// Where Image pixel (x, y) is on screen, in points.
    pub fn screen(&self, x: f32, y: f32) -> egui::Pos2 {
        let canvas = self
            .harness
            .state()
            .canvas()
            .expect("an image on the canvas");
        let doc = self.doc();
        let sx = canvas.image.width() / doc.width() as f32;
        let sy = canvas.image.height() / doc.height() as f32;
        egui::pos2(canvas.image.min.x + x * sx, canvas.image.min.y + y * sy)
    }

    /// Press and release a key with modifiers, then settle.
    pub fn key_with(&mut self, modifiers: egui::Modifiers, key: egui::Key) {
        self.harness.key_press_modifiers(modifiers, key);
        self.settle();
    }

    /// Drag (left button) on the canvas from one Image pixel to another, in a few pointer moves,
    /// then settle. `_px` helpers take Image pixel coordinates, mapped through the canvas.
    pub fn drag_px(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.drag_px_with(egui::Modifiers::NONE, from, to);
    }

    pub fn drag_px_with(&mut self, modifiers: egui::Modifiers, from: (f32, f32), to: (f32, f32)) {
        let a = self.screen(from.0, from.1);
        let b = self.screen(to.0, to.1);
        let h = &self.harness;
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.hover_at(a);
        h.event(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        for t in [0.25, 0.5, 0.75, 1.0] {
            h.hover_at(a + (b - a) * t);
        }
        h.event(egui::Event::PointerButton {
            pos: b,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.remove_cursor();
        self.settle();
    }

    /// Drag (left button) on the canvas from one Image pixel to another and HOLD: press, a few
    /// pointer moves, run a frame, and stop without releasing, so the gesture is still open. Lets a
    /// test snapshot the composite (§7.2) mid-gesture, where the dirty-region fast path and the
    /// opaque-effect freeze are observable (#46). The pointer is left down; the caller ends the
    /// gesture itself (another drag, Escape, or stepping the harness) if it needs to.
    pub fn drag_hold_px(&mut self, from: (f32, f32), to: (f32, f32)) {
        let a = self.screen(from.0, from.1);
        let b = self.screen(to.0, to.1);
        let h = &self.harness;
        h.hover_at(a);
        h.event(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        for t in [0.25, 0.5, 0.75, 1.0] {
            h.hover_at(a + (b - a) * t);
        }
        // Step a few frames (no release) so the editor processes the press and drag and recomposites
        // mid-gesture. `run` can't be used: an open gesture repaints every frame (the drag keeps
        // firing while the button is held), so it never settles. The pointer stays down and the
        // gesture is still open afterwards.
        for _ in 0..3 {
            self.harness.step();
        }
    }

    /// Drag (left button) along a path of Image pixels, one pointer move per point, then settle.
    pub fn drag_path_px(&mut self, path: &[(f32, f32)]) {
        let points: Vec<egui::Pos2> = path.iter().map(|&(x, y)| self.screen(x, y)).collect();
        let (first, last) = (points[0], points[points.len() - 1]);
        let h = &self.harness;
        h.hover_at(first);
        h.event(egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        for p in &points[1..] {
            h.hover_at(*p);
        }
        h.event(egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.remove_cursor();
        self.settle();
    }

    /// Click on the canvas at an Image pixel, then settle.
    pub fn click_px(&mut self, at: (f32, f32)) {
        self.click_px_with(egui::Modifiers::NONE, at);
    }

    /// Click with modifiers held (Shift+click adds to the selection), then settle.
    pub fn click_px_with(&mut self, modifiers: egui::Modifiers, at: (f32, f32)) {
        let p = self.screen(at.0, at.1);
        let h = &self.harness;
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.hover_at(p);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            });
        }
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.remove_cursor();
        self.settle();
    }

    /// Press the secondary (right) mouse button on the canvas at an Image pixel, then release and
    /// settle. Right-click returns to the Select tool (#47); it is acted on at press, so the
    /// release carries no meaning.
    pub fn secondary_press_px(&mut self, at: (f32, f32)) {
        let p = self.screen(at.0, at.1);
        let h = &self.harness;
        h.hover_at(p);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.remove_cursor();
        self.settle();
    }

    /// The selected Annotations' bounds, in Image pixels, rounded.
    pub fn selection_bounds(&self) -> Option<[i32; 4]> {
        self.doc().selection_bounds().map(|r| {
            [
                r.min.x.round() as i32,
                r.min.y.round() as i32,
                r.max.x.round() as i32,
                r.max.y.round() as i32,
            ]
        })
    }

    /// An interactive control (button, checkbox, value) by its accessibility label, skipping the
    /// plain text labels next to it (the property bar shows "Thickness" beside the "Thickness"
    /// value).
    pub fn control(&self, label: &str) -> egui_kittest::Node<'_> {
        let wanted = label.to_owned();
        self.harness.get_by(move |node| {
            node.role() != Role::Label && node.label().as_deref() == Some(wanted.as_str())
        })
    }

    /// Like [`Self::control`], but only inside the modal dialog that is up (a quick action or a
    /// menu entry may carry the same label: "Save", "Cancel").
    pub fn dialog_control(&self, label: &str) -> egui_kittest::Node<'_> {
        let wanted = label.to_owned();
        self.harness.get_by_role(Role::Dialog).get_by(move |node| {
            node.role() != Role::Label && node.label().as_deref() == Some(wanted.as_str())
        })
    }

    /// Compare the canvas composite (§7.2) with `tests/golden/<name>.png`, within a tolerance.
    /// Set `LECOQ_UPDATE_GOLDEN=1` to (re)write the golden file instead.
    pub fn assert_golden(&self, name: &str) {
        assert_golden_image(&self.composite_image(), name);
    }

    /// The canvas composite (§7.2) as an image (premultiplied; the canvas is opaque).
    pub fn composite_image(&self) -> image::RgbaImage {
        let composite = self
            .harness
            .state()
            .composite()
            .expect("a Skia composite on the canvas");
        image::RgbaImage::from_raw(
            composite.width,
            composite.height,
            composite.rgba_premultiplied.clone(),
        )
        .expect("composite buffer matches its size")
    }

    /// A pixel of the Image on screen, in the display-size decode's coordinates
    /// (`canvas().decoded_size`), after orientation and colour conversion. `None` without an Image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<egui::Color32> {
        self.harness.state().shown_pixel(x, y)
    }

    /// Like [`Self::status`], but `None` when the field isn't shown.
    pub fn status_field(&self, label: &str) -> Option<String> {
        let wanted = label.to_owned();
        self.harness
            .query_by(move |node| {
                node.role() == Role::Label && node.label().as_deref() == Some(wanted.as_str())
            })
            .map(|node| node.value().unwrap_or_default())
    }
}
