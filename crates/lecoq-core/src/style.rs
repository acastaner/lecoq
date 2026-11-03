//! Style properties (§4.3): what the property bar edits.
//!
//! One [`Style`] struct holds every property any Annotation type can have; each type lists the
//! ones it uses ([`crate::Shape::properties`]), and the property bar shows exactly those.
//!
//! Adding a property (arrowheads, font, size, mode, strength, …):
//! 1. a field on [`Style`] with its default in `Style::default`,
//! 2. a variant in [`PropertyKind`] and in [`Property`], and an arm in `Style::get`/`Style::set`,
//! 3. a control in the `lecoq` crate's `ui/property_bar.rs`,
//! 4. the kind in the `properties()` list of the Annotation types (and tools) that use it.

/// A colour, 8-bit sRGB with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const RED: Rgba = Rgba::rgb(255, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const YELLOW: Rgba = Rgba::rgb(255, 255, 0);
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque: how colours are written in `config.toml` (§8.3).
impl std::fmt::Display for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Rgba {
    type Err = String;

    /// `#rrggbb` or `#rrggbbaa`.
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("not a colour (#rrggbb or #rrggbbaa): {s:?}");
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        let a = if hex.len() == 8 { byte(6)? } else { 255 };
        Ok(Self::new(byte(0)?, byte(2)?, byte(4)?, a))
    }
}

impl serde::Serialize for Rgba {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Rgba {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A fill or border colour in `config.toml`: a colour, or `"none"`.
mod fill_serde {
    use super::Rgba;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(fill: &Option<Rgba>, s: S) -> Result<S::Ok, S::Error> {
        match fill {
            Some(c) => s.collect_str(c),
            None => s.serialize_str("none"),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Rgba>, D::Error> {
        let s = String::deserialize(d)?;
        if s == "none" {
            return Ok(None);
        }
        s.parse().map(Some).map_err(serde::de::Error::custom)
    }
}

/// Every style property an Annotation can have. Types ignore the ones they don't list.
///
/// Each tool's style is saved as `tools.<tool>` in `config.toml` (§8.3) through serde: a new field
/// is persisted automatically (its type needs `Serialize`/`Deserialize`), and a missing key takes
/// its value from [`Style::default`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Style {
    pub line: Rgba,
    /// `None` is "no fill" (∅).
    #[serde(with = "fill_serde")]
    pub fill: Option<Rgba>,
    /// Line thickness in Image pixels.
    pub thickness: f32,
    pub shadow: bool,
    /// Step counter diameter in Image pixels.
    pub counter_size: f32,
    /// The text font's family name (§4.4).
    pub font: String,
    /// The text size in Image pixels.
    pub font_size: f32,
    /// Which ends of an arrow carry a head.
    pub arrowheads: Arrowheads,
    /// An Inset's border (§4.6): `None` is no border; its width is `thickness`.
    #[serde(with = "fill_serde")]
    pub border: Option<Rgba>,
    pub highlight_mode: HighlightMode,
    /// The text highlight's colour (multiplied over the region).
    pub highlight: Rgba,
    /// The magnify highlight's factor. Not saved with the tool's style: it is the `magnify_factor`
    /// setting (§8.3), which the app seeds the highlight tool's default from.
    #[serde(skip)]
    pub magnification: f32,
    pub obfuscate_mode: ObfuscateMode,
    /// The obfuscation's strength in Image pixels: the pixelate block size, the blur radius.
    pub strength: f32,
}

/// The obfuscation's modes (§4.2).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ObfuscateMode {
    /// Blocks whose size is the strength.
    #[default]
    Pixelate,
    /// A Gaussian blur whose radius is the strength.
    Blur,
}

impl ObfuscateMode {
    /// In the order the property bar lists them.
    pub const ALL: [ObfuscateMode; 2] = [ObfuscateMode::Pixelate, ObfuscateMode::Blur];
}

/// The highlight's modes (§4.2).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum HighlightMode {
    /// A translucent colour multiplied over the region.
    #[default]
    Text,
    /// Everything outside the region is dimmed.
    Area,
    /// Everything outside the region is shown in grayscale.
    Grayscale,
    /// The region shows what's under it, enlarged.
    Magnify,
}

impl HighlightMode {
    /// In the order the property bar lists them.
    pub const ALL: [HighlightMode; 4] = [
        HighlightMode::Text,
        HighlightMode::Area,
        HighlightMode::Grayscale,
        HighlightMode::Magnify,
    ];
}

/// Which ends of an arrow carry a head (§4.1 A).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Arrowheads {
    #[default]
    End,
    Start,
    Both,
    None,
}

