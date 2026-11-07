//! Colour conversion (§5.2): an embedded ICC profile that isn't sRGB is converted to sRGB with
//! lcms2. sRGB and untagged images skip it. The display is assumed to be sRGB.
//!
//! The conversion is prepared once, as a [`Converter`], from the profile bytes: the profile is
//! parsed, compared against sRGB, and (when it differs) turned into lcms transforms there and
//! then. Applying the converter to a frame's pixels is then only the transform. An animation
//! prepares one converter and applies it to every frame, so a profiled animation parses its
//! profile once rather than once per frame (#60).

use lcms2::{CIEXYZ, Flags, Intent, PixelFormat, Profile, Tag, TagSignature, Transform};

use lecoq_core::{PixelData, Pixels};

thread_local! {
    /// lcms2's reference sRGB profile, built on first use and reused: the "is this sRGB?"
    /// comparison and every sRGB output profile need it, and building it per call was the
    /// per-frame cost #60 removes.
    static SRGB: Profile = Profile::new_srgb();
}

/// A prepared colour conversion to sRGB, built once from an ICC profile and applied to many
/// buffers (every frame of an animation). It owns no profile bytes: the profile is parsed and the
/// lcms transforms built when the converter is made, not when it is applied.
pub(crate) struct Converter {
    /// `None` when the pixels pass through unchanged: the profile is absent, unreadable, not an
    /// RGB profile, or already sRGB. `Some` holds the transforms, one per pixel layout, that take
    /// the profile's colours to sRGB.
    transforms: Option<Transforms>,
}

/// The sRGB transform for each pixel layout a decode produces, built once from one profile. Each
/// is `None` when lcms couldn't build it; those pixels then pass through, as the per-call path
/// did.
struct Transforms {
    rgb: Option<Transform<u8, u8>>,
    rgba: Option<Transform<u8, u8>>,
}

impl Converter {
    /// The converter for pixels tagged `icc` (`None` when untagged). Parses the profile and
    /// decides, once, whether a conversion is needed, building the transforms when it is.
    pub(crate) fn prepare(icc: Option<&[u8]>) -> Self {
        Self {
            transforms: Self::build(icc),
        }
    }

    fn build(icc: Option<&[u8]>) -> Option<Transforms> {
        let icc = icc?;
        let profile = Profile::new_icc(icc).ok()?;
        // A grey or CMYK profile describes pixels the decoder has already turned into RGB.
        if profile.color_space() != lcms2::ColorSpaceSignature::RgbData {
            return None;
        }
        if SRGB.with(|srgb| is_srgb(&profile, srgb)) {
            return None;
        }
        let transform = |format| {
            SRGB.with(|srgb| {
                Transform::<u8, u8>::new_flags(
                    &profile,
                    format,
                    srgb,
                    format,
                    Intent::Perceptual,
                    Flags::COPY_ALPHA,
                )
                .ok()
            })
        };
        Some(Transforms {
            rgb: transform(PixelFormat::RGB_8),
            rgba: transform(PixelFormat::RGBA_8),
        })
    }

    /// Convert `pixels` from the profile to sRGB, in place. A no-op for a passthrough converter, or
    /// for a layout lcms couldn't build a transform for.
    pub(crate) fn convert(&self, pixels: &mut Pixels) {
        let Some(transforms) = &self.transforms else {
            return;
        };
        match &mut pixels.data {
            PixelData::Rgb(buffer) => {
                if let Some(transform) = &transforms.rgb {
                    transform.transform_in_place(buffer);
                }
            }
            // Decode pixels are straight; premultiplied pixels never reach colour conversion.
            PixelData::Rgba(buffer) | PixelData::RgbaPremultiplied(buffer) => {
                if let Some(transform) = &transforms.rgba {
                    transform.transform_in_place(buffer);
                }
            }
        }
    }
}

/// Is `profile` sRGB? Compared by what it does, not by name: the same colorants and tone curves as
/// lcms2's own sRGB, within rounding. Profiles that aren't matrix/TRC (LUT-based) are never sRGB.
fn is_srgb(profile: &Profile, srgb: &Profile) -> bool {
    const COLORANTS: [TagSignature; 3] = [
        TagSignature::RedColorantTag,
        TagSignature::GreenColorantTag,
        TagSignature::BlueColorantTag,
    ];
    const CURVES: [TagSignature; 3] = [
        TagSignature::RedTRCTag,
        TagSignature::GreenTRCTag,
        TagSignature::BlueTRCTag,
    ];
    let colorants_match =
        COLORANTS.iter().all(
            |&sig| match (xyz(profile.read_tag(sig)), xyz(srgb.read_tag(sig))) {
                (Some(a), Some(b)) => {
                    (a.X - b.X).abs() < 0.002
                        && (a.Y - b.Y).abs() < 0.002
                        && (a.Z - b.Z).abs() < 0.002
                }
                _ => false,
            },
        );
    colorants_match
        && CURVES
            .iter()
            .all(|&sig| match (profile.read_tag(sig), srgb.read_tag(sig)) {
                (Tag::ToneCurve(a), Tag::ToneCurve(b)) => [0.02f32, 0.1, 0.25, 0.5, 0.75, 0.9]
                    .iter()
                    .all(|&v| (a.eval(v) - b.eval(v)).abs() < 0.004),
                _ => false,
            })
}

