//! The clipboard JSON (§6.2): Annotations copied from one Image, to be pasted onto another, in
//! this window or another lecoq's.
//!
//! ```json
//! {"lecoq_annotations": 1,
//!  "annotations": [{"shape": {"type": "rectangle", "rect": {..}}, "style": {..}}, ..]}
//! ```
//!
//! Positions are Image pixels, relative to the copied Image's top-left corner (see
//! [`crate::geometry`]); styles are [`Style`] as `config.toml` writes it, plus the magnify
//! highlight's `magnification` (which the config keeps as `magnify_factor`). Annotations are listed
//! bottom to top. Ids and Step counter numbers are not copied: the Document a paste goes into
//! gives the Annotations new ids, so pasted Step counters take its next numbers. A Text's layout
//! is measured again there. An Inset's pixels travel as base64 (see [`pixels_serde`]).

use crate::annotation::{Annotation, AnnotationShape};
use crate::style::Style;

/// The MIME type of lecoq's Annotations on the clipboard.
pub const MIME: &str = "application/x-lecoq-annotations+json";

/// The format version written in the JSON. A newer version is not pasted.
const VERSION: u32 = 1;

/// One copied Annotation: its geometry and its style.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CopiedAnnotation {
    pub shape: AnnotationShape,
    #[serde(with = "style_serde")]
    pub style: Style,
}

/// A style in the JSON: [`Style`] as `config.toml` writes it, plus `magnification`. The config
/// leaves that out (it is the `magnify_factor` setting), but a pasted magnify highlight keeps the
/// factor it was copied with.
mod style_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::style::Style;

    #[derive(Serialize)]
    struct Out<'a> {
        #[serde(flatten)]
        style: &'a Style,
        magnification: f32,
    }

    #[derive(Deserialize)]
    struct In {
        #[serde(flatten)]
        style: Style,
        magnification: Option<f32>,
    }

    pub fn serialize<S: Serializer>(style: &Style, s: S) -> Result<S::Ok, S::Error> {
        Out {
            style,
            magnification: style.magnification,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Style, D::Error> {
        let In {
            mut style,
            magnification,
        } = In::deserialize(d)?;
        if let Some(factor) = magnification {
            style.magnification = factor;
        }
        Ok(style)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Payload {
    lecoq_annotations: u32,
    annotations: Vec<CopiedAnnotation>,
}

/// The JSON of `annotations` (bottom to top).
pub fn to_json<'a>(annotations: impl IntoIterator<Item = &'a Annotation>) -> Vec<u8> {
    let payload = Payload {
        lecoq_annotations: VERSION,
        annotations: annotations
            .into_iter()
            .map(|a| CopiedAnnotation {
                shape: a.shape.clone(),
                style: a.style.clone(),
            })
            .collect(),
    };
    serde_json::to_vec(&payload).expect("Annotations serialise")
}

/// The Annotations in clipboard JSON, bottom to top. The error says why it can't be pasted.
pub fn from_json(data: &[u8]) -> Result<Vec<CopiedAnnotation>, String> {
    let payload: Payload = serde_json::from_slice(data).map_err(|e| e.to_string())?;
    if payload.lecoq_annotations > VERSION {
        return Err("they come from a newer lecoq".into());
    }
    Ok(payload
        .annotations
        .into_iter()
        .map(|mut a| {
            a.style = a.style.sanitised();
            a
        })
        .collect())
}

/// An Inset's pixels in the JSON: `{"width", "height", "channels": 3|4, "data": base64}`.
pub(crate) mod pixels_serde {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    use crate::document::{PixelData, Pixels};

    #[derive(Serialize, Deserialize)]
    struct Encoded {
        width: u32,
        height: u32,
        channels: u8,
        data: String,
    }

    pub fn serialize<S: Serializer>(pixels: &Arc<Pixels>, s: S) -> Result<S::Ok, S::Error> {
        // An Inset's pixels are full-resolution and straight; premultiplied pixels never reach the
        // clipboard, but straighten defensively so the wire format stays 3 or 4 straight channels.
        let straightened;
        let data = match &pixels.data {
            PixelData::Rgb(_) | PixelData::Rgba(_) => &pixels.data,
            PixelData::RgbaPremultiplied(_) => {
                straightened = pixels.straight();
                &straightened.data
            }
        };
        let (channels, data) = match data {
            PixelData::Rgb(rgb) => (3, rgb),
            PixelData::Rgba(rgba) => (4, rgba),
            PixelData::RgbaPremultiplied(_) => unreachable!("straight() is never premultiplied"),
        };
        Encoded {
            width: pixels.width,
            height: pixels.height,
            channels,
            data: super::base64::encode(data),
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Pixels>, D::Error> {
        let e = Encoded::deserialize(d)?;
        let data = super::base64::decode(&e.data).ok_or_else(|| D::Error::custom("bad base64"))?;
        let expected = e.width as usize * e.height as usize * usize::from(e.channels);
        if data.len() != expected || e.width == 0 || e.height == 0 {
            return Err(D::Error::custom("pixel data doesn't match the size"));
        }
        let data = match e.channels {
            3 => PixelData::Rgb(data),
            4 => PixelData::Rgba(data),
            _ => return Err(D::Error::custom("channels must be 3 or 4")),
        };
        Ok(Arc::new(Pixels {
            width: e.width,
            height: e.height,
            data,
        }))
    }
}

/// Standard base64 (RFC 4648, padded).
mod base64 {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let text = text.trim_end_matches('=').as_bytes();
        let mut out = Vec::with_capacity(text.len() * 3 / 4);
        let mut n = 0u32;
        let mut bits = 0;
        for &c in text {
            let v = ALPHABET.iter().position(|&a| a == c)? as u32;
            n = (n << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((n >> bits) as u8);
                n &= (1 << bits) - 1;
            }
        }
        Some(out)
    }
}
