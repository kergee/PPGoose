mod api;
pub use api::{compress_bytes, EncodedImage, EncodingOptions};

pub mod avif;
pub(crate) mod file_io;
pub mod gif;
pub mod jpeg;
pub mod png;
pub mod smart;
pub mod storage;
mod target;
pub mod webp;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Decode image bytes, apply any EXIF orientation (so re-encoded pixels are
/// upright even though the EXIF metadata itself is dropped), and extract the
/// ICC colour profile if present so encoders can carry it over.
pub(crate) fn decode_with_meta(data: &[u8]) -> Result<(image::DynamicImage, Option<Vec<u8>>)> {
    use image::{ImageDecoder, ImageReader};

    let mut decoder = ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let icc = decoder.icc_profile()?;
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok((img, icc))
}

pub(crate) fn decode_oriented(data: &[u8]) -> Result<image::DynamicImage> {
    decode_with_meta(data).map(|(img, _)| img)
}

/// Detected image format.
///
/// AVIF is an output-only format: `ravif` can encode it, but there is no
/// portable decoder available (image's `avif-native` needs system dav1d),
/// so `from_path` never returns `Avif`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
    Avif,
}

impl ImageFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    /// Detect supported input from its bytes, independently of filenames.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Ok(match image::guess_format(data)? {
            image::ImageFormat::Png => Self::Png,
            image::ImageFormat::Jpeg => Self::Jpeg,
            image::ImageFormat::Gif => Self::Gif,
            image::ImageFormat::WebP => Self::WebP,
            _ => anyhow::bail!("仅支持 PNG、JPEG、GIF 和静态 WebP 输入"),
        })
    }

    #[allow(dead_code)]
    pub fn ext(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Avif => "avif",
        }
    }
}

/// When set, WebP files are converted to the specified format instead of
/// being compressed in-place. The output file extension changes accordingly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ConvertTarget {
    Png,
    Jpeg,
    Avif,
}

