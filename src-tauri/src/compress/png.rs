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
    let img = image::load_from_memory(data)?.into_rgba8();
    let (width, height) = img.dimensions();

    // --- Lossy quantisation ----------------------------------------------
    let quantised = quantise(&img, quality).unwrap_or_default();

    // --- Lossless oxipng pass on original --------------------------------
    let oxipng_opts = oxipng::Options::from_preset(4);
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

    // Silence unused variable warnings for width/height
    let _ = (width, height);

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
    let q = quality.clamp(30, 95) as u32;
    liq.set_quality(0, q as u8).ok()?;

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
