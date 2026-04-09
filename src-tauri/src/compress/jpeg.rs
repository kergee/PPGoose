use anyhow::Result;

/// Compress JPEG bytes using Mozilla's mozjpeg encoder.
///
/// mozjpeg produces files ~30-40% smaller than standard libjpeg
/// at equivalent visual quality.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let img = image::load_from_memory(data)?.into_rgb8();
    let (width, height) = img.dimensions();
    let pixels = img.into_raw();

    std::panic::catch_unwind(|| {
        encode_mozjpeg(&pixels, width as usize, height as usize, quality)
    })
    .map_err(|_| anyhow::anyhow!("mozjpeg panicked"))?
}

fn encode_mozjpeg(pixels: &[u8], width: usize, height: usize, quality: u8) -> Result<Vec<u8>> {
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_quality(quality as f32);
    comp.set_optimize_coding(true);

    // start_compress takes a writer and returns CompressStarted
    let mut started = comp
        .start_compress(Vec::new())
        .map_err(|e| anyhow::anyhow!("start_compress: {e}"))?;

    started
        .write_scanlines(pixels)
        .map_err(|e| anyhow::anyhow!("write_scanlines: {e}"))?;

    let buf = started
        .finish()
        .map_err(|e| anyhow::anyhow!("finish: {e}"))?;

    Ok(buf)
}
