use anyhow::Result;
use image::ColorType;

/// Compress JPEG bytes using Mozilla's mozjpeg encoder.
///
/// mozjpeg produces files ~30-40% smaller than standard libjpeg
/// at equivalent visual quality. Progressive mode is enabled (one of
/// mozjpeg's main wins over baseline libjpeg). The source's ICC colour
/// profile, if any, is carried over so wide-gamut images keep their colours.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let (img, icc) = super::decode_with_meta(data)?;
    let width = img.width() as usize;
    let height = img.height() as usize;

    // Grayscale sources stay single-channel: smaller output, no chroma noise.
    let grayscale = matches!(
        img.color(),
        ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16
    );

    let (pixels, color_space) = if grayscale {
        (img.into_luma8().into_raw(), mozjpeg::ColorSpace::JCS_GRAYSCALE)
    } else {
        (img.into_rgb8().into_raw(), mozjpeg::ColorSpace::JCS_RGB)
    };

    // mozjpeg reports errors via panics; catch_unwind turns them into
    // per-file errors (requires the default unwinding panic strategy).
    std::panic::catch_unwind(|| {
        encode_mozjpeg(&pixels, width, height, quality, color_space, icc.as_deref())
    })
    .map_err(|_| anyhow::anyhow!("mozjpeg panicked"))?
}

fn encode_mozjpeg(
    pixels: &[u8],
    width: usize,
    height: usize,
    quality: u8,
    color_space: mozjpeg::ColorSpace,
    icc: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut comp = mozjpeg::Compress::new(color_space);
    comp.set_size(width, height);
    comp.set_quality(quality as f32);
    comp.set_progressive_mode();
    comp.set_optimize_coding(true);

    // At high quality, 4:2:0 chroma subsampling causes visible colour
    // fringing on text/screenshots — switch to 4:4:4.
    if color_space == mozjpeg::ColorSpace::JCS_RGB && quality >= 90 {
        comp.set_chroma_sampling_pixel_sizes((1, 1), (1, 1));
    }

    // start_compress takes a writer and returns CompressStarted
    let mut started = comp
        .start_compress(Vec::new())
        .map_err(|e| anyhow::anyhow!("start_compress: {e}"))?;

    if let Some(icc) = icc {
        write_icc_markers(&mut started, icc);
    }

    started
        .write_scanlines(pixels)
        .map_err(|e| anyhow::anyhow!("write_scanlines: {e}"))?;

    let buf = started
        .finish()
        .map_err(|e| anyhow::anyhow!("finish: {e}"))?;

    Ok(buf)
}

/// Write an ICC profile as APP2 markers per the JPEG/ICC spec:
/// each marker carries "ICC_PROFILE\0" + 1-based chunk index + chunk count.
fn write_icc_markers<W: std::io::Write>(
    started: &mut mozjpeg::compress::CompressStarted<W>,
    icc: &[u8],
) {
    const HEADER: &[u8] = b"ICC_PROFILE\0";
    // APP marker payload limit (65533) minus the 14-byte ICC chunk header
    const CHUNK: usize = 65519;

    let chunks: Vec<&[u8]> = icc.chunks(CHUNK).collect();
    if chunks.is_empty() || chunks.len() > 255 {
        return;
    }
    for (i, chunk) in chunks.iter().enumerate() {
        let mut payload = Vec::with_capacity(HEADER.len() + 2 + chunk.len());
        payload.extend_from_slice(HEADER);
        payload.push((i + 1) as u8);
        payload.push(chunks.len() as u8);
        payload.extend_from_slice(chunk);
        started.write_marker(mozjpeg::Marker::APP(2), &payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_roundtrip() {
        let img = image::RgbImage::from_fn(48, 32, |x, y| {
            image::Rgb([(x * 5) as u8, (y * 7) as u8, 100])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();

        let out = compress(&buf.into_inner(), 80).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (48, 32));
    }

    #[test]
    fn grayscale_stays_single_channel() {
        let img = image::GrayImage::from_fn(32, 32, |x, y| image::Luma([(x + y * 4) as u8]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();

        let out = compress(&buf.into_inner(), 80).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.color(), ColorType::L8);
    }

    #[test]
    fn icc_profile_is_carried_over() {
        use image::{ImageDecoder, ImageEncoder};

        let icc = b"fake-icc-profile-for-test".to_vec();

        // Build a source JPEG that carries an ICC profile
        let img = image::RgbImage::from_pixel(16, 16, image::Rgb([200, 100, 50]));
        let mut src = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut src);
        encoder.set_icc_profile(icc.clone()).unwrap();
        encoder
            .write_image(img.as_raw(), 16, 16, image::ExtendedColorType::Rgb8)
            .unwrap();

        let out = compress(&src, 80).unwrap();

        let mut decoder = image::ImageReader::new(std::io::Cursor::new(out.as_slice()))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        assert_eq!(decoder.icc_profile().unwrap(), Some(icc));
    }
}
