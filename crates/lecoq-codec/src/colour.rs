//! Colour conversion (§5.2): an embedded ICC profile that isn't sRGB is converted to sRGB with
//! lcms2. sRGB and untagged images skip it. The display is assumed to be sRGB.

use lcms2::{CIEXYZ, Flags, Intent, PixelFormat, Profile, Tag, TagSignature, Transform};

use crate::Pixels;

/// Convert `pixels` from the `icc` profile to sRGB, in place. Nothing happens when the profile is
/// sRGB, isn't an RGB profile (a grey or CMYK profile describes pixels the decoder has already
/// turned into RGB), or can't be read.
pub(crate) fn to_srgb(icc: &[u8], pixels: &mut Pixels) {
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
    let (format, buffer) = match pixels {
        Pixels::Rgb(rgb) => (PixelFormat::RGB_8, rgb),
        Pixels::Rgba(rgba) => (PixelFormat::RGBA_8, rgba),
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
    let srgb = Profile::new_srgb();
    if profile.color_space() == lcms2::ColorSpaceSignature::RgbData && is_srgb(&profile, &srgb) {
        return Some(icc.to_vec());
    }
    srgb.icc().ok()
}

fn xyz(tag: Tag<'_>) -> Option<CIEXYZ> {
    match tag {
        Tag::CIEXYZ(xyz) => Some(*xyz),
        _ => None,
    }
}
