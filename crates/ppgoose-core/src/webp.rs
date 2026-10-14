use anyhow::Result;

/// Compress WebP bytes using libwebp.
///
/// Lossless sources (VP8L) are re-encoded losslessly so quality is never
/// silently degraded; lossy sources are re-encoded at the requested quality.
/// Animated WebP is rejected upstream in `compress_file`.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let lossless = is_lossless(data);
    compress_mode(data, quality, lossless)
}

pub fn compress_mode(data: &[u8], quality: u8, lossless: bool) -> Result<Vec<u8>> {
    anyhow::ensure!(!is_animated(data), "暂不支持动画 WebP，避免丢失动画帧");
    let (img, icc) = super::decode_with_meta(data)?;
    let img = img.into_rgba8();
    let (width, height) = img.dimensions();
    let pixels = img.into_raw();

    let encoder = webp::Encoder::from_rgba(&pixels, width, height);
    let mut config = webp::WebPConfig::new().map_err(|_| anyhow::anyhow!("WebP 配置初始化失败"))?;
    config.lossless = i32::from(lossless);
    config.quality = if lossless { 75.0 } else { quality as f32 };
    // libwebp's default exact=0 clears RGB under transparent pixels, even
    // in lossless mode. Preserve every source RGBA byte for lossless inputs.
    config.exact = i32::from(lossless);
    let compressed = encoder
        .encode_advanced(&config)
        .map_err(|error| anyhow::anyhow!("WebP 编码失败: {error:?}"))?;
    anyhow::ensure!(!compressed.is_empty(), "WebP 编码器返回空输出");
    with_icc(
        compressed.to_vec(),
        icc.as_deref(),
        width,
        height,
        pixels.chunks_exact(4).any(|pixel| pixel[3] != 255),
    )
}

/// Attach a profile to a static WebP extended container. ICCP must precede
/// image chunks; EXIF orientation is dropped because pixels are already upright.
fn with_icc(
    data: Vec<u8>,
    icc: Option<&[u8]>,
    width: u32,
    height: u32,
    alpha: bool,
) -> Result<Vec<u8>> {
    let Some(icc) = icc.filter(|profile| !profile.is_empty()) else {
        return Ok(data);
    };
    let mut body = Vec::new();
    let mut extended = vec![0x20 | if alpha { 0x10 } else { 0 }, 0, 0, 0];
    extended.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    extended.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    append_chunk(&mut body, b"VP8X", &extended)?;
    append_chunk(&mut body, b"ICCP", icc)?;
    for_each_chunk(&data, |fourcc, payload| {
        if fourcc != b"VP8X" && fourcc != b"ICCP" {
            // Encoder chunk sizes are already u32 and valid.
            body.extend_from_slice(fourcc);
            body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            body.extend_from_slice(payload);
            if payload.len() & 1 != 0 {
                body.push(0);
            }
        }
    });
    let size = u32::try_from(body.len() + 4)?;
    let mut output = b"RIFF".to_vec();
    output.extend_from_slice(&size.to_le_bytes());
    output.extend_from_slice(b"WEBP");
    output.extend(body);
    Ok(output)
}

fn append_chunk(output: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) -> Result<()> {
    output.extend_from_slice(fourcc);
    output.extend_from_slice(&u32::try_from(payload.len())?.to_le_bytes());
    output.extend_from_slice(payload);
    if payload.len() & 1 != 0 {
        output.push(0);
    }
    Ok(())
}

/// True if the WebP container has animation chunks (ANIM/ANMF) or the
/// animation flag set in VP8X.
pub(crate) fn is_animated(data: &[u8]) -> bool {
    let mut animated = false;
    for_each_chunk(data, |fourcc, body| match fourcc {
        b"ANIM" | b"ANMF" => animated = true,
        // VP8X feature flags: bit 1 (0x02) = animation
        b"VP8X" => animated |= body.first().is_some_and(|f| f & 0x02 != 0),
        _ => {}
    });
    animated
}

/// True if the image data is lossless-coded (VP8L chunk).
pub(crate) fn is_lossless(data: &[u8]) -> bool {
    let mut lossless = false;
    for_each_chunk(data, |fourcc, _| {
        if fourcc == b"VP8L" {
            lossless = true;
        }
    });
    lossless
}

