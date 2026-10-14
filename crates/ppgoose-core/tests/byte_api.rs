use ppgoose_core::{compress_bytes, EncodingOptions, ImageFormat};

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(64, 32, |x, y| {
        image::Rgba([x as u8 * 3, y as u8 * 7, 120, 255])
    });
    let mut input = std::io::Cursor::new(Vec::new());
    img.write_to(&mut input, image::ImageFormat::Png).unwrap();
    input.into_inner()
}

#[test]
fn bytes_are_detected_by_content_and_same_format_never_inflates() {
    let input = png();
    let result = compress_bytes(&input, &EncodingOptions::default()).unwrap();
    assert_eq!(result.output_format, ImageFormat::Png);
    assert!(result.bytes.len() <= input.len());
    assert_eq!(result.compressed_size, result.bytes.len() as u64);
    assert_eq!((result.width, result.height), (64, 32));
}

#[test]
fn byte_conversion_keeps_pixels_and_reports_the_real_format() {
    let pixels = image::RgbaImage::from_pixel(16, 8, image::Rgba([120, 200, 30, 128]));
    let input = webp::Encoder::from_rgba(pixels.as_raw(), 16, 8)
        .encode_lossless()
        .to_vec();
    let result = compress_bytes(
        &input,
        &EncodingOptions {
            convert_webp_to: Some(ppgoose_core::ConvertTarget::Png),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.output_format, ImageFormat::Png);
    assert!(!result.kept_original);
    assert_eq!(
        image::load_from_memory(&result.bytes).unwrap().into_rgba8(),
        pixels
    );
}

#[test]
fn byte_api_rejects_invalid_data_options_and_unsupported_formats() {
    assert!(compress_bytes(b"not an image", &Default::default()).is_err());
    assert!(compress_bytes(
        &png(),
        &EncodingOptions {
            quality: 101,
            ..Default::default()
        }
    )
    .is_err());
    assert!(compress_bytes(
        &png(),
        &EncodingOptions {
            target_size_kb: Some(1),
            smart_quality: true,
            ..Default::default()
        }
    )
    .is_err());
    let mut bmp = std::io::Cursor::new(Vec::new());
    // BMP is deliberately absent from the supported byte API.
    bmp.get_mut().extend_from_slice(b"BM\0\0\0\0");
    assert!(compress_bytes(bmp.get_ref(), &Default::default()).is_err());
    assert!(serde_json::from_str::<EncodingOptions>(r#"{"qualty": 10}"#).is_err());
}

#[test]
fn options_default_and_legacy_desktop_json_remain_compatible() {
    let encoding: EncodingOptions = serde_json::from_str("{}").unwrap();
    assert_eq!(encoding.quality, 0);
    assert_eq!(
        encoding.jpeg_strategy,
        ppgoose_core::jpeg::JpegStrategy::Balanced
    );
    let legacy: ppgoose_core::CompressOptions = serde_json::from_str(
        r#"{"quality":0,"outputMode":"overwrite","smartQuality":false,"pngStrategy":"detail"}"#,
    )
    .unwrap();
    assert_eq!(
        legacy.jpeg_strategy,
        ppgoose_core::jpeg::JpegStrategy::Balanced
    );
}
