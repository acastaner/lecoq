//! Resampling for the resize Image operation (§4.7): Skia's cubic (Mitchell) when upscaling,
//! mipmap plus cubic when downscaling.
//!
//! Downscaling first halves each axis with a 2×2 box filter (one mipmap level per halving) while
//! that stays at or above the target size, then finishes with the Mitchell cubic. A cubic alone
//! only looks at 4×4 source pixels, so it would alias on large reductions; the mipmap levels
//! average everything first. (Skia can't combine cubic sampling with its own mipmaps, so the
//! levels are made here.)

use lecoq_core::{PixelData, Pixels};
use skia_safe as sk;

/// `pixels` resampled to `width`×`height`. Keeps the pixel format: RGB stays RGB, RGBA stays
/// RGBA with straight alpha.
pub fn resample(pixels: &Pixels, width: u32, height: u32) -> Pixels {
    assert!(width > 0 && height > 0, "resample to an empty size");
    let alpha = matches!(pixels.data, PixelData::Rgba(_));
    let mut image = crate::to_skia(pixels);
    // Mipmap levels: halve each axis while the result stays at or above the target.
    loop {
        let w = if image.width() as u32 / 2 >= width {
            image.width() / 2
        } else {
            image.width()
        };
        let h = if image.height() as u32 / 2 >= height {
            image.height() / 2
        } else {
            image.height()
        };
        if (w, h) == (image.width(), image.height()) {
            break;
        }
        // Bilinear at an exact half samples between four pixels: a 2×2 box filter.
        let linear = sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None);
        let (info, data) = scale(&image, w, h, sk::AlphaType::Premul, linear);
        image = sk::images::raster_from_data(&info, sk::Data::new_copy(&data), w as usize * 4)
            .expect("a mipmap level");
    }
    let alpha_type = if alpha {
        sk::AlphaType::Unpremul
    } else {
        sk::AlphaType::Opaque
    };
    let mitchell = sk::SamplingOptions::from(sk::CubicResampler::mitchell());
    let (_, rgba) = scale(&image, width as i32, height as i32, alpha_type, mitchell);
    let data = if alpha {
        PixelData::Rgba(rgba)
    } else {
        PixelData::Rgb(
            rgba.chunks_exact(4)
                .flat_map(|px| [px[0], px[1], px[2]])
                .collect(),
        )
    };
    Pixels {
        width,
        height,
        data,
    }
}

/// `image` scaled onto a fresh `width`×`height` RGBA buffer.
fn scale(
    image: &sk::Image,
    width: i32,
    height: i32,
    alpha_type: sk::AlphaType,
    sampling: sk::SamplingOptions,
) -> (sk::ImageInfo, Vec<u8>) {
    let info = sk::ImageInfo::new((width, height), sk::ColorType::RGBA8888, alpha_type, None);
    let mut data = vec![0u8; width as usize * height as usize * 4];
    let ok = {
        let pixmap =
            sk::Pixmap::new(&info, &mut data, width as usize * 4).expect("a pixmap to scale into");
        image.scale_pixels(&pixmap, sampling, sk::image::CachingHint::Disallow)
    };
    assert!(ok, "Skia couldn't scale the Image");
    (info, data)
}
