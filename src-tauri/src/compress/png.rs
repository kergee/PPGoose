use anyhow::Result;
use image::RgbaImage;
use rgb::RGBA8;

/// Compress PNG bytes.
///
/// Strategy (auto):
///   1. Decode to RGBA pixels with `image` crate.
///   2. Run `imagequant` lossy palette quantisation (max 256 colours).
///   3. Re-encode as indexed PNG with `png` crate.
///   4. Run `oxipng` lossless optimiser on the result.
///   5. Compare all candidates and return the smallest.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    // --- Decode ----------------------------------------------------------
    let img = super::decode_oriented(data)?.into_rgba8();

    // --- Lossy quantisation ----------------------------------------------
    let quantised = quantise(&img, quality).unwrap_or_default();

    // --- Lossless oxipng pass on original --------------------------------
    // Preset 2 is ~4x faster than 4 on large images for a marginal size difference.
    let oxipng_opts = oxipng::Options::from_preset(2);
    let lossless = oxipng::optimize_from_memory(data, &oxipng_opts).unwrap_or_else(|_| data.to_vec());

    // --- Pick the winner -------------------------------------------------
    let mut best: &[u8] = data;

    if !quantised.is_empty() && quantised.len() < best.len() {
        best = &quantised;
    }
    // Run oxipng on quantised result too for an extra pass
    let quantised_opt = if !quantised.is_empty() {
        oxipng::optimize_from_memory(&quantised, &oxipng_opts).unwrap_or_default()
    } else {
        vec![]
    };
    if !quantised_opt.is_empty() && quantised_opt.len() < best.len() {
        best = &quantised_opt;
    }
    if lossless.len() < best.len() {
        best = &lossless;
    }

    Ok(best.to_vec())
}

/// Returns empty vec on failure (caller falls back to original).
fn quantise(img: &RgbaImage, quality: u8) -> Option<Vec<u8>> {
    let (width, height) = img.dimensions();

    // Build pixel slice for imagequant (RGBA8)
    let pixels: Vec<RGBA8> = img.pixels().map(|p| {
        let c = p.0;
        RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] }
    }).collect();

    let mut liq = imagequant::new();
    // quality range 0-100; clamp to ensure valid range
    let q = quality.clamp(30, 95);
    // Minimum acceptable quality: if 256 colours can't reach it (photos,
    // gradients), quantize() fails and the caller falls back to lossless
    // oxipng. This is what makes "lossy for flat art, lossless for photos"
    // actually happen — a floor of 0 would force-posterize everything.
    liq.set_quality(q.saturating_sub(25), q).ok()?;

    let mut liq_img = liq.new_image(
        pixels.as_slice(),
        width as usize,
        height as usize,
        0.0,
    ).ok()?;

    let mut res = liq.quantize(&mut liq_img).ok()?;
    res.set_dithering_level(1.0).ok()?;

    let (palette, indexed_pixels) = res.remapped(&mut liq_img).ok()?;

    // Encode as indexed PNG
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);

        // Build RGBA palette + alpha/transparency chunk
        let mut palette_rgb = Vec::with_capacity(palette.len() * 3);
        let mut palette_alpha = Vec::with_capacity(palette.len());
        for c in &palette {
            palette_rgb.extend_from_slice(&[c.r, c.g, c.b]);
            palette_alpha.push(c.a);
        }
        encoder.set_palette(palette_rgb);
        encoder.set_trns(palette_alpha);

        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&indexed_pixels).ok()?;
    }

    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(img: &RgbaImage) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn flat_art_is_quantised() {
        // 4-colour flat image: quantisation must succeed under the quality floor
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([if x < 32 { 255 } else { 0 }, if y < 32 { 255 } else { 0 }, 0, 255])
        });
        assert!(quantise(&img, 80).is_some());
        let out = compress(&png_bytes(&img), 80).unwrap();
        assert!(image::load_from_memory(&out).is_ok());
    }

    #[test]
    fn noise_rejects_quantisation_but_still_compresses() {
        // Pseudo-random noise can't hit the quality floor with 256 colours:
        // quantise() must fail so the caller falls back to lossless oxipng
        let mut seed = 0x12345678u32;
        let img = RgbaImage::from_fn(64, 64, |_, _| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = seed.to_le_bytes();
            image::Rgba([b[0], b[1], b[2], 255])
        });
        assert!(quantise(&img, 80).is_none());

        let out = compress(&png_bytes(&img), 80).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));
    }
}
