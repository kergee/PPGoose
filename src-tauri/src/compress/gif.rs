use anyhow::Result;
use image::AnimationDecoder;
use rgb::FromSlice;

/// Compress GIF by re-encoding all frames with gifski.
///
/// gifski builds palettes across frames (via imagequant) and does inter-frame
/// optimisation, giving far better quality and size than per-frame NeuQuant.
/// The original loop count is preserved.
pub fn compress(data: &[u8], quality: u8) -> Result<Vec<u8>> {
    // image's GifDecoder doesn't expose the loop-count extension, so read it
    // with the gif crate directly. Default to Infinite (the common case) if
    // the header can't be parsed.
    let repeat = gif::DecodeOptions::new()
        .read_info(std::io::Cursor::new(data))
        .map(|d| d.repeat())
        .unwrap_or(gif::Repeat::Infinite);

    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data))?;
    let frames = decoder.into_frames().collect_frames()?;
    if frames.is_empty() {
        anyhow::bail!("GIF has no frames");
    }

    let settings = gifski::Settings {
        quality: quality.clamp(1, 100),
        repeat: match repeat {
            gif::Repeat::Finite(n) => gifski::Repeat::Finite(n),
            gif::Repeat::Infinite  => gifski::Repeat::Infinite,
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
    for (index, frame) in frames.into_iter().enumerate() {
        let (numer_ms, denom_ms) = frame.delay().numer_denom_ms();
        let delay_secs = numer_ms as f64 / denom_ms.max(1) as f64 / 1000.0;

        let buffer = frame.into_buffer();
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
                    0, 0, delay,
                ),
                Frame::from_parts(
                    RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255])),
                    0, 0, delay,
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
}
