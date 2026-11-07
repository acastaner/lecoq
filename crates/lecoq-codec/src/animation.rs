//! The frames of an animated GIF or WebP (§3.4), composited by the decoder and reduced for display.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::metadata::Orientation;
use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader};

use lecoq_core::{PixelData, Pixels};

use crate::colour::Converter;
use crate::{DecodeError, Decoded, DisplayImage, Size};

/// Delays at or under this are played as [`SHORT_DELAY_SUBSTITUTE`], as browsers do: many GIFs
/// say 0 or 10 ms and were made to be shown at 100 ms.
const SHORT_DELAY_LIMIT: Duration = Duration::from_millis(10);
const SHORT_DELAY_SUBSTITUTE: Duration = Duration::from_millis(100);

/// One frame of an animation: the whole picture as it is on screen at that point, for `delay`.
pub struct Frame {
    pub image: DisplayImage,
    pub delay: Duration,
}

/// The frames of an animation, decoded one at a time, in order.
pub struct Frames {
    inner: image::Frames<'static>,
    orientation: Orientation,
    /// The colour conversion for this animation, prepared once from its profile and applied to
    /// every frame (#60). The iterator owns no raw profile bytes.
    converter: Converter,
    max: Size,
}

impl Iterator for Frames {
    type Item = Result<Frame, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let frame = match self.inner.next()? {
            Ok(frame) => frame,
            Err(error) => return Some(Err(DecodeError::new(error))),
        };
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay = Duration::from_secs_f64(f64::from(numer) / f64::from(denom.max(1)) / 1000.0);
        let delay = if delay <= SHORT_DELAY_LIMIT {
            SHORT_DELAY_SUBSTITUTE
        } else {
            delay
        };
        let buffer = frame.into_buffer();
        let (width, height) = (buffer.width(), buffer.height());
        let decoded = Decoded {
            stored: Size::new(width, height),
            pixels: Pixels {
                width,
                height,
                data: PixelData::Rgba(buffer.into_raw()),
            },
            orientation: self.orientation,
            // The converter carries the colour conversion; a frame needs no profile bytes of its
            // own, so none is cloned here (#60).
            icc: None,
            may_animate: true,
        };
        Some(Ok(Frame {
            image: decoded.into_display_with(self.max, &self.converter),
            delay,
        }))
    }
}

/// The frames of `path` reduced to fit inside `max`, or `None` when it isn't an animated GIF or
/// WebP. A GIF of a single frame still yields `Some`: count the frames.
pub fn frames(path: &Path, max: Size) -> Result<Option<Frames>, DecodeError> {
    let reader = ImageReader::open(path)
        .map_err(DecodeError::new)?
        .with_guessed_format()
        .map_err(DecodeError::new)?;
    let format = reader.format();
    let input = || -> Result<BufReader<File>, DecodeError> {
        Ok(BufReader::new(File::open(path).map_err(DecodeError::new)?))
    };
    let (inner, orientation, icc) = match format {
        Some(ImageFormat::Gif) => {
            let mut decoder = GifDecoder::new(input()?).map_err(DecodeError::new)?;
            let (orientation, icc) = metadata(&mut decoder);
            (decoder.into_frames(), orientation, icc)
        }
        Some(ImageFormat::WebP) => {
            let mut decoder = WebPDecoder::new(input()?).map_err(DecodeError::new)?;
            if !decoder.has_animation() {
                return Ok(None);
            }
            let (orientation, icc) = metadata(&mut decoder);
            (decoder.into_frames(), orientation, icc)
        }
        _ => return Ok(None),
    };
    Ok(Some(Frames {
        inner,
        orientation,
        // Parse the profile and build the transform once, for the whole animation (#60).
        converter: Converter::prepare(icc.as_deref()),
        max,
    }))
}

fn metadata(decoder: &mut impl ImageDecoder) -> (Orientation, Option<Vec<u8>>) {
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    (orientation, decoder.icc_profile().ok().flatten())
}
