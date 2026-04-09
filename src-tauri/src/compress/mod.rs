pub mod avif;
pub mod gif;
pub mod jpeg;
pub mod png;
pub mod webp;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Detected image format
#[derive(Debug, Clone, PartialEq)]
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
            "png"          => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif"          => Some(Self::Gif),
            "webp"         => Some(Self::WebP),
            "avif"         => Some(Self::Avif),
            _              => None,
        }
    }

    #[allow(dead_code)]
    pub fn ext(&self) -> &'static str {
        match self {
            Self::Png  => "png",
            Self::Jpeg => "jpg",
            Self::Gif  => "gif",
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
            Self::Png  => "png",
            Self::Jpeg => "jpg",
            Self::Avif => "avif",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            quality: 0,
            output_mode: OutputMode::Overwrite,
            custom_dir: None,
            suffix: None,
            convert_webp_to: None,
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
}

/// Compress (or convert) a single file and write the result.
pub fn compress_file(input: &Path, opts: &CompressOptions) -> Result<CompressResult> {
    let fmt = ImageFormat::from_path(input)
        .ok_or_else(|| anyhow::anyhow!("Unsupported: {:?}", input.extension()))?;

    let original_bytes = std::fs::read(input)?;
    let original_size = original_bytes.len() as u64;

    let quality = if opts.quality == 0 { 80 } else { opts.quality };

    // ── Determine output format ──────────────────────────────────────────
    // WebP can be converted to a different format; everything else stays as-is.
    let output_fmt: ImageFormat = if fmt == ImageFormat::WebP {
        match &opts.convert_webp_to {
            Some(ConvertTarget::Png)  => ImageFormat::Png,
            Some(ConvertTarget::Jpeg) => ImageFormat::Jpeg,
            Some(ConvertTarget::Avif) => ImageFormat::Avif,
            None                      => ImageFormat::WebP,
        }
    } else {
        fmt.clone()
    };

    // ── Compress / convert ───────────────────────────────────────────────
    let compressed = match &output_fmt {
        ImageFormat::Png  => png::compress(&original_bytes, quality)?,
        ImageFormat::Jpeg => jpeg::compress(&original_bytes, quality)?,
        ImageFormat::Gif  => gif::compress(&original_bytes)?,
        ImageFormat::WebP => webp::compress(&original_bytes, quality)?,
        ImageFormat::Avif => avif::compress(&original_bytes, quality)?,
    };

    // Never inflate: keep original bytes if compressed is larger
    // (only applies when format is unchanged)
    let final_bytes = if output_fmt == fmt && compressed.len() >= original_bytes.len() {
        original_bytes.clone()
    } else {
        compressed
    };

    let output_path = resolve_output_path(input, opts, output_fmt.ext())?;
    std::fs::create_dir_all(output_path.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(&output_path, &final_bytes)?;

    Ok(CompressResult {
        input_path: input.to_string_lossy().into_owned(),
        output_path: output_path.to_string_lossy().into_owned(),
        original_size,
        compressed_size: final_bytes.len() as u64,
    })
}

fn resolve_output_path(input: &Path, opts: &CompressOptions, ext: &str) -> Result<PathBuf> {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let suffix = opts.suffix.as_deref().unwrap_or("");
    let filename = format!("{}{}.{}", stem, suffix, ext);

    let output = match &opts.output_mode {
        OutputMode::Overwrite => {
            // If the extension changes (e.g. webp→avif), always write next to original
            input.with_file_name(&filename)
        }
        OutputMode::Subfolder => {
            let parent = input.parent().unwrap_or(Path::new("."));
            parent.join("compressed").join(&filename)
        }
        OutputMode::Custom => {
            let dir = opts.custom_dir.as_deref()
                .ok_or_else(|| anyhow::anyhow!("custom_dir required for Custom mode"))?;
            PathBuf::from(dir).join(&filename)
        }
    };

    Ok(output)
}
