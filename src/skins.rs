use image::GenericImageView;

/// Renders a square head avatar from a skin file. Minecraft classic skins are
/// 64x64 pixels (or 128x128 for HD skins); the front of the head is an 8x8
/// region located at (8, 8). We crop that region and scale it up.
pub fn custom_avatar(path: &str) -> slint::Image {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| decode_head(&bytes))
        .unwrap_or_else(fallback_avatar)
}

fn decode_head(bytes: &[u8]) -> Option<slint::Image> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();

    let scale = width / 64;
    if scale == 0 || width % 64 != 0 {
        return None;
    }

    let face = 8 * scale;
    if width < face * 2 || height < face * 2 {
        return None;
    }

    let cropped = image.view(face, face, face, face).to_image();
    let resized = image::imageops::resize(&cropped, 64, 64, image::imageops::FilterType::Nearest);
    Some(crate::image_util::from_rgba(&resized))
}

fn fallback_avatar() -> slint::Image {
    let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64);
    pixels.make_mut_slice().fill(slint::Rgba8Pixel {
        r: 0x5b,
        g: 0x5b,
        b: 0x6e,
        a: 0xff,
    });
    slint::Image::from_rgba8(pixels)
}
