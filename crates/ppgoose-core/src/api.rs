use crate::{jpeg::JpegStrategy, png::PngStrategy, CompressOptions, ConvertTarget, ImageFormat};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Encoding policy without any filesystem, UI or storage options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct EncodingOptions {
    pub quality: u8,
    pub convert_webp_to: Option<ConvertTarget>,
    pub smart_quality: bool,
    pub png_strategy: PngStrategy,
    pub jpeg_strategy: JpegStrategy,
    pub target_size_kb: Option<u32>,
    pub max_edge: Option<u32>,
    pub allow_resize: bool,
    pub quality_floor: u8,
}

impl Default for EncodingOptions {
    fn default() -> Self {
        let options = CompressOptions::default();
        Self {
            quality: options.quality,
            convert_webp_to: options.convert_webp_to,
            smart_quality: options.smart_quality,
            png_strategy: options.png_strategy,
            jpeg_strategy: options.jpeg_strategy,
            target_size_kb: options.target_size_kb,
            max_edge: options.max_edge,
            allow_resize: options.allow_resize,
            quality_floor: options.quality_floor,
        }
    }
}

#[derive(Debug)]
pub struct EncodedImage {
    pub bytes: Vec<u8>,
    pub output_format: ImageFormat,
    pub original_size: u64,
    pub compressed_size: u64,
    pub width: u32,
    pub height: u32,
    pub resized: bool,
    pub kept_original: bool,
    pub target_size: Option<u64>,
}

/// Compress in memory. The format is detected from the content. No files,
/// backups, cache, history, or directories are created, even on failure.
pub fn compress_bytes(data: &[u8], options: &EncodingOptions) -> Result<EncodedImage> {
    let source = ImageFormat::from_bytes(data)?;
    let options = CompressOptions {
        quality: options.quality,
        convert_webp_to: options.convert_webp_to.clone(),
        smart_quality: options.smart_quality,
        png_strategy: options.png_strategy,
        jpeg_strategy: options.jpeg_strategy,
        target_size_kb: options.target_size_kb,
        max_edge: options.max_edge,
        allow_resize: options.allow_resize,
        quality_floor: options.quality_floor,
        ..Default::default()
    };
    crate::encode_validated(data, &source, &options)
}
