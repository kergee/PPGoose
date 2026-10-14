use ppgoose_core::{compress_bytes, EncodingOptions};

#[test]
fn png_resize_retains_gamma_chromaticities_and_srgb() {
    for srgb in [false, true] {
        let mut input = Vec::new();
        let mut info = png::Info::with_size(64, 32);
        info.color_type = png::ColorType::Rgba;
        info.bit_depth = png::BitDepth::Eight;
        info.source_gamma = Some(png::ScaledFloat::new(0.45455));
        info.source_chromaticities = Some(png::SourceChromaticities::new(
            (0.3127, 0.3290),
            (0.64, 0.33),
            (0.30, 0.60),
            (0.15, 0.06),
        ));
        if srgb {
            info.srgb = Some(png::SrgbRenderingIntent::Perceptual);
        }
        {
            png::Encoder::with_info(&mut input, info)
                .unwrap()
                .write_header()
                .unwrap()
                .write_image_data(&[130, 60, 40, 255].repeat(64 * 32))
                .unwrap();
        }
        let result = compress_bytes(
            &input,
            &EncodingOptions {
                max_edge: Some(32),
                ..Default::default()
            },
        )
        .unwrap();
        let before = png::Decoder::new(input.as_slice()).read_info().unwrap();
        let after = png::Decoder::new(result.bytes.as_slice())
            .read_info()
            .unwrap();
        assert_eq!(before.info().source_gamma, after.info().source_gamma);
        assert_eq!(
            before.info().source_chromaticities,
            after.info().source_chromaticities
        );
        assert_eq!(before.info().srgb, after.info().srgb);
        assert_eq!((result.width, result.height), (32, 16));
    }
}

#[test]
fn sixteen_bit_resize_keeps_precision_and_endianness() {
    let img = image::ImageBuffer::from_fn(64, 32, |x, y| image::Luma([(x * 997 + y * 281) as u16]));
    let mut source = std::io::Cursor::new(Vec::new());
    img.write_to(&mut source, image::ImageFormat::Png).unwrap();
    let input = source.into_inner();
    let expected = image::DynamicImage::ImageLuma16(img)
        .resize(32, 32, image::imageops::FilterType::Lanczos3)
        .into_luma16();
    let result = compress_bytes(
        &input,
        &EncodingOptions {
            max_edge: Some(32),
            ..Default::default()
        },
    )
    .unwrap();
    let decoded = image::load_from_memory(&result.bytes).unwrap();
    assert_eq!(decoded.color(), image::ColorType::L16);
    assert_eq!(decoded.into_luma16(), expected);
}

#[test]
fn profiled_webp_is_not_silently_relabelled_when_converting_to_avif() {
    use image::ImageEncoder;
    let img = image::RgbaImage::from_pixel(32, 16, image::Rgba([200, 50, 70, 255]));
    let mut source = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut source);
    encoder
        .set_icc_profile(include_bytes!("fixtures/srgb.icc").to_vec())
        .unwrap();
    encoder
        .write_image(img.as_raw(), 32, 16, image::ExtendedColorType::Rgba8)
        .unwrap();
    let webp = ppgoose_core::webp::compress_mode(&source, 80, true).unwrap();
    assert!(compress_bytes(
        &webp,
        &EncodingOptions {
            convert_webp_to: Some(ppgoose_core::ConvertTarget::Avif),
            ..Default::default()
        }
    )
    .unwrap_err()
    .to_string()
    .contains("ICC"));
}
