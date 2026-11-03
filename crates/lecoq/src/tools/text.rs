//! Text (T, §4.4): a click places a box and starts editing it; double-click edits one later.
//!
//! The pointer side lives in [`crate::editor::Editor`] (a text session outlives the click that
//! starts it and takes the keyboard), so [`TextTool`] itself ignores the pointer. A
//! [`TextSession`] is one editing session: it opens a gesture on the Document when it starts and
//! commits it when it finishes, so the whole session is one History step (§4.8).
//!
//! The text is edited in place, laid out by the renderer exactly as it is drawn; the caret and the
//! input-method pre-edit text are painted on top by egui.

use egui::{Event, ImeEvent, Key};
use lecoq_core::annotation::text::TextSpec;
use lecoq_core::{AnnotationId, AnnotationShape, Document, Point, Rect, Style, Text};

use super::{Pointer, Tool, ToolContext};

/// The text tool's button and key. Clicks are handled by the editor (see the module docs).
pub struct TextTool;

impl Tool for TextTool {
    fn press(&mut self, _cx: &mut ToolContext<'_>, _pointer: Pointer) {}
    fn drag(&mut self, _cx: &mut ToolContext<'_>, _pointer: Pointer) {}
    fn release(&mut self, _cx: &mut ToolContext<'_>, _pointer: Pointer) {}
}

/// The topmost text Annotation under `p`, if that is what is on top there.
pub fn text_at(doc: &Document, p: Point, tolerance: f32) -> Option<AnnotationId> {
    let id = doc.hit_test(p, tolerance)?;
    matches!(doc.annotation(id)?.shape, AnnotationShape::Text(_)).then_some(id)
}

/// One text-editing session on one text Annotation.
pub struct TextSession {
    id: AnnotationId,
    /// The committed text (without the pre-edit text).
    text: String,
    /// Byte offset of the caret in `text`.
    cursor: usize,
    /// The input method's composition in progress, shown at the caret (§4.4).
    preedit: Option<String>,
}

/// What a session's caret and pre-edit text look like, in Image pixels.
pub struct CaretPaint {
    pub caret: Rect,
    /// Under the pre-edit text, when composing on one line.
    pub preedit: Option<Rect>,
}

impl TextSession {
    /// Place a new, empty box with its top-left corner at `at` and start editing it.
    pub fn place(doc: &mut Document, style: &Style, at: Point) -> Self {
        doc.begin_gesture();
        let id = doc.add(Text::new(at, "", style), style.clone());
        doc.select([id]);
        Self {
            id,
            text: String::new(),
            cursor: 0,
            preedit: None,
        }
    }

    /// Start editing an existing text Annotation, with the caret at the end.
    pub fn edit(doc: &mut Document, id: AnnotationId) -> Option<Self> {
        let AnnotationShape::Text(t) = &doc.annotation(id)?.shape else {
            return None;
        };
        let text = t.text().to_owned();
        doc.begin_gesture();
        doc.select([id]);
        Some(Self {
            id,
            cursor: text.len(),
            text,
            preedit: None,
        })
    }

    pub fn id(&self) -> AnnotationId {
        self.id
    }

    /// Is `p` inside the box being edited?
    pub fn contains(&self, doc: &Document, p: Point, tolerance: f32) -> bool {
        doc.annotation(self.id)
            .is_some_and(|a| a.bounds().expand(tolerance).contains(p))
    }

    /// A click inside the box moves the caret there.
    pub fn click(&mut self, doc: &Document, p: Point) {
        if self.preedit.is_some() {
            return;
        }
        if let Some((spec_text, origin, style, width)) = self.layout(doc) {
            let spec = spec(&spec_text, &style, width);
            self.cursor = lecoq_render::text::index_at(&spec, p.x - origin.x, p.y - origin.y)
                .min(self.text.len());
        }
    }

    /// Apply this frame's keyboard and input-method events. Returns true when the session should
    /// finish (Esc).
    pub fn input(&mut self, doc: &mut Document, events: &[Event]) -> bool {
        let before = (self.text.clone(), self.preedit.clone());
        let mut finish = false;
        for event in events {
            match event {
                Event::Text(s) => self.insert(&s.replace(char::is_control, "")),
                Event::Paste(s) => self.insert(&s.replace("\r\n", "\n").replace('\r', "\n")),
                Event::Ime(ime) => self.ime(ime),
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if self.preedit.is_none() => {
                    if *key == Key::Escape {
                        finish = true;
                        break;
                    }
                    self.key(doc, *key, *modifiers);
                }
                _ => {}
            }
        }
        if (self.text.clone(), self.preedit.clone()) != before {
            self.sync(doc);
        }
        finish
    }

    /// Restyle the box being edited (part of the session's step).
    pub fn restyle(&self, doc: &mut Document, property: lecoq_core::Property) {
        if let Some(a) = doc.annotation_mut(self.id) {
            a.style.set(property);
            a.relayout();
        }
    }

    /// End the session: drop any unfinished composition, discard the box if it is empty, and
    /// record the whole session as one step.
    pub fn finish(mut self, doc: &mut Document) {
        self.preedit = None;
        self.sync(doc);
        if self.text.is_empty() {
            doc.remove(&[self.id]);
        }
        doc.commit_gesture();
    }

    /// Where the caret (and the pre-edit underline) go, in Image pixels.
    pub fn caret(&self, doc: &Document) -> Option<CaretPaint> {
        let (text, origin, style, width) = self.layout(doc)?;
        let spec = spec(&text, &style, width);
        let at = |index: usize| {
            let (x, top, h) = lecoq_render::text::caret(&spec, index);
            Rect::from_min_size(Point::new(origin.x + x, origin.y + top), 0.0, h)
        };
        let preedit_len = self.preedit.as_ref().map_or(0, String::len);
        let caret = at(self.cursor + preedit_len);
        let preedit = (preedit_len > 0)
            .then(|| (at(self.cursor), caret))
            .filter(|(a, b)| (a.min.y - b.min.y).abs() < 0.5)
            .map(|(a, b)| Rect::from_corners(Point::new(a.min.x, a.max.y), b.max));
        Some(CaretPaint { caret, preedit })
    }

    /// The text as shown (with the pre-edit text at the caret).
    fn shown(&self) -> String {
        match &self.preedit {
            Some(p) => {
                let mut s = self.text.clone();
                s.insert_str(self.cursor, p);
                s
            }
            None => self.text.clone(),
        }
    }

    fn sync(&self, doc: &mut Document) {
        let shown = self.shown();
        if let Some(a) = doc.annotation_mut(self.id)
            && let AnnotationShape::Text(t) = &mut a.shape
        {
            t.set_text(&shown);
        }
    }

    /// The shown text, where it starts, its style and its fixed width.
    fn layout(&self, doc: &Document) -> Option<(String, Point, Style, Option<f32>)> {
        let a = doc.annotation(self.id)?;
        let AnnotationShape::Text(t) = &a.shape else {
            return None;
        };
        Some((
            t.text().to_owned(),
            t.text_origin(),
            a.style.clone(),
            t.width(),
        ))
    }

    fn insert(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    fn ime(&mut self, ime: &ImeEvent) {
        match ime {
            ImeEvent::Preedit { text, .. } => {
                self.preedit = (!text.is_empty()).then(|| text.clone());
            }
            ImeEvent::Commit(text) => {
                self.preedit = None;
                if text != "\n" && text != "\r" {
                    self.insert(text);
                }
            }
            ImeEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            } => {
                let start = self.text[..self.cursor]
                    .char_indices()
                    .rev()
                    .nth(before_chars.saturating_sub(1))
                    .map_or(0, |(i, _)| i);
                let start = if *before_chars == 0 {
                    self.cursor
                } else {
                    start
                };
                let end = self.text[self.cursor..]
                    .char_indices()
                    .nth(*after_chars)
                    .map_or(self.text.len(), |(i, _)| self.cursor + i);
                self.text.replace_range(start..end, "");
                self.cursor = start;
            }
            #[expect(deprecated)]
            ImeEvent::Enabled | ImeEvent::Disabled => {}
        }
    }

    fn key(&mut self, doc: &Document, key: Key, modifiers: egui::Modifiers) {
        if modifiers.command || modifiers.ctrl || modifiers.alt {
            return;
        }
        match key {
            Key::Enter => self.insert("\n"),
            Key::Backspace => {
                if let Some(prev) = self.prev_boundary() {
                    self.text.replace_range(prev..self.cursor, "");
                    self.cursor = prev;
                }
            }
            Key::Delete => {
                if let Some(next) = self.next_boundary() {
                    self.text.replace_range(self.cursor..next, "");
                }
            }
            Key::ArrowLeft => self.cursor = self.prev_boundary().unwrap_or(0),
            Key::ArrowRight => self.cursor = self.next_boundary().unwrap_or(self.text.len()),
            Key::Home => {
                self.cursor = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1);
            }
            Key::End => {
                self.cursor = self.text[self.cursor..]
                    .find('\n')
                    .map_or(self.text.len(), |i| self.cursor + i);
            }
            Key::ArrowUp | Key::ArrowDown => self.vertical(doc, key == Key::ArrowDown),
            _ => {}
        }
    }

    /// Move the caret one line up or down, keeping its x.
    fn vertical(&mut self, doc: &Document, down: bool) {
        let Some((text, _, style, width)) = self.layout(doc) else {
            return;
        };
        let spec = spec(&text, &style, width);
        let (x, top, h) = lecoq_render::text::caret(&spec, self.cursor);
        let y = if down { top + h * 1.5 } else { top - h * 0.5 };
        if y < 0.0 {
            self.cursor = 0;
            return;
        }
        self.cursor = lecoq_render::text::index_at(&spec, x, y).min(self.text.len());
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.text[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
    }
}

fn spec<'a>(text: &'a str, style: &'a Style, width: Option<f32>) -> TextSpec<'a> {
    TextSpec {
        text,
        font: &style.font,
        size: style.font_size,
        wrap: width,
    }
}