impl ConvertTarget {
    #[allow(dead_code)]
    pub fn ext(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Avif => "avif",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct CompressOptions {
    /// 0 = auto (uses 80), 1–100 = manual quality
    pub quality: u8,
    pub output_mode: OutputMode,
    /// Used when output_mode == Custom
    pub custom_dir: Option<String>,
    /// Appended before extension, e.g. "_min"
    pub suffix: Option<String>,
    /// If set, WebP files are converted to this format instead of compressed
    pub convert_webp_to: Option<ConvertTarget>,
    /// 极致模式: perceptual-quality-guided search for the smallest acceptable
    /// quality (JPEG/WebP outputs, only when quality is auto). Slower.
    #[serde(default)]
    pub smart_quality: bool,
    /// PNG palette policy. Detail borrows the selected high-detail ceiling.
    #[serde(default)]
    pub png_strategy: png::PngStrategy,
    /// JPEG sampling/quantization policy; balanced retains the existing default.
    #[serde(default)]
    pub jpeg_strategy: jpeg::JpegStrategy,
    #[serde(default)]
    pub target_size_kb: Option<u32>,
    #[serde(default)]
    pub max_edge: Option<u32>,
    #[serde(default)]
    pub allow_resize: bool,
    #[serde(default = "default_quality_floor")]
    pub quality_floor: u8,
}

fn default_quality_floor() -> u8 {
    40
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            quality: 0,
            output_mode: OutputMode::Overwrite,
            custom_dir: None,
            suffix: None,
            convert_webp_to: None,
            smart_quality: false,
            png_strategy: png::PngStrategy::Detail,
            jpeg_strategy: jpeg::JpegStrategy::Balanced,
            target_size_kb: None,
            max_edge: None,
            allow_resize: false,
            quality_floor: 40,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    Overwrite,
    Subfolder,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressResult {
    pub input_path: String,
    pub output_path: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub record_id: String,
    pub overwritten: bool,
    pub warning: Option<String>,
    pub resized: bool,
    pub width: u32,
    pub height: u32,
    pub target_size: Option<u64>,
    pub kept_original: bool,
}

/// Compress (or convert) a single file and write the result.
pub fn compress_file(
    input: &Path,
    opts: &CompressOptions,
    store: &storage::Store,
) -> Result<CompressResult> {
    validate_options(opts)?;
    let canonical = input.canonicalize()?;
    let input = canonical.as_path();
    let fmt = ImageFormat::from_path(input)
        .ok_or_else(|| anyhow::anyhow!("Unsupported: {:?}", input.extension()))?;

    let original_bytes = std::fs::read(input)?;
    let original_size = original_bytes.len() as u64;
    let processed = encode_validated(&original_bytes, &fmt, opts)?;
    let final_bytes = processed.bytes;
    let output_fmt = processed.output_format;
    let kept_original = processed.kept_original;

    let desired = resolve_output_path(input, opts, &fmt, &output_fmt)?;
    let replacing = desired == input && opts.output_mode == OutputMode::Overwrite;
    if replacing && std::fs::metadata(input)?.permissions().readonly() {
        anyhow::bail!("原文件为只读，未覆盖。请选择另存目录");
    }
    let committed = store.commit(input, &desired, &original_bytes, &final_bytes, replacing)?;
    let output_path = PathBuf::from(&committed.record.output_path);

    Ok(CompressResult {
        input_path: input.to_string_lossy().into_owned(),
        output_path: output_path.to_string_lossy().into_owned(),
        original_size,
        compressed_size: final_bytes.len() as u64,
        record_id: committed.record.id,
        overwritten: committed.record.overwritten,
        warning: committed.warning,
        resized: processed.resized,
        width: processed.width,
        height: processed.height,
        target_size: opts.target_size_kb.map(|kb| u64::from(kb) * 1024),
        kept_original,
    })
}

fn encode_validated(
    original: &[u8],
    source: &ImageFormat,
    opts: &CompressOptions,
) -> Result<EncodedImage> {
    validate_options(opts)?;
    validate_format(original, source)?;
    if *source == ImageFormat::WebP && webp::is_animated(original) {
        anyhow::bail!("暂不支持动画 WebP，已跳过以避免丢失动画帧");
    }
    let output_format = match (source, &opts.convert_webp_to) {
        (ImageFormat::WebP, Some(ConvertTarget::Png)) => ImageFormat::Png,
        (ImageFormat::WebP, Some(ConvertTarget::Jpeg)) => ImageFormat::Jpeg,
        (ImageFormat::WebP, Some(ConvertTarget::Avif)) => ImageFormat::Avif,
        _ => source.clone(),
    };
    let processed = target::process(original, source, &output_format, opts)?;
    let bytes = if source == &output_format
        && !processed.resized
        && processed.bytes.len() >= original.len()
    {
        original.to_vec()
    } else {
        processed.bytes
    };
    validate_format(&bytes, &output_format)?;
    let target_size = opts.target_size_kb.map(|kb| u64::from(kb) * 1024);
    anyhow::ensure!(
        target_size.is_none_or(|cap| bytes.len() as u64 <= cap),
        "压缩输出超过目标大小，未写入输出"
    );
    Ok(EncodedImage {
        kept_original: bytes == original,
        original_size: original.len() as u64,
        compressed_size: bytes.len() as u64,
        bytes,
        output_format,
        resized: processed.resized,
        width: processed.width,
        height: processed.height,
        target_size,
    })
}

fn resolve_output_path(
    input: &Path,
    opts: &CompressOptions,
    source: &ImageFormat,
    target: &ImageFormat,
) -> Result<PathBuf> {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let suffix = opts.suffix.as_deref().unwrap_or("");
    let filename = format!("{}{}.{}", stem, suffix, target.ext());

    let output = match &opts.output_mode {
        OutputMode::Overwrite => {
            if source == target && suffix.is_empty() {
                return Ok(input.to_path_buf());
            }
            // If the extension changes (e.g. webp→avif), always write next to original
            input.with_file_name(&filename)
        }
        OutputMode::Subfolder => {
            let parent = input.parent().unwrap_or(Path::new("."));
            parent.join("compressed").join(&filename)
        }
        OutputMode::Custom => {
            let dir = opts
                .custom_dir
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("custom_dir required for Custom mode"))?;
            PathBuf::from(dir).join(&filename)
        }
    };

    Ok(output)
}

fn validate_options(opts: &CompressOptions) -> Result<()> {
    anyhow::ensure!(opts.quality <= 100, "质量必须在 0–100 之间");
    anyhow::ensure!(
        (1..=100).contains(&opts.quality_floor),
        "最低质量必须在 1–100 之间"
    );
    if let Some(kb) = opts.target_size_kb {
        anyhow::ensure!(
            (1..=102400).contains(&kb),
            "目标大小必须在 1–102400 KB 之间"
        );
        anyhow::ensure!(!opts.smart_quality, "目标大小模式与极致模式请选其一");
        anyhow::ensure!(
            opts.quality == 0 || opts.quality_floor <= opts.quality,
            "最低质量不能高于手动质量上限"
        );
    }
    if let Some(edge) = opts.max_edge {
        anyhow::ensure!((1..=30000).contains(&edge), "最长边必须在 1–30000 像素之间");
    }
    if let Some(suffix) = &opts.suffix {
        anyhow::ensure!(
            !suffix
                .chars()
                .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)),
            "文件名后缀包含无效字符"
        );
    }
    if opts.output_mode == OutputMode::Custom {
        let dir = opts
            .custom_dir
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("请选择输出目录"))?;
        anyhow::ensure!(Path::new(dir).is_absolute(), "输出目录必须是绝对路径");
    }
    Ok(())
}

