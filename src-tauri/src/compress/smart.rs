use anyhow::Result;
use rgb::FromSlice;

/// DSSIM score the search must stay under. ~0.001 is "visually excellent";
/// 0.0015 keeps results indistinguishable at normal viewing while allowing
/// noticeably lower encoder qualities than a flat q80.
const DSSIM_TARGET: f64 = 0.0015;

const Q_MIN: u8 = 40;
const Q_MAX: u8 = 95;

/// 极致模式: binary-search the encoder quality for the smallest output whose
/// perceptual difference (DSSIM) from the source stays under `DSSIM_TARGET`.
///
/// `encode` is called with candidate qualities (~6 times); its output must be
/// decodable by the `image` crate. If even `Q_MAX` misses the target (already
/// heavily compressed sources), the `Q_MAX` encode is returned.
pub fn search(original: &[u8], encode: impl Fn(u8) -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    let src = super::decode_oriented(original)?.into_rgba8();
    let (width, height) = (src.width() as usize, src.height() as usize);

    let attr = dssim_core::Dssim::new();
    let reference = attr
        .create_image_rgba(src.as_raw().as_rgba(), width, height)
        .ok_or_else(|| anyhow::anyhow!("dssim: failed to build reference image"))?;

    let mut lo = Q_MIN;
    let mut hi = Q_MAX;
    let mut best: Option<Vec<u8>> = None;

    while lo <= hi {
        let q = lo + (hi - lo) / 2;
        let candidate = encode(q)?;

        let decoded = image::load_from_memory(&candidate)?.into_rgba8();
        let candidate_img = attr
            .create_image_rgba(decoded.as_raw().as_rgba(), width, height)
            .ok_or_else(|| anyhow::anyhow!("dssim: failed to build candidate image"))?;
        let (score, _) = attr.compare(&reference, candidate_img);

        if f64::from(score) <= DSSIM_TARGET {
            // Good enough — try lower quality for a smaller file
            best = Some(candidate);
            hi = q - 1;
        } else {
            lo = q + 1;
        }
    }

    match best {
        Some(bytes) => Ok(bytes),
        None => encode(Q_MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_quality_under_target_for_smooth_image() {
        // Smooth gradient: low qualities should already pass the DSSIM target
        let img = image::RgbImage::from_fn(64, 64, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 4) as u8, 128])
        });
        let mut src = std::io::Cursor::new(Vec::new());
        img.write_to(&mut src, image::ImageFormat::Png).unwrap();
        let src = src.into_inner();

        let out = search(&src, |q| crate::compress::jpeg::compress(&src, q)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));

        // Must not be worse than a flat q95 encode
        let q95 = crate::compress::jpeg::compress(&src, 95).unwrap();
        assert!(out.len() <= q95.len());
    }
}
