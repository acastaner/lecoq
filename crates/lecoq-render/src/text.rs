//! Text layout with Skia's `textlayout` (§4.4, §7.2): measuring a text box, drawing it, and the
//! caret positions the text tool needs.
//!
//! The system fonts are only touched here, the first time text is laid out or the families are
//! listed (§7.2: fonts are enumerated the first time the text tool is used). Each thread keeps its
//! own font collection (Skia's isn't thread-safe).
//!
//! Indices into the text are UTF-8 byte offsets; Skia's paragraph API counts UTF-16 code units,
//! converted here.

use std::cell::RefCell;

use lecoq_core::annotation::text::TextSpec;
use skia_safe as sk;
use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, RectHeightStyle, RectWidthStyle,
    TextStyle,
};

thread_local! {
    static FONTS: RefCell<Option<FontCollection>> = const { RefCell::new(None) };
}

fn font_collection() -> FontCollection {
    FONTS.with(|f| {
        f.borrow_mut()
            .get_or_insert_with(|| {
                let mut fc = FontCollection::new();
                fc.set_default_font_manager(sk::FontMgr::new(), None);
                fc
            })
            .clone()
    })
}

/// The installed font families, sorted and without duplicates.
pub fn families() -> Vec<String> {
    let mut names: Vec<String> = sk::FontMgr::new()
        .family_names()
        .filter(|n| !n.is_empty() && !n.starts_with('.'))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// `spec` laid out, drawing in `colour`.
pub fn paragraph(spec: &TextSpec<'_>, colour: sk::Color) -> Paragraph {
    let mut style = TextStyle::new();
    style.set_font_families(&[spec.font]);
    style.set_font_size(spec.size.max(1.0));
    style.set_color(colour);
    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_style(&style);
    paragraph_style.set_replace_tab_characters(true);
    let mut builder = ParagraphBuilder::new(&paragraph_style, font_collection());
    builder.add_text(spec.text);
    let mut paragraph = builder.build();
    paragraph.layout(spec.wrap.unwrap_or(f32::INFINITY));
    paragraph
}

/// The laid-out (width, height): [`lecoq_core::annotation::text::MeasureFn`].
pub fn measure(spec: &TextSpec<'_>) -> (f32, f32) {
    let p = paragraph(spec, sk::Color::BLACK);
    let width = match spec.wrap {
        Some(w) => w,
        None => p.max_intrinsic_width().max(0.0),
    };
    (width, p.height())
}

/// The caret before byte `index`, relative to the text's origin: (x, top, height).
pub fn caret(spec: &TextSpec<'_>, index: usize) -> (f32, f32, f32) {
    let p = paragraph(spec, sk::Color::BLACK);
    let lines = p.get_line_metrics();
    let Some(last) = lines.last() else {
        return (0.0, 0.0, p.height());
    };
    let top = |l: &sk::textlayout::LineMetrics| (l.baseline - l.ascent) as f32;
    let height = |l: &sk::textlayout::LineMetrics| (l.ascent + l.descent) as f32;
    let u = utf16_len(&spec.text[..index]);
    // After a final newline the caret sits on the empty last line.
    if index == spec.text.len() && spec.text.ends_with('\n') {
        return (last.left as f32, top(last), height(last));
    }
    let line = lines
        .iter()
        .find(|l| l.start_index <= u && u < l.end_including_newline)
        .unwrap_or(last);
    let x = if u <= line.start_index {
        line.left as f32
    } else {
        p.get_rects_for_range(
            line.start_index..u,
            RectHeightStyle::Tight,
            RectWidthStyle::Tight,
        )
        .iter()
        .map(|b| b.rect.right)
        .fold(line.left as f32, f32::max)
    };
    (x, top(line), height(line))
}

/// The byte index of the caret position nearest `(x, y)`, relative to the text's origin.
pub fn index_at(spec: &TextSpec<'_>, x: f32, y: f32) -> usize {
    let p = paragraph(spec, sk::Color::BLACK);
    let u = p.get_glyph_position_at_coordinate((x, y)).position.max(0) as usize;
    byte_index(spec.text, u)
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The byte offset of UTF-16 offset `u` (clamped to the text).
fn byte_index(s: &str, u: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= u {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}
