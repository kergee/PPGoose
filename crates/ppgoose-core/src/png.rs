use anyhow::Result;
use image::RgbaImage;
use rgb::RGBA8;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PngStrategy {
    Balanced,
    #[default]
    Detail,
    Lossless,
}

/// Compress PNG bytes.
///
/// Strategy (auto):
///   1. Decode to RGBA pixels with `image` crate.
///   2. Run `imagequant` lossy palette quantisation (max 256 colours).
///   3. Re-encode as indexed PNG with `png` crate.
///   4. Run `oxipng` lossless optimiser on the result.
///   5. Compare all candidates and return the smallest.
#[allow(dead_code)]
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    compress_with_strategy(data, quality, PngStrategy::Balanced)
}

pub fn compress_with_strategy(data: &[u8], quality: u8, strategy: PngStrategy) -> Result<Vec<u8>> {
    // Only PNG bytes can be a valid original/lossless PNG candidate.
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return convert_lossless(data);
    }
    let reader = png::Decoder::new(std::io::Cursor::new(data)).read_info()?;
    let info = reader.info();
    if strategy == PngStrategy::Lossless
        || info.animation_control.is_some()
        || info.bit_depth == png::BitDepth::Sixteen
    {
        let options = oxipng::Options::from_preset(2);
        let optimized =
            oxipng::optimize_from_memory(data, &options).unwrap_or_else(|_| data.to_vec());
        return Ok(if optimized.len() < data.len() {
            optimized
        } else {
            data.to_vec()
        });
    }
    // --- Decode ----------------------------------------------------------
    let img = super::decode_oriented(data)?.into_rgba8();

    // --- Lossy quantisation ----------------------------------------------
    let (minimum, maximum) = quality_bounds(quality, strategy);
    let quantised = quantise_with_meta(&img, minimum, maximum, info).unwrap_or_default();

    // --- Lossless oxipng pass on original --------------------------------
    // Preset 2 is ~4x faster than 4 on large images for a marginal size difference.
    let oxipng_opts = oxipng::Options::from_preset(2);
    let lossless =
        oxipng::optimize_from_memory(data, &oxipng_opts).unwrap_or_else(|_| data.to_vec());

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

fn quality_bounds(quality: u8, strategy: PngStrategy) -> (u8, u8) {
    // floor (80 - 25) so difficult photos/noise still fall back to lossless.
    if quality == 0 && strategy == PngStrategy::Detail {
        return (55, 98);
    }
    let q = if quality == 0 { 80 } else { quality }.clamp(30, 100);
    let q = if strategy == PngStrategy::Balanced {
        q.min(95)
    } else {
        q
    };
    (q.saturating_sub(25), q)
}

/// Conversion means preserving decoded pixels, not returning a smaller WebP.
pub fn convert_lossless(data: &[u8]) -> Result<Vec<u8>> {
    let (image, icc) = super::decode_with_meta(data)?;
    let image = image.into_rgba8();
    let mut info = png::Info::with_size(image.width(), image.height());
    info.color_type = png::ColorType::Rgba;
    info.bit_depth = png::BitDepth::Eight;
    info.icc_profile = icc.map(Cow::Owned);
    let mut output = Vec::new();
    {
        let encoder = png::Encoder::with_info(&mut output, info)?;
        encoder.write_header()?.write_image_data(image.as_raw())?;
    }
    Ok(output)
}

/// Returns empty vec on failure (caller falls back to original).
#[cfg(test)]
fn quantise(img: &RgbaImage, quality: u8) -> Option<Vec<u8>> {
    let (minimum, maximum) = quality_bounds(quality, PngStrategy::Balanced);
    quantise_with_meta(
        img,
        minimum,
        maximum,
        &png::Info::with_size(img.width(), img.height()),
    )
}

