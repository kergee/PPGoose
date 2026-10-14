use super::{avif, gif, jpeg, png, smart, webp, CompressOptions, ImageFormat};
use anyhow::Result;
use image::{AnimationDecoder, ImageEncoder};

pub struct Processed {
    pub bytes: Vec<u8>,
    pub resized: bool,
    pub width: u32,
    pub height: u32,
}

/// Search from the same original pixels at each size, never from a previously
/// lossy candidate. A successful target-size result is always <= the byte cap.
pub fn process(
    original: &[u8],
    source: &ImageFormat,
    target: &ImageFormat,
    opts: &CompressOptions,
) -> Result<Processed> {
    let (image, icc) = super::decode_with_meta(original)?;
    let (width, height) = (image.width(), image.height());
    let original_edge = width.max(height);
    let mut edge = opts.max_edge.unwrap_or(original_edge).min(original_edge);
    let cap = opts.target_size_kb.map(|kb| u64::from(kb) * 1024);
    let apng = *source == ImageFormat::Png
        && ::png::Decoder::new(original)
            .read_info()?
            .info()
            .animation_control
            .is_some();
    let fixed_pixels =
        (*target == ImageFormat::Png && opts.png_strategy == png::PngStrategy::Lossless) || apng;
    if edge < original_edge && fixed_pixels {
        anyhow::bail!("像素无损 PNG 或 APNG 不支持缩小尺寸，请关闭尺寸调整或选择其他 PNG 策略");
    }
    let lossless_webp = *source == ImageFormat::WebP && webp::is_lossless(original);
    if source == target && edge == original_edge && cap.is_some_and(|n| original.len() as u64 <= n)
    {
        return Ok(Processed {
            bytes: original.to_vec(),
            resized: false,
            width,
            height,
        });
    }
    let quality_ceiling = if opts.quality == 0 { 100 } else { opts.quality };
    let normal_ceiling = if *target == ImageFormat::Jpeg {
        opts.jpeg_strategy.auto_quality()
    } else if *target == ImageFormat::Png && opts.png_strategy == png::PngStrategy::Detail {
        98
    } else {
        80
    };
    let initial_quality =
        if cap.is_some() && opts.quality == 0 && opts.quality_floor > normal_ceiling {
            opts.quality_floor
        } else {
            opts.quality
        };
    let searchable = !(fixed_pixels
        || lossless_webp
        || (*target == ImageFormat::Png && *source != ImageFormat::Png));
    let mut smallest = u64::MAX;
    // At most 5 distinct dimensions, each with a bounded integer-quality search.
    for _ in 0..5 {
        let resized = edge < original_edge;
        let (input, dimensions) = if !resized {
            (original.to_vec(), (width, height))
        } else if *source == ImageFormat::Gif {
            resize_gif(original, edge)?
        } else {
            let pixels = image.resize(edge, edge, image::imageops::FilterType::Lanczos3);
            let dimensions = (pixels.width(), pixels.height());
            if *source == ImageFormat::Png {
                (png::encode_resized(&pixels, original)?, dimensions)
            } else {
                let mut bytes = Vec::new();
                let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
                if let Some(icc) = icc.clone() {
                    encoder.set_icc_profile(icc)?;
                }
                encoder.write_image(
                    pixels.as_bytes(),
                    pixels.width(),
                    pixels.height(),
                    pixels.color().into(),
                )?;
                (bytes, dimensions)
            }
        };
        let initial = encode(
            &input,
            source,
            target,
            opts,
            initial_quality,
            lossless_webp,
            cap.is_none(),
        )?;
        smallest = smallest.min(initial.len() as u64);
        if cap.is_none_or(|limit| initial.len() as u64 <= limit) {
            return Ok(Processed {
                bytes: initial,
                resized,
                width: dimensions.0,
                height: dimensions.1,
            });
        }
        let limit = cap.unwrap();
        let mut best: Option<Vec<u8>> = None;
        if searchable {
            let mut low = u16::from(opts.quality_floor);
            let mut high = u16::from(quality_ceiling);
            while low <= high {
                let quality = ((low + high) / 2) as u8;
                let candidate =
                    encode(&input, source, target, opts, quality, lossless_webp, false)?;
                smallest = smallest.min(candidate.len() as u64);
                if candidate.len() as u64 <= limit {
                    best = Some(candidate);
                    low = u16::from(quality) + 1;
                } else {
                    high = u16::from(quality).saturating_sub(1);
                }
            }
            // Palette and progressive encoders are not strictly monotonic.
            // Always probe the floor before resizing or reporting failure.
            if best.is_none() {
                let candidate = encode(
                    &input,
                    source,
                    target,
                    opts,
                    opts.quality_floor,
                    lossless_webp,
                    false,
                )?;
                smallest = smallest.min(candidate.len() as u64);
                if candidate.len() as u64 <= limit {
                    best = Some(candidate);
                }
            }
        }
        if let Some(bytes) = best {
            return Ok(Processed {
                bytes,
                resized,
                width: dimensions.0,
                height: dimensions.1,
            });
        }
        if !opts.allow_resize || fixed_pixels || edge <= 8 {
            break;
        }
        let scale = ((limit as f64 / smallest as f64).sqrt() * 0.9).clamp(0.15, 0.85);
        let next = ((edge as f64 * scale).floor() as u32)
            .max(8)
            .min(edge.saturating_sub(1));
        if next == 0 || next >= edge {
            break;
        }
        edge = next;
    }
    anyhow::bail!(
        "无法在当前质量和尺寸限制下达到 {} KB；最小候选约 {:.1} KB。请允许缩小尺寸或调整最低质量",
        opts.target_size_kb.unwrap(),
        smallest as f64 / 1024.0
    )
}