fn validate_format(data: &[u8], expected: &ImageFormat) -> Result<()> {
    let guessed = image::guess_format(data)?;
    let correct = matches!(
        (expected, guessed),
        (ImageFormat::Png, image::ImageFormat::Png)
            | (ImageFormat::Jpeg, image::ImageFormat::Jpeg)
            | (ImageFormat::Gif, image::ImageFormat::Gif)
            | (ImageFormat::WebP, image::ImageFormat::WebP)
            | (ImageFormat::Avif, image::ImageFormat::Avif)
    );
    anyhow::ensure!(correct, "文件内容与预期格式不一致，未写入输出");
    if *expected == ImageFormat::WebP {
        anyhow::ensure!(data.len() >= 12, "WebP RIFF 头不完整");
        let size = u32::from_le_bytes(data[4..8].try_into()?);
        anyhow::ensure!(
            u64::from(size) + 8 == data.len() as u64,
            "WebP RIFF 长度与文件内容不一致"
        );
    }
    if *expected != ImageFormat::Avif {
        image::load_from_memory(data)?;
    }
    Ok(())
}

#[cfg(test)]
mod io_tests {
    use super::*;
    fn compress_file(input: &Path, opts: &CompressOptions) -> Result<CompressResult> {
        let data = tempfile::tempdir().unwrap();
        super::compress_file(input, opts, &storage::Store::new(data.path().to_owned()))
    }
    #[test]
    fn webp_conversion_writes_a_real_lossless_png() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("alpha.webp");
        let image = image::RgbaImage::from_fn(16, 16, |x, y| {
            image::Rgba([x as u8 * 12, y as u8 * 12, 100, if x < 8 { 0 } else { 128 }])
        });
        let bytes = ::webp::Encoder::from_rgba(image.as_raw(), 16, 16)
            .encode_lossless()
            .to_vec();
        std::fs::write(&source, &bytes).unwrap();
        let options = CompressOptions {
            convert_webp_to: Some(ConvertTarget::Png),
            ..Default::default()
        };
        let result = compress_file(&source, &options).unwrap();
        let output = std::fs::read(&result.output_path).unwrap();
        assert_eq!(Path::new(&result.output_path).extension().unwrap(), "png");
        assert!(output.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            image::guess_format(&output).unwrap(),
            image::ImageFormat::Png
        );
        assert_eq!(
            image::load_from_memory(&output).unwrap().into_rgba8(),
            image::load_from_memory(&bytes).unwrap().into_rgba8()
        );
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
    }

    #[test]
    fn overwrite_preserves_jpeg_extension_and_custom_same_folder_is_separate() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("image.jpeg");
        let image = image::RgbImage::from_pixel(32, 32, image::Rgb([230, 30, 30]));
        image.save(&source).unwrap();
        let result = compress_file(&source, &Default::default()).unwrap();
        assert_eq!(
            Path::new(&result.output_path),
            source.canonicalize().unwrap()
        );
        let options = CompressOptions {
            output_mode: OutputMode::Custom,
            custom_dir: Some(dir.path().to_string_lossy().into_owned()),
            ..Default::default()
        };
        let other = compress_file(&source, &options).unwrap();
        assert_ne!(
            Path::new(&other.output_path),
            source.canonicalize().unwrap()
        );
    }

    #[test]
    fn invalid_suffix_does_not_write_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let options = CompressOptions {
            suffix: Some("../escape".into()),
            ..Default::default()
        };
        assert!(compress_file(&dir.path().join("test.png"), &options).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn unreachable_target_never_overwrites_or_creates_a_record() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("noise.jpg");
        let mut seed = 7u32;
        let image = image::RgbImage::from_fn(160, 120, |_, _| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let bytes = seed.to_le_bytes();
            image::Rgb([bytes[0], bytes[1], bytes[2]])
        });
        image.save(&source).unwrap();
        let original = std::fs::read(&source).unwrap();
        let store = storage::Store::new(dir.path().join("data"));
        let options = CompressOptions {
            target_size_kb: Some(1),
            quality_floor: 80,
            ..Default::default()
        };
        assert!(super::compress_file(&source, &options, &store).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(!store.root.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn resized_overwrite_preview_and_restart_restore_are_consistent() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("image.jpg");
        image::RgbImage::from_fn(160, 80, |x, y| image::Rgb([x as u8, y as u8, 50]))
            .save(&source)
            .unwrap();
        let original = std::fs::read(&source).unwrap();
        let store = storage::Store::new(dir.path().join("data"));
        let options = CompressOptions {
            target_size_kb: Some(10),
            max_edge: Some(80),
            ..Default::default()
        };
        let result = super::compress_file(&source, &options, &store).unwrap();
        assert!(result.resized && result.overwritten && result.compressed_size <= 10 * 1024);
        assert_eq!((result.width, result.height), (80, 40));
        let preview = store.preview(&result.record_id).unwrap();
        assert_eq!(std::fs::read(&preview.before_path).unwrap(), original);
        assert_eq!(
            image::load_from_memory(&std::fs::read(&preview.after_path).unwrap())
                .unwrap()
                .width(),
            80
        );
        storage::Store::new(store.root.clone())
            .restore(&result.record_id)
            .unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), original);
        // Open comparisons retain an immutable snapshot even after restoration.
        assert_eq!(
            image::load_from_memory(&std::fs::read(&preview.after_path).unwrap())
                .unwrap()
                .width(),
            80
        );
    }

    #[test]
    fn conversion_preserves_existing_png_and_composites_jpeg_on_white() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("alpha.webp");
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([230, 10, 30, 0]));
        let original = ::webp::Encoder::from_rgba(image.as_raw(), 32, 32)
            .encode_lossless()
            .to_vec();
        std::fs::write(&source, &original).unwrap();
        let existing = dir.path().join("alpha.png");
        std::fs::write(&existing, b"user existing file").unwrap();
        let options = CompressOptions {
            convert_webp_to: Some(ConvertTarget::Png),
            ..Default::default()
        };
        let png = compress_file(&source, &options).unwrap();
        assert_ne!(Path::new(&png.output_path), existing);
        assert_eq!(std::fs::read(&existing).unwrap(), b"user existing file");
        assert_eq!(
            image::guess_format(&std::fs::read(png.output_path).unwrap()).unwrap(),
            image::ImageFormat::Png
        );
        let jpeg = compress_file(
            &source,
            &CompressOptions {
                convert_webp_to: Some(ConvertTarget::Jpeg),
                ..Default::default()
            },
        )
        .unwrap();
        let pixel = image::load_from_memory(&std::fs::read(jpeg.output_path).unwrap())
            .unwrap()
            .into_rgb8();
        assert!(pixel.pixels().all(|p| p.0.iter().all(|&v| v >= 250)));
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
}