/// The ICC profile a save writes for pixels decoded from a file tagged `icc` (§5.3): the original
/// when the decode left the pixels in it (sRGB already), sRGB when they were converted, or when
/// the profile isn't an RGB one (the decoder turned those pixels into RGB, shown as sRGB). An
/// unreadable profile is dropped: the pixels were shown as sRGB.
pub(crate) fn output_profile(icc: Option<&[u8]>) -> Option<Vec<u8>> {
    let icc = icc?;
    let profile = Profile::new_icc(icc).ok()?;
    if profile.color_space() == lcms2::ColorSpaceSignature::RgbData
        && SRGB.with(|srgb| is_srgb(&profile, srgb))
    {
        return Some(icc.to_vec());
    }
    SRGB.with(|srgb| srgb.icc().ok())
}

fn xyz(tag: Tag<'_>) -> Option<CIEXYZ> {
    match tag {
        Tag::CIEXYZ(xyz) => Some(*xyz),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use lcms2::{CIExyY, CIExyYTRIPLE, ToneCurve};

    use super::*;

    /// An RGB profile that is plainly not sRGB: sRGB's primaries with red and green swapped, and a
    /// pure 2.2 gamma. Serialized to ICC bytes, as a file would carry it.
    fn non_srgb_icc() -> Vec<u8> {
        let white = CIExyY {
            x: 0.3127,
            y: 0.3290,
            Y: 1.0,
        };
        let primaries = CIExyYTRIPLE {
            // sRGB's green where red belongs, and sRGB's red where green belongs.
            Red: CIExyY {
                x: 0.30,
                y: 0.60,
                Y: 1.0,
            },
            Green: CIExyY {
                x: 0.64,
                y: 0.33,
                Y: 1.0,
            },
            Blue: CIExyY {
                x: 0.15,
                y: 0.06,
                Y: 1.0,
            },
        };
        let gamma = ToneCurve::new(2.2);
        Profile::new_rgb(&white, &primaries, &[&gamma, &gamma, &gamma])
            .unwrap()
            .icc()
            .unwrap()
    }

    /// The old per-frame conversion, replicated verbatim as the reference the prepared converter
    /// must match byte for byte: parse the profile, reject non-RGB and sRGB, build the transform,
    /// convert. This is what `to_srgb` did for every frame before #60.
    fn convert_per_frame(icc: &[u8], pixels: &mut Pixels) {
        let Ok(profile) = Profile::new_icc(icc) else {
            return;
        };
        if profile.color_space() != lcms2::ColorSpaceSignature::RgbData {
            return;
        }
        let srgb = Profile::new_srgb();
        if is_srgb(&profile, &srgb) {
            return;
        }
        let (format, buffer) = match &mut pixels.data {
            PixelData::Rgb(rgb) => (PixelFormat::RGB_8, rgb),
            PixelData::Rgba(rgba) | PixelData::RgbaPremultiplied(rgba) => {
                (PixelFormat::RGBA_8, rgba)
            }
        };
        let Ok(transform) = Transform::<u8, u8>::new_flags(
            &profile,
            format,
            &srgb,
            format,
            Intent::Perceptual,
            Flags::COPY_ALPHA,
        ) else {
            return;
        };
        transform.transform_in_place(buffer);
    }

    fn rgba(data: Vec<u8>) -> Pixels {
        Pixels {
            width: (data.len() / 4) as u32,
            height: 1,
            data: PixelData::Rgba(data),
        }
    }

    #[test]
    fn a_non_srgb_profile_builds_a_converting_converter() {
        let converter = Converter::prepare(Some(&non_srgb_icc()));
        assert!(
            converter.transforms.is_some(),
            "a non-sRGB RGB profile needs a conversion"
        );
    }

    #[test]
    fn an_srgb_profile_is_passthrough() {
        let srgb = Profile::new_srgb().icc().unwrap();
        let converter = Converter::prepare(Some(&srgb));
        assert!(
            converter.transforms.is_none(),
            "sRGB needs no conversion, so the converter is None (passthrough)"
        );
    }

    #[test]
    fn an_absent_profile_is_passthrough() {
        let converter = Converter::prepare(None);
        assert!(
            converter.transforms.is_none(),
            "untagged pixels pass through"
        );
    }

    #[test]
    fn a_passthrough_converter_leaves_pixels_untouched() {
        let original = vec![10, 20, 30, 255, 200, 150, 100, 128];
        let mut pixels = rgba(original.clone());
        Converter::prepare(None).convert(&mut pixels);
        let PixelData::Rgba(out) = pixels.data else {
            panic!("layout kept");
        };
        assert_eq!(out, original);
    }

    #[test]
    fn one_prepared_converter_matches_the_per_frame_path_for_every_frame() {
        let icc = non_srgb_icc();
        // Several distinct "frames", as an animation would feed through one converter.
        let frames = [
            vec![255, 0, 0, 255, 0, 255, 0, 255],
            vec![0, 0, 255, 255, 128, 64, 32, 200],
            vec![10, 120, 200, 255, 90, 90, 90, 255],
        ];
        let converter = Converter::prepare(Some(&icc));
        for frame in &frames {
            let mut once = rgba(frame.clone());
            converter.convert(&mut once);

            let mut reference = rgba(frame.clone());
            convert_per_frame(&icc, &mut reference);

            let (PixelData::Rgba(once), PixelData::Rgba(reference)) = (once.data, reference.data)
            else {
                panic!("layout kept");
            };
            assert_eq!(
                once, reference,
                "prepared-once conversion must equal the per-frame conversion, byte for byte"
            );
            assert_ne!(
                once, *frame,
                "the non-sRGB profile must actually have changed the pixels"
            );
        }
    }
}