impl Arrowheads {
    /// In the order the property bar lists them.
    pub const ALL: [Arrowheads; 4] = [
        Arrowheads::End,
        Arrowheads::Start,
        Arrowheads::Both,
        Arrowheads::None,
    ];

    pub fn at_start(self) -> bool {
        matches!(self, Arrowheads::Start | Arrowheads::Both)
    }

    pub fn at_end(self) -> bool {
        matches!(self, Arrowheads::End | Arrowheads::Both)
    }
}

impl Default for Style {
    /// Greenshot-like defaults (§8.3): red, 3 px, shadow on, no fill.
    fn default() -> Self {
        Self {
            line: Rgba::RED,
            fill: None,
            thickness: 3.0,
            shadow: true,
            counter_size: 30.0,
            font: DEFAULT_FONT.to_owned(),
            font_size: 20.0,
            arrowheads: Arrowheads::End,
            border: None,
            highlight_mode: HighlightMode::Text,
            highlight: Rgba::YELLOW,
            magnification: 2.0,
            obfuscate_mode: ObfuscateMode::Pixelate,
            strength: 10.0,
        }
    }
}

/// Names a style property, without a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyKind {
    LineColour,
    FillColour,
    Thickness,
    Shadow,
    CounterSize,
    /// The text colour: the same field as the line colour (`Style::line`).
    TextColour,
    Font,
    FontSize,
    Arrowheads,
    BorderColour,
    HighlightMode,
    HighlightColour,
    Magnification,
    ObfuscateMode,
    Strength,
}

/// A style property with its value: one change in the property bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    LineColour(Rgba),
    FillColour(Option<Rgba>),
    Thickness(f32),
    Shadow(bool),
    CounterSize(f32),
    TextColour(Rgba),
    Font(String),
    FontSize(f32),
    Arrowheads(Arrowheads),
    /// `None` is no border.
    BorderColour(Option<Rgba>),
    HighlightMode(HighlightMode),
    HighlightColour(Rgba),
    Magnification(f32),
    ObfuscateMode(ObfuscateMode),
    Strength(f32),
}

impl Property {
    pub fn kind(&self) -> PropertyKind {
        match self {
            Property::LineColour(_) => PropertyKind::LineColour,
            Property::FillColour(_) => PropertyKind::FillColour,
            Property::Thickness(_) => PropertyKind::Thickness,
            Property::Shadow(_) => PropertyKind::Shadow,
            Property::CounterSize(_) => PropertyKind::CounterSize,
            Property::TextColour(_) => PropertyKind::TextColour,
            Property::Font(_) => PropertyKind::Font,
            Property::FontSize(_) => PropertyKind::FontSize,
            Property::Arrowheads(_) => PropertyKind::Arrowheads,
            Property::BorderColour(_) => PropertyKind::BorderColour,
            Property::HighlightMode(_) => PropertyKind::HighlightMode,
            Property::HighlightColour(_) => PropertyKind::HighlightColour,
            Property::Magnification(_) => PropertyKind::Magnification,
            Property::ObfuscateMode(_) => PropertyKind::ObfuscateMode,
            Property::Strength(_) => PropertyKind::Strength,
        }
    }
}

/// Thickness limits of the property bar.
pub const THICKNESS_RANGE: std::ops::RangeInclusive<f32> = 1.0..=40.0;

/// Step counter size limits of the property bar.
pub const COUNTER_SIZE_RANGE: std::ops::RangeInclusive<f32> = 10.0..=200.0;
/// Font size limits of the property bar, in Image pixels.
pub const FONT_SIZE_RANGE: std::ops::RangeInclusive<f32> = 6.0..=400.0;
/// Magnification limits of the property bar.
pub const MAGNIFICATION_RANGE: std::ops::RangeInclusive<f32> = 1.0..=10.0;
/// Obfuscation strength limits of the property bar, in Image pixels.
pub const STRENGTH_RANGE: std::ops::RangeInclusive<f32> = 2.0..=100.0;

