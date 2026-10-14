use anyhow::Result;
use rgb::RGBA8;

/// Encode image bytes as AVIF using the pure-Rust ravif encoder.
///
/// AVIF uses AV1 intra-frame encoding and typically achieves ~50% smaller
/// files than JPEG at equivalent perceptual quality.
///
/// speed: 1 (slowest/best) – 10 (fastest/worst); 4 is a good default.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let (img, icc) = super::decode_with_meta(data)?;
    // ravif's current wrapper cannot attach source ICC or perform a colour
    // transform. Refuse rather than silently relabel wide-gamut pixels as sRGB.
    anyhow::ensure!(
        icc.is_none(),
        "携带 ICC profile 的图片暂不支持转 AVIF，请改用 PNG/JPEG 或先进行色彩转换"
    );
    let img = img.into_rgba8();
    let (width, height) = img.dimensions();

    let pixels: Vec<RGBA8> = img
        .pixels()
        .map(|p| RGBA8 {
            r: p[0],
            g: p[1],
            b: p[2],
            a: p[3],
        })
        .collect();

    let encoder = ravif::Encoder::new()
        .with_quality(quality as f32)
        .with_alpha_quality(quality as f32)
        .with_speed(4);

    let encoded = encoder.encode_rgba(ravif::Img::new(&pixels, width as usize, height as usize))?;

    Ok(encoded.avif_file)
}
