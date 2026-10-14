use anyhow::Result;
use image::AnimationDecoder;
use rgb::FromSlice;
use std::borrow::Cow;
use std::collections::HashMap;

/// Compare a lossless palette/delta candidate with the existing gifski path.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data))?;
    let frames = decoder.into_frames().collect_frames()?;
    if frames.is_empty() {
        anyhow::bail!("GIF has no frames");
    }
    let delta = lossless_delta(data, &frames)?;
    let encoded = encode_gifski(data, quality, &frames).map(|candidate| {
        // gifski may infer the last delay or merge equal frames. Preserve the
        // source timeline when that changes the original animation contract.
        if same_timeline(data, &candidate).unwrap_or(false) {
            candidate
        } else {
            data.to_vec()
        }
    });
    match (delta, encoded) {
        (Some(delta), Ok(encoded)) => Ok(if delta.len() < encoded.len() {
            delta
        } else {
            encoded
        }),
        (Some(delta), Err(_)) => Ok(delta),
        (None, encoded) => encoded,
    }
}

fn same_timeline(source: &[u8], candidate: &[u8]) -> Result<bool> {
    fn timeline(data: &[u8]) -> Result<(u16, u16, gif::Repeat, Vec<u16>)> {
        let mut reader = gif::DecodeOptions::new().read_info(std::io::Cursor::new(data))?;
        let header = (reader.width(), reader.height(), reader.repeat());
        let mut delays = Vec::new();
        while let Some(frame) = reader.read_next_frame()? {
            delays.push(frame.delay);
        }
        Ok((header.0, header.1, header.2, delays))
    }
    Ok(timeline(source)? == timeline(candidate)?)
}

/// Opaque animations with <=256 colours can be encoded without quantization.
/// A shared palette and changed rectangles avoid re-encoding every full frame.
/// Transparent/disposal-sensitive inputs use gifski's established decoder path.
fn lossless_delta(data: &[u8], frames: &[image::Frame]) -> Result<Option<Vec<u8>>> {
    let first = frames[0].buffer();
    let (width, height) = first.dimensions();
    let (w, h) = (u16::try_from(width)?, u16::try_from(height)?);
    let mut indices: HashMap<[u8; 3], u8> = HashMap::new();
    let mut palette = Vec::new();
    for frame in frames {
        if frame.buffer().dimensions() != (width, height) {
            return Ok(None);
        }
        for pixel in frame.buffer().pixels() {
            if pixel[3] != 255 {
                return Ok(None);
            }
            let rgb = [pixel[0], pixel[1], pixel[2]];
            if !indices.contains_key(&rgb) {
                if indices.len() == 256 {
                    return Ok(None);
                }
                indices.insert(rgb, indices.len() as u8);
                palette.extend_from_slice(&rgb);
            }
        }
    }
    // An extra palette entry is reserved only for unchanged pixels in patches.
    let transparent = if indices.len() < 256 {
        let index = indices.len() as u8;
        palette.extend_from_slice(&[0, 0, 0]);
        Some(index)
    } else {
        None
    };
    let repeat = gif::DecodeOptions::new()
        .read_info(std::io::Cursor::new(data))?
        .repeat();
    let mut output = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut output, w, h, &palette)?;
        encoder.set_repeat(repeat)?;
        let mut previous: Option<&image::RgbaImage> = None;
        for source in frames {
            let pixels = source.buffer();
            let (mut left, mut top, mut right, mut bottom) = (0, 0, width, height);
            if let Some(prev) = previous {
                (left, top, right, bottom) = (width, height, 0, 0);
                for y in 0..height {
                    for x in 0..width {
                        if pixels.get_pixel(x, y) != prev.get_pixel(x, y) {
                            left = left.min(x);
                            top = top.min(y);
                            right = right.max(x + 1);
                            bottom = bottom.max(y + 1);
                        }
                    }
                }
                if right == 0 {
                    (left, top, right, bottom) = (0, 0, 1, 1);
                }
            }
            let mut patch = Vec::with_capacity(((right - left) * (bottom - top)) as usize);
            for y in top..bottom {
                for x in left..right {
                    let pixel = pixels.get_pixel(x, y);
                    let unchanged = previous.is_some_and(|p| p.get_pixel(x, y) == pixel);
                    patch.push(if unchanged && transparent.is_some() {
                        transparent.unwrap()
                    } else {
                        indices[&[pixel[0], pixel[1], pixel[2]]]
                    });
                }
            }
            // A tiny moving region may use only 2–3 colours even when the
            // whole UI uses hundreds. Compact its indices and palette so its
            // LZW codes don't inherit the global palette's 8-bit starting size.
            let mut local_palette = None;
            let mut patch_transparent = transparent;
            if previous.is_some() && patch.len() >= 128 && indices.len() > 16 {
                let mut mapping = [None; 256];
                let mut local = Vec::new();
                let mut remapped = Vec::with_capacity(patch.len());
                for &index in &patch {
                    let mapped = *mapping[index as usize].get_or_insert_with(|| {
                        let id = (local.len() / 3) as u8;
                        local.extend_from_slice(
                            &palette[index as usize * 3..index as usize * 3 + 3],
                        );
                        id
                    });
                    remapped.push(mapped);
                }
                if local.len() / 3 <= 16 {
                    patch_transparent = transparent.and_then(|id| mapping[id as usize]);
                    local_palette = Some(local);
                    patch = remapped;
                }
            }
            let (numer, denom) = source.delay().numer_denom_ms();
            let delay = u16::try_from(u64::from(numer) / (u64::from(denom.max(1)) * 10))?;
            let frame = gif::Frame {
                left: left as u16,
                top: top as u16,
                width: (right - left) as u16,
                height: (bottom - top) as u16,
                delay,
                dispose: gif::DisposalMethod::Keep,
                transparent: patch_transparent,
                palette: local_palette,
                buffer: Cow::Owned(patch),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame)?;
            previous = Some(pixels);
        }
    }
    Ok(Some(output))
}

