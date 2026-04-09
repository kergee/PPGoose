use anyhow::Result;

/// Compress WebP bytes using libwebp lossy encoder.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let img = image::load_from_memory(data)?.into_rgba8();
    let (width, height) = img.dimensions();
    let pixels = img.into_raw();

    let encoder = webp::Encoder::from_rgba(&pixels, width, height);
    let compressed = encoder.encode(quality as f32);

    Ok(compressed.to_vec())
}
