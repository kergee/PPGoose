use anyhow::Result;

/// Compress WebP bytes using libwebp.
///
/// Lossless sources (VP8L) are re-encoded losslessly so quality is never
/// silently degraded; lossy sources are re-encoded at the requested quality.
/// Animated WebP is rejected upstream in `compress_file`.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let lossless = is_lossless(data);

    let img = super::decode_oriented(data)?.into_rgba8();
    let (width, height) = img.dimensions();
    let pixels = img.into_raw();

    let encoder = webp::Encoder::from_rgba(&pixels, width, height);
    let compressed = if lossless {
        encoder.encode_lossless()
    } else {
        encoder.encode(quality as f32)
    };

    Ok(compressed.to_vec())
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
    let mut off = 12;
    while off + 8 <= data.len() {
        let fourcc = &data[off..off + 4];
        let size = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap()) as usize;
        let body_end = (off + 8 + size).min(data.len());
        f(fourcc, &data[off + 8..body_end]);
        // chunk payloads are padded to even length
        off += 8 + size + (size & 1);
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
    fn detects_animation_flag_in_vp8x() {
        let mut data = Vec::new();
        data.extend(b"RIFF");
        data.extend(&[0u8; 4]);
        data.extend(b"WEBP");
        data.extend(b"VP8X");
        data.extend(&10u32.to_le_bytes());
        data.extend(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(is_animated(&data));
    }
}