fn quantise_with_meta(
    img: &RgbaImage,
    minimum: u8,
    maximum: u8,
    source: &png::Info<'_>,
) -> Option<Vec<u8>> {
    let (width, height) = img.dimensions();

    // Build pixel slice for imagequant (RGBA8)
    let pixels: Vec<RGBA8> = img
        .pixels()
        .map(|p| {
            let c = p.0;
            RGBA8 {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            }
        })
        .collect();

    let mut liq = imagequant::new();
    // quality range 0-100; clamp to ensure valid range
    // Minimum acceptable quality: if 256 colours can't reach it (photos,
    // gradients), quantize() fails and the caller falls back to lossless
    // oxipng. This is what makes "lossy for flat art, lossless for photos"
    // actually happen — a floor of 0 would force-posterize everything.
    liq.set_quality(minimum, maximum).ok()?;

    let gamma = if source.srgb.is_some() {
        0.0
    } else {
        source
            .source_gamma
            .map_or(0.0, |gamma| f64::from(gamma.into_value()))
    };
    let mut liq_img = liq
        .new_image(pixels.as_slice(), width as usize, height as usize, gamma)
        .ok()?;

    let mut res = liq.quantize(&mut liq_img).ok()?;
    res.set_dithering_level(1.0).ok()?;

    let (palette, indexed_pixels) = res.remapped(&mut liq_img).ok()?;

    // Encode as indexed PNG
    let mut output = Vec::new();
    {
        let mut info = png::Info::with_size(width, height);
        info.icc_profile = source.icc_profile.as_ref().map(|v| Cow::Owned(v.to_vec()));
        info.source_gamma = source.source_gamma;
        info.source_chromaticities = source.source_chromaticities;
        info.srgb = source.srgb;
        let mut encoder = png::Encoder::with_info(&mut output, info).ok()?;
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

/// Resize intermediates retain the source colour interpretation. Dropping
/// gAMA/cHRM/sRGB here would make the subsequent quantizer use default gamma.
pub(crate) fn encode_resized(image: &image::DynamicImage, source: &[u8]) -> Result<Vec<u8>> {
    let reader = png::Decoder::new(source).read_info()?;
    let original = reader.info();
    let mut info = png::Info::with_size(image.width(), image.height());
    info.icc_profile = original
        .icc_profile
        .as_ref()
        .map(|v| Cow::Owned(v.to_vec()));
    info.source_gamma = original.source_gamma;
    info.source_chromaticities = original.source_chromaticities;
    info.srgb = original.srgb;
    let (colour, depth) = match image.color() {
        image::ColorType::L8 => (png::ColorType::Grayscale, png::BitDepth::Eight),
        image::ColorType::La8 => (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight),
        image::ColorType::Rgb8 => (png::ColorType::Rgb, png::BitDepth::Eight),
        image::ColorType::Rgba8 => (png::ColorType::Rgba, png::BitDepth::Eight),
        image::ColorType::L16 => (png::ColorType::Grayscale, png::BitDepth::Sixteen),
        image::ColorType::La16 => (png::ColorType::GrayscaleAlpha, png::BitDepth::Sixteen),
        image::ColorType::Rgb16 => (png::ColorType::Rgb, png::BitDepth::Sixteen),
        image::ColorType::Rgba16 => (png::ColorType::Rgba, png::BitDepth::Sixteen),
        _ => anyhow::bail!("不支持的 PNG 像素类型"),
    };
    info.color_type = colour;
    info.bit_depth = depth;
    let pixels: Cow<'_, [u8]> = if depth == png::BitDepth::Sixteen {
        Cow::Owned(
            image
                .as_bytes()
                .chunks_exact(2)
                .flat_map(|v| u16::from_ne_bytes([v[0], v[1]]).to_be_bytes())
                .collect(),
        )
    } else {
        Cow::Borrowed(image.as_bytes())
    };
    let mut output = Vec::new();
    png::Encoder::with_info(&mut output, info)?
        .write_header()?
        .write_image_data(&pixels)?;
    Ok(output)
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
            image::Rgba([
                if x < 32 { 255 } else { 0 },
                if y < 32 { 255 } else { 0 },
                0,
                255,
            ])
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
        let detail = compress_with_strategy(&png_bytes(&img), 0, PngStrategy::Detail).unwrap();
        assert_eq!(image::load_from_memory(&detail).unwrap().into_rgba8(), img);
    }

    #[test]
    fn lossless_mode_preserves_rgba_pixels() {
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([x as u8, y as u8, 123, (x * 4) as u8])
        });
        let output = compress_with_strategy(&png_bytes(&img), 0, PngStrategy::Lossless).unwrap();
        assert_eq!(image::load_from_memory(&output).unwrap().into_rgba8(), img);
    }

    #[test]
    fn conversion_returns_real_png_when_webp_is_smaller() {
        let img = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let source = webp::Encoder::from_rgba(img.as_raw(), 1, 1)
            .encode_lossless()
            .to_vec();
        let output = compress_with_strategy(&source, 0, PngStrategy::Detail).unwrap();
        assert!(output.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(image::load_from_memory(&output).unwrap().into_rgba8(), img);
    }

    #[test]
    fn automatic_and_manual_quality_have_distinct_bounds() {
        assert_eq!(quality_bounds(0, PngStrategy::Detail), (55, 98));
        assert_eq!(quality_bounds(80, PngStrategy::Detail), (55, 80));
        assert_eq!(quality_bounds(0, PngStrategy::Balanced), (55, 80));
    }

    #[test]
    fn animated_png_preserves_all_frames() {
        let mut data = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut data, 8, 8);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_animated(2, 3).unwrap();
            let mut writer = enc.write_header().unwrap();
            writer.set_frame_delay(1, 10).unwrap();
            writer
                .write_image_data(&[255u8, 0, 0, 255].repeat(64))
                .unwrap();
            writer
                .write_image_data(&[0u8, 255, 0, 255].repeat(64))
                .unwrap();
        }
        let output = compress_with_strategy(&data, 0, PngStrategy::Detail).unwrap();
        let reader = png::Decoder::new(output.as_slice()).read_info().unwrap();
        let control = reader.info().animation_control.unwrap();
        assert_eq!((control.num_frames, control.num_plays), (2, 3));
        use image::AnimationDecoder;
        let before = image::codecs::png::PngDecoder::new(std::io::Cursor::new(data))
            .unwrap()
            .apng()
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        let after = image::codecs::png::PngDecoder::new(std::io::Cursor::new(output))
            .unwrap()
            .apng()
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        for (a, b) in before.iter().zip(after.iter()) {
            assert_eq!(a.buffer(), b.buffer());
            assert_eq!(a.delay(), b.delay());
        }
    }

    #[test]
    fn sixteen_bit_png_preserves_precision() {
        let mut data = Vec::new();
        let pixels: Vec<u8> = (0..64u16).flat_map(|v| (v * 997).to_be_bytes()).collect();
        {
            let mut enc = png::Encoder::new(&mut data, 8, 8);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Sixteen);
            enc.write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
        }
        let output = compress_with_strategy(&data, 0, PngStrategy::Detail).unwrap();
        assert_eq!(
            image::load_from_memory(&output).unwrap().into_luma16(),
            image::load_from_memory(&data).unwrap().into_luma16()
        );
    }

    #[test]
    fn quantized_png_preserves_icc_profile() {
        let image = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([
                if x < 32 { 255 } else { 0 },
                if y < 32 { 255 } else { 0 },
                0,
                255,
            ])
        });
        let profile = include_bytes!("../tests/fixtures/srgb.icc").to_vec();
        let mut source = png::Info::with_size(64, 64);
        source.icc_profile = Some(Cow::Owned(profile.clone()));
        let output = quantise_with_meta(&image, 55, 98, &source).unwrap();
        let reader = png::Decoder::new(output.as_slice()).read_info().unwrap();
        assert_eq!(
            reader.info().icc_profile.as_deref(),
            Some(profile.as_slice())
        );
    }
}