fn encode(
    data: &[u8],
    source: &ImageFormat,
    target: &ImageFormat,
    opts: &CompressOptions,
    quality: u8,
    lossless_webp: bool,
    ordinary: bool,
) -> Result<Vec<u8>> {
    let q = if quality == 0 { 80 } else { quality };
    let smart = ordinary && opts.smart_quality && opts.quality == 0;
    match target {
        ImageFormat::Png if *source != ImageFormat::Png => png::convert_lossless(data),
        ImageFormat::Png => png::compress_with_strategy(data, quality, opts.png_strategy),
        ImageFormat::Jpeg if smart => smart::search(data, |q| {
            jpeg::compress_with_strategy(data, q, opts.jpeg_strategy)
        }),
        ImageFormat::Jpeg => jpeg::compress_with_strategy(data, quality, opts.jpeg_strategy),
        ImageFormat::Gif => gif::compress(data, q),
        ImageFormat::WebP if smart && !lossless_webp => {
            smart::search(data, |q| webp::compress_mode(data, q, false))
        }
        ImageFormat::WebP => webp::compress_mode(data, q, lossless_webp),
        ImageFormat::Avif => avif::compress(data, q),
    }
}

fn resize_gif(data: &[u8], edge: u32) -> Result<(Vec<u8>, (u32, u32))> {
    let repeat = ::gif::DecodeOptions::new()
        .read_info(std::io::Cursor::new(data))?
        .repeat();
    let frames = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data))?
        .into_frames()
        .collect_frames()?;
    let mut output = Vec::new();
    let mut dimensions = (0, 0);
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut output);
        encoder.set_repeat(match repeat {
            ::gif::Repeat::Infinite => image::codecs::gif::Repeat::Infinite,
            ::gif::Repeat::Finite(n) => image::codecs::gif::Repeat::Finite(n),
        })?;
        for frame in frames {
            let delay = frame.delay();
            let pixels = image::DynamicImage::ImageRgba8(frame.into_buffer())
                .resize(edge, edge, image::imageops::FilterType::Lanczos3)
                .into_rgba8();
            dimensions = pixels.dimensions();
            encoder.encode_frame(image::Frame::from_parts(pixels, 0, 0, delay))?;
        }
    }
    Ok((output, dimensions))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn jpeg_source() -> Vec<u8> {
        let mut seed = 7u32;
        let image = image::RgbImage::from_fn(160, 120, |_, _| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = seed.to_le_bytes();
            image::Rgb([b[0], b[1], b[2]])
        });
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
            .encode_image(&image)
            .unwrap();
        bytes
    }
    #[test]
    fn jpeg_target_is_enforced_at_original_size() {
        let options = CompressOptions {
            target_size_kb: Some(10),
            quality_floor: 20,
            ..Default::default()
        };
        let output = process(
            &jpeg_source(),
            &ImageFormat::Jpeg,
            &ImageFormat::Jpeg,
            &options,
        )
        .unwrap();
        assert!(output.bytes.len() <= 10 * 1024);
        assert_eq!((output.width, output.height), (160, 120));
        assert!(!output.resized);
    }
    #[test]
    fn impossible_target_fails_instead_of_returning_oversized_file() {
        let options = CompressOptions {
            target_size_kb: Some(1),
            quality_floor: 80,
            ..Default::default()
        };
        assert!(process(
            &jpeg_source(),
            &ImageFormat::Jpeg,
            &ImageFormat::Jpeg,
            &options
        )
        .is_err());
    }
    #[test]
    fn resizing_is_explicit_and_reaches_small_target() {
        let options = CompressOptions {
            target_size_kb: Some(2),
            quality_floor: 50,
            allow_resize: true,
            ..Default::default()
        };
        let output = process(
            &jpeg_source(),
            &ImageFormat::Jpeg,
            &ImageFormat::Jpeg,
            &options,
        )
        .unwrap();
        assert!(output.bytes.len() <= 2048);
        assert!(output.resized);
        assert!(output.width < 160);
    }
    #[test]
    fn lossless_webp_is_still_lossless_after_resizing() {
        let img = image::RgbaImage::from_pixel(80, 40, image::Rgba([230, 20, 20, 128]));
        let data = ::webp::Encoder::from_rgba(img.as_raw(), 80, 40)
            .encode_lossless()
            .to_vec();
        let options = CompressOptions {
            max_edge: Some(40),
            ..Default::default()
        };
        let output = process(&data, &ImageFormat::WebP, &ImageFormat::WebP, &options).unwrap();
        assert!(webp::is_lossless(&output.bytes));
        assert_eq!((output.width, output.height), (40, 20));
    }
    #[test]
    fn gif_resize_preserves_timing_and_loop() {
        let mut data = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Finite(3))
                .unwrap();
            encoder
                .encode_frames([70, 230, 410].into_iter().enumerate().map(|(n, delay)| {
                    image::Frame::from_parts(
                        image::RgbaImage::from_pixel(
                            40,
                            20,
                            image::Rgba([n as u8 * 80, 30, 40, 255]),
                        ),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    )
                }))
                .unwrap();
        }
        let options = CompressOptions {
            max_edge: Some(20),
            ..Default::default()
        };
        let output = process(&data, &ImageFormat::Gif, &ImageFormat::Gif, &options).unwrap();
        let frames = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&output.bytes))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[2].delay(), image::Delay::from_numer_denom_ms(410, 1));
        assert_eq!(frames[0].buffer().dimensions(), (20, 10));
        assert_eq!(
            ::gif::DecodeOptions::new()
                .read_info(output.bytes.as_slice())
                .unwrap()
                .repeat(),
            ::gif::Repeat::Finite(3)
        );
    }
    #[test]
    fn pixel_lossless_png_does_not_silently_resize() {
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([20, 40, 60, 255]));
        let mut data = std::io::Cursor::new(Vec::new());
        image.write_to(&mut data, image::ImageFormat::Png).unwrap();
        let options = CompressOptions {
            max_edge: Some(16),
            png_strategy: png::PngStrategy::Lossless,
            ..Default::default()
        };
        assert!(process(
            &data.into_inner(),
            &ImageFormat::Png,
            &ImageFormat::Png,
            &options
        )
        .is_err());
    }
}