/// Walk the top-level RIFF chunks of a WebP file.
fn for_each_chunk(data: &[u8], mut f: impl FnMut(&[u8], &[u8])) {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return;
    }
    let declared = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let Some(end) = declared.checked_add(8) else {
        return;
    };
    if end != data.len() {
        return;
    }
    let mut off: usize = 12;
    while off + 8 <= data.len() {
        let fourcc = &data[off..off + 4];
        let size = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap()) as usize;
        let Some(body_end) = off.checked_add(8).and_then(|v| v.checked_add(size)) else {
            return;
        };
        if body_end > end {
            return;
        }
        f(fourcc, &data[off + 8..body_end]);
        // chunk payloads are padded to even length
        let Some(next) = body_end.checked_add(size & 1) else {
            return;
        };
        off = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> image::RgbaImage {
        image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 128, 255])
        })
    }

    #[test]
    fn detects_lossless_source_and_keeps_it_lossless() {
        let img = gradient(32, 32);
        let src = webp::Encoder::from_rgba(img.as_raw(), 32, 32)
            .encode_lossless()
            .to_vec();
        assert!(is_lossless(&src));
        assert!(!is_animated(&src));

        // Re-encoded output must still be lossless: identical pixels
        let out = compress(&src, 80).unwrap();
        let decoded = image::load_from_memory(&out).unwrap().into_rgba8();
        assert_eq!(decoded.as_raw(), img.as_raw());
    }

    #[test]
    fn lossy_source_is_not_flagged_lossless() {
        let img = gradient(32, 32);
        let src = webp::Encoder::from_rgba(img.as_raw(), 32, 32)
            .encode(80.0)
            .to_vec();
        assert!(!is_lossless(&src));
        assert!(!is_animated(&src));
        assert!(compress(&src, 80).is_ok());
    }

    #[test]
    fn lossless_keeps_rgb_under_fully_transparent_pixels() {
        let img = image::RgbaImage::from_fn(32, 16, |x, y| {
            image::Rgba([x as u8 * 7, y as u8 * 11, 173, if x < 16 { 0 } else { 128 }])
        });
        let mut source = std::io::Cursor::new(Vec::new());
        img.write_to(&mut source, image::ImageFormat::Png).unwrap();
        let output = compress_mode(&source.into_inner(), 80, true).unwrap();
        assert!(is_lossless(&output));
        assert_eq!(image::load_from_memory(&output).unwrap().into_rgba8(), img);
    }

    #[test]
    fn icc_survives_static_webp_reencoding_and_resize() {
        use image::{ImageDecoder, ImageEncoder};
        let profile = include_bytes!("../tests/fixtures/srgb.icc").to_vec();
        for alpha in [0, 128, 255] {
            let img = image::RgbaImage::from_pixel(32, 16, image::Rgba([220, 70, 30, alpha]));
            let mut source = Vec::new();
            let mut encoder = image::codecs::png::PngEncoder::new(&mut source);
            encoder.set_icc_profile(profile.clone()).unwrap();
            encoder
                .write_image(img.as_raw(), 32, 16, image::ExtendedColorType::Rgba8)
                .unwrap();
            for lossless in [false, true] {
                let webp = compress_mode(&source, 80, lossless).unwrap();
                let out = crate::compress_bytes(
                    &webp,
                    &crate::EncodingOptions {
                        max_edge: Some(16),
                        ..Default::default()
                    },
                )
                .unwrap();
                let mut decoder = image::ImageReader::new(std::io::Cursor::new(&out.bytes))
                    .with_guessed_format()
                    .unwrap()
                    .into_decoder()
                    .unwrap();
                assert_eq!(decoder.icc_profile().unwrap(), Some(profile.clone()));
                assert_eq!(decoder.dimensions(), (16, 8));
                assert_eq!(is_lossless(&out.bytes), lossless);
            }
        }
    }

    #[test]
    fn failed_webp_encoding_is_an_error_instead_of_a_panic() {
        let img = image::RgbaImage::from_pixel(16384, 1, image::Rgba([1, 2, 3, 255]));
        let mut source = std::io::Cursor::new(Vec::new());
        img.write_to(&mut source, image::ImageFormat::Png).unwrap();
        assert!(compress_mode(&source.into_inner(), 80, true).is_err());
    }

    #[test]
    fn detects_animation_flag_in_vp8x() {
        let mut data = Vec::new();
        data.extend(b"RIFF");
        data.extend(&[0u8; 4]);
        data.extend(b"WEBP");
        data.extend(b"VP8X");
        data.extend(&10u32.to_le_bytes());
        data.extend(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let len = (data.len() - 8) as u32;
        data[4..8].copy_from_slice(&len.to_le_bytes());
        assert!(is_animated(&data));
    }
}