/// Compress GIF by re-encoding all frames with gifski.
///
/// gifski builds palettes across frames (via imagequant) and does inter-frame
/// optimisation, giving far better quality and size than per-frame NeuQuant.
/// The original loop count is preserved.
fn encode_gifski(data: &[u8], quality: u8, frames: &[image::Frame]) -> Result<Vec<u8>> {
    // image's GifDecoder doesn't expose the loop-count extension, so read it
    // with the gif crate directly. Default to Infinite (the common case) if
    // the header can't be parsed.
    let repeat = gif::DecodeOptions::new()
        .read_info(std::io::Cursor::new(data))
        .map(|d| d.repeat())
        .unwrap_or(gif::Repeat::Infinite);

    if frames.is_empty() {
        anyhow::bail!("GIF has no frames");
    }

    let settings = gifski::Settings {
        quality: quality.clamp(1, 100),
        repeat: match repeat {
            gif::Repeat::Finite(n) => gifski::Repeat::Finite(n),
            gif::Repeat::Infinite => gifski::Repeat::Infinite,
        },
        ..gifski::Settings::default()
    };

    let (collector, writer) = gifski::new(settings)?;

    // gifski streams frames through a bounded channel: the writer must run
    // concurrently with the collector or large GIFs would deadlock.
    let write_thread = std::thread::spawn(move || -> Result<Vec<u8>> {
        let mut out = Vec::new();
        writer.write(&mut out, &mut gifski::progress::NoProgress {})?;
        Ok(out)
    });

    let mut timestamp = 0.0f64;
    for (index, frame) in frames.iter().enumerate() {
        let (numer_ms, denom_ms) = frame.delay().numer_denom_ms();
        let delay_secs = numer_ms as f64 / denom_ms.max(1) as f64 / 1000.0;

        let buffer = frame.buffer();
        let (width, height) = (buffer.width() as usize, buffer.height() as usize);
        let pixels = buffer.as_raw().as_rgba().to_vec();

        collector.add_frame_rgba(index, imgref::ImgVec::new(pixels, width, height), timestamp)?;
        timestamp += delay_secs;
    }
    drop(collector); // signals end-of-input to the writer

    write_thread
        .join()
        .map_err(|_| anyhow::anyhow!("gifski writer thread panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_frame_gif(repeat: image::codecs::gif::Repeat) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame, Rgba, RgbaImage};

        let mut src = Vec::new();
        {
            let mut enc = GifEncoder::new_with_speed(&mut src, 30);
            enc.set_repeat(repeat).unwrap();
            let delay = Delay::from_numer_denom_ms(100, 1);
            enc.encode_frames(vec![
                Frame::from_parts(
                    RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255])),
                    0,
                    0,
                    delay,
                ),
                Frame::from_parts(
                    RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255])),
                    0,
                    0,
                    delay,
                ),
            ])
            .unwrap();
        }
        src
    }

    #[test]
    fn preserves_finite_loop_count() {
        let src = two_frame_gif(image::codecs::gif::Repeat::Finite(3));
        let out = compress(&src, 80).unwrap();
        let repeat = gif::DecodeOptions::new()
            .read_info(std::io::Cursor::new(out.as_slice()))
            .unwrap()
            .repeat();
        assert_eq!(repeat, gif::Repeat::Finite(3));
    }

    #[test]
    fn keeps_all_frames() {
        let src = two_frame_gif(image::codecs::gif::Repeat::Infinite);
        let out = compress(&src, 80).unwrap();
        let decoder =
            image::codecs::gif::GifDecoder::new(std::io::Cursor::new(out.as_slice())).unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn delta_preserves_pixels_variable_delays_and_loop() {
        use image::{Delay, Frame, Rgba, RgbaImage};
        let mut source = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut source);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Finite(4))
                .unwrap();
            let frames = [70, 230, 410].into_iter().enumerate().map(|(i, delay)| {
                let mut image = RgbaImage::from_pixel(80, 60, Rgba([250, 250, 250, 255]));
                for y in 20..30 {
                    for x in (i * 12)..(i * 12 + 10) {
                        image.put_pixel(x as u32, y, Rgba([0, 0, 255, 255]));
                    }
                }
                Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(delay, 1))
            });
            encoder.encode_frames(frames).unwrap();
        }
        let decoder =
            image::codecs::gif::GifDecoder::new(std::io::Cursor::new(source.as_slice())).unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        let delta = lossless_delta(&source, &frames).unwrap().unwrap();
        assert!(same_timeline(&source, &delta).unwrap());
        let decoded = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(delta.as_slice()))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        for (a, b) in frames.iter().zip(decoded.iter()) {
            assert_eq!(a.buffer(), b.buffer());
        }
        let result = compress(&source, 80).unwrap();
        assert!(same_timeline(&source, &result).unwrap());
    }

    #[test]
    fn transparency_uses_existing_encoder_path() {
        use image::{Delay, Frame, Rgba, RgbaImage};
        let mut source = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut source);
            encoder
                .encode_frames([Frame::from_parts(
                    RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0])),
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                )])
                .unwrap();
        }
        let frames = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(source.as_slice()))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert!(lossless_delta(&source, &frames).unwrap().is_none());
        let output = compress(&source, 80).unwrap();
        assert!(same_timeline(&source, &output).unwrap());
        let decoded = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(output.as_slice()))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert_eq!(decoded[0].buffer(), frames[0].buffer());
    }

    #[test]
    fn repeated_frames_preserve_each_delay() {
        use image::{Delay, Frame, Rgba, RgbaImage};
        let mut source = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut source);
            encoder
                .encode_frames([100, 400].into_iter().map(|delay| {
                    Frame::from_parts(
                        RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255])),
                        0,
                        0,
                        Delay::from_numer_denom_ms(delay, 1),
                    )
                }))
                .unwrap();
        }
        let output = compress(&source, 80).unwrap();
        assert!(same_timeline(&source, &output).unwrap());
    }
}