/// The text tool's default font: the platform's sans-serif (fontconfig resolves the alias; where
/// nothing matches, Skia falls back to the system's default font).
pub const DEFAULT_FONT: &str = "sans-serif";

impl Style {
    pub fn get(&self, kind: PropertyKind) -> Property {
        match kind {
            PropertyKind::LineColour => Property::LineColour(self.line),
            PropertyKind::FillColour => Property::FillColour(self.fill),
            PropertyKind::Thickness => Property::Thickness(self.thickness),
            PropertyKind::Shadow => Property::Shadow(self.shadow),
            PropertyKind::CounterSize => Property::CounterSize(self.counter_size),
            PropertyKind::TextColour => Property::TextColour(self.line),
            PropertyKind::Font => Property::Font(self.font.clone()),
            PropertyKind::FontSize => Property::FontSize(self.font_size),
            PropertyKind::Arrowheads => Property::Arrowheads(self.arrowheads),
            PropertyKind::BorderColour => Property::BorderColour(self.border),
            PropertyKind::HighlightMode => Property::HighlightMode(self.highlight_mode),
            PropertyKind::HighlightColour => Property::HighlightColour(self.highlight),
            PropertyKind::Magnification => Property::Magnification(self.magnification),
            PropertyKind::ObfuscateMode => Property::ObfuscateMode(self.obfuscate_mode),
            PropertyKind::Strength => Property::Strength(self.strength),
        }
    }

    /// Values read from `config.toml` or the clipboard JSON brought back into range.
    pub fn sanitised(mut self) -> Self {
        if !self.thickness.is_finite() {
            self.thickness = Self::default().thickness;
        }
        self.set(Property::Thickness(self.thickness));
        if !self.font_size.is_finite() {
            self.font_size = Self::default().font_size;
        }
        self.set(Property::FontSize(self.font_size));
        if !self.counter_size.is_finite() {
            self.counter_size = Self::default().counter_size;
        }
        self.set(Property::CounterSize(self.counter_size));
        if !self.strength.is_finite() {
            self.strength = Self::default().strength;
        }
        self.set(Property::Strength(self.strength));
        // Only the clipboard JSON carries it (the config has `magnify_factor`).
        if !self.magnification.is_finite() {
            self.magnification = Self::default().magnification;
        }
        self.set(Property::Magnification(self.magnification));
        if self.font.trim().is_empty() {
            self.font = DEFAULT_FONT.to_owned();
        }
        self
    }

    pub fn set(&mut self, property: Property) {
        match property {
            Property::LineColour(c) => self.line = c,
            Property::FillColour(c) => self.fill = c,
            Property::Thickness(t) => {
                self.thickness = t.clamp(*THICKNESS_RANGE.start(), *THICKNESS_RANGE.end())
            }
            Property::Shadow(s) => self.shadow = s,
            Property::CounterSize(s) => {
                self.counter_size = s.clamp(*COUNTER_SIZE_RANGE.start(), *COUNTER_SIZE_RANGE.end())
            }
            Property::TextColour(c) => self.line = c,
            Property::Font(f) => self.font = f,
            Property::FontSize(s) => {
                self.font_size = s.clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
            }
            Property::Arrowheads(a) => self.arrowheads = a,
            Property::BorderColour(c) => self.border = c,
            Property::HighlightMode(m) => self.highlight_mode = m,
            Property::HighlightColour(c) => self.highlight = c,
            Property::Magnification(f) => {
                if f.is_finite() {
                    self.magnification =
                        f.clamp(*MAGNIFICATION_RANGE.start(), *MAGNIFICATION_RANGE.end())
                }
            }
            Property::ObfuscateMode(m) => self.obfuscate_mode = m,
            Property::Strength(s) => {
                if s.is_finite() {
                    self.strength = s.clamp(*STRENGTH_RANGE.start(), *STRENGTH_RANGE.end())
                }
            }
        }
    }
}
