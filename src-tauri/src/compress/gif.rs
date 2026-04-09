use anyhow::Result;
use image::AnimationDecoder;

/// Compress GIF by decoding frames and re-encoding with reduced palette.
///
/// GIF is limited to 256 colours per frame; the main compression lever
/// is frame deduplication and per-frame colour reduction.
pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};

    let cursor = std::io::Cursor::new(data);
    let decoder = GifDecoder::new(cursor)?;
    let frames = decoder.into_frames().collect_frames()?;

    let mut output = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut output, 30);
        encoder.set_repeat(Repeat::Infinite)?;
        encoder.encode_frames(frames)?;
    }

    Ok(output)
}
