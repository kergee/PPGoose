use anyhow::Result;
use image::ColorType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JpegStrategy {
    /// Existing q80 auto policy; 4:4:4 only at q >= 90.
    #[default]
    Balanced,
    /// Keep colour edges at every quality; use upstream table 3.
    Detail,
    /// Compact encoding: auto q67, 4:4:4, table 3.
    Compact,
}

impl JpegStrategy {
    pub(crate) fn auto_quality(self) -> u8 {
        if self == Self::Compact {
            67
        } else {
            80
        }
    }
}

/// Compress JPEG bytes using Mozilla's mozjpeg encoder.
///
/// mozjpeg produces files ~30-40% smaller than standard libjpeg
/// at equivalent visual quality. Progressive mode is enabled (one of
/// mozjpeg's main wins over baseline libjpeg). The source's ICC colour
/// profile, if any, is carried over so wide-gamut images keep their colours.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    compress_with_strategy(data, quality, JpegStrategy::Balanced)
}

pub fn compress_with_strategy(data: &[u8], quality: u8, strategy: JpegStrategy) -> Result<Vec<u8>> {
    let quality = if quality == 0 {
        strategy.auto_quality()
    } else {
        quality
    };
    anyhow::ensure!(quality <= 100, "JPEG 质量必须在 0–100 之间");
    let (img, icc) = super::decode_with_meta(data)?;
    let img = if img.color().has_alpha() {
        let rgba = img.into_rgba8();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(
            rgba.width(),
            rgba.height(),
            |x, y| {
                let p = rgba.get_pixel(x, y);
                let a = u16::from(p[3]);
                image::Rgb(
                    [0, 1, 2].map(|c| ((u16::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8),
                )
            },
        ))
    } else {
        img
    };
    let width = img.width() as usize;
    let height = img.height() as usize;

    // Grayscale sources stay single-channel: smaller output, no chroma noise.
    let grayscale = matches!(
        img.color(),
        ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16
    );

    let (pixels, color_space) = if grayscale {
        (
            img.into_luma8().into_raw(),
            mozjpeg::ColorSpace::JCS_GRAYSCALE,
        )
    } else {
        (img.into_rgb8().into_raw(), mozjpeg::ColorSpace::JCS_RGB)
    };

    // mozjpeg reports errors via panics; catch_unwind turns them into
    // per-file errors (requires the default unwinding panic strategy).
    std::panic::catch_unwind(|| {
        encode_mozjpeg(
            &pixels,
            width,
            height,
            quality,
            color_space,
            icc.as_deref(),
            strategy,
        )
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
    strategy: JpegStrategy,
) -> Result<Vec<u8>> {
    let mut comp = mozjpeg::Compress::new(color_space);
    comp.set_size(width, height);
    comp.set_quality(quality as f32);
    comp.set_progressive_mode();
    comp.set_optimize_coding(true);

    if strategy != JpegStrategy::Balanced {
        // N. Robidoux is table index 3 in upstream mozjpeg jcparam.c, used
        // for both luma and chroma. The safe API clamps to baseline 1..255.
        let table = mozjpeg::qtable::NRobidoux.scaled(quality as f32, quality as f32);
        comp.set_luma_qtable(&table);
        comp.set_chroma_qtable(&table);
        comp.set_optimize_scans(true);
    }

    // At high quality, 4:2:0 chroma subsampling causes visible colour
    // fringing on text/screenshots — switch to 4:4:4.
    if color_space == mozjpeg::ColorSpace::JCS_RGB
        && (quality >= 90 || strategy != JpegStrategy::Balanced)
    {
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

    fn markers(data: &[u8], requested: u8) -> Vec<&[u8]> {
        let mut pos = 2;
        let mut result = Vec::new();
        while pos + 4 <= data.len() {
            assert_eq!(data[pos], 0xff);
            let marker = data[pos + 1];
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            let length = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            if marker == requested {
                result.push(&data[pos + 4..pos + 2 + length]);
            }
            pos += 2 + length;
        }
        result
    }

    #[test]
    fn compact_auto_uses_table_three_q67_and_444_sampling() {
        let img = image::RgbImage::from_fn(48, 32, |x, y| {
            image::Rgb([(x * 5) as u8, (y * 7) as u8, 100])
        });
        let mut input = std::io::Cursor::new(Vec::new());
        img.write_to(&mut input, image::ImageFormat::Png).unwrap();
        let input = input.into_inner();
        let output = compress_with_strategy(&input, 0, JpegStrategy::Compact).unwrap();
        assert_eq!(
            output,
            compress_with_strategy(&input, 67, JpegStrategy::Compact).unwrap()
        );
        let sof = markers(&output, 0xc2);
        assert_eq!(sof[0][5], 3);
        for index in [7, 10, 13] {
            assert_eq!(sof[0][index], 0x11);
        }
        let tables = markers(&output, 0xdb);
        // One DQT marker may contain several tables.
        assert!(!tables.is_empty());
        // Upstream table 3: first coefficient 16 -> 11 at quality 67,
        // last 418 -> baseline-clamped 255 (DQT's last zigzag element).
        for segment in tables {
            assert_eq!(segment.len() % 65, 0);
            for table in segment.chunks_exact(65) {
                assert_eq!(table[0] >> 4, 0, "baseline-clamped 8-bit DQT");
                assert_eq!(table[1], 11);
                assert_eq!(table[64], 255);
            }
        }
    }

    #[test]
    fn strategy_respects_manual_quality_and_keeps_grayscale() {
        let image = image::GrayImage::from_fn(32, 16, |x, y| image::Luma([(x + y * 9) as u8]));
        let mut source = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut source, image::ImageFormat::Png)
            .unwrap();
        let source = source.into_inner();
        let output = compress_with_strategy(&source, 90, JpegStrategy::Compact).unwrap();
        assert_ne!(
            output,
            compress_with_strategy(&source, 67, JpegStrategy::Compact).unwrap()
        );
        assert_eq!(
            image::load_from_memory(&output).unwrap().color(),
            ColorType::L8
        );
    }

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
