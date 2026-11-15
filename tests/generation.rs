use chroma::palette::{
    Palette,
    generator::{Mode, generate},
};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use tempfile::tempdir;

#[test]
fn png_jpeg_generation_is_deterministic_readable_and_distinct() {
    let root = tempdir().unwrap();
    let mut image = RgbImage::new(96, 64);
    for (x, y, p) in image.enumerate_pixels_mut() {
        *p = if x < 48 {
            Rgb([220, 75 + (y / 2) as u8, 40])
        } else {
            Rgb([40, 110, 180 + (y / 2) as u8])
        };
    }
    for ext in ["png", "jpg"] {
        let path = root.path().join(format!("wall.{ext}"));
        image.save(&path).unwrap();
        for mode in [Mode::Dark, Mode::Light] {
            let a = generate(&path, mode, "Wallpaper").unwrap();
            let b = generate(&path, mode, "Wallpaper").unwrap();
            assert_eq!(a, b);
            assert_eq!(toml::to_string(&a).unwrap(), toml::to_string(&b).unwrap());
            assert_eq!(Palette::parse(&toml::to_string(&a).unwrap()).unwrap(), a);
            let c = &a.colors;
            for bg in [c.background, c.surface, c.surface_alt] {
                assert!(c.foreground.contrast(bg) >= 4.5);
                assert!(c.muted.contrast(bg) >= 4.5);
                for accent in [c.primary, c.secondary, c.error, c.warning, c.success] {
                    assert!(accent.contrast(bg) >= 3.0);
                }
            }
            assert!(c.primary.distance(c.secondary) > 0.05);
            assert!(c.surface.distance(c.background) > 0.04);
            assert_eq!(
                c.background.luminance() < c.foreground.luminance(),
                mode == Mode::Dark
            );
        }
    }
}

#[test]
fn monochrome_and_single_pixel_images_have_usable_fallbacks() {
    let root = tempdir().unwrap();
    for rgb in [
        [0, 0, 0],
        [255, 255, 255],
        [127, 127, 127],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
    ] {
        let path = root.path().join("flat.png");
        RgbImage::from_pixel(1, 1, Rgb(rgb)).save(&path).unwrap();
        for mode in [Mode::Dark, Mode::Light] {
            let p = generate(&path, mode, "Flat").unwrap();
            assert!(p.colors.primary.distance(p.colors.secondary) > 0.05);
            let colors: Vec<_> = p.colors.named().into_iter().collect();
            for (i, (an, a)) in colors.iter().enumerate() {
                for (bn, b) in &colors[i + 1..] {
                    assert!(a.distance(*b) > 0.035, "{rgb:?} {mode:?} {an} vs {bn}");
                }
            }
        }
    }
}

#[test]
fn corrupt_missing_unsupported_and_transparent_images_fail() {
    let root = tempdir().unwrap();
    assert!(generate(&root.path().join("missing.png"), Mode::Dark, "X").is_err());
    let path = root.path().join("bad.png");
    std::fs::write(&path, b"not an image").unwrap();
    assert!(generate(&path, Mode::Dark, "X").is_err());
    RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 0]))
        .save(&path)
        .unwrap();
    assert!(generate(&path, Mode::Dark, "X").is_err());
    let unsupported = root.path().join("file.bmp");
    std::fs::write(&unsupported, b"BM12345678").unwrap();
    assert!(generate(&unsupported, Mode::Dark, "X").is_err());
}

#[test]
fn dark_bright_and_low_saturation_gradients_remain_readable() {
    let root = tempdir().unwrap();
    for (name, base, range) in [
        ("dark", 0_u8, 12_u8),
        ("bright", 240, 15),
        ("gray", 70, 120),
        ("muted", 100, 60),
    ] {
        let mut image = RgbImage::new(256, 128);
        for (x, y, p) in image.enumerate_pixels_mut() {
            let shade = base + ((x + y) % u32::from(range.max(1))) as u8;
            *p = Rgb([
                shade,
                shade.saturating_add(if name == "muted" { 3 } else { 0 }),
                shade,
            ]);
        }
        let path = root.path().join(format!("{name}.png"));
        image.save(&path).unwrap();
        for mode in [Mode::Dark, Mode::Light] {
            let palette = generate(&path, mode, name).unwrap();
            assert_eq!(generate(&path, mode, name).unwrap(), palette);
            let c = palette.colors;
            for bg in [c.background, c.surface, c.surface_alt] {
                assert!(c.foreground.contrast(bg) >= 4.5);
                assert!(c.muted.contrast(bg) >= 4.5);
                for accent in [c.primary, c.secondary, c.error, c.warning, c.success] {
                    assert!(accent.contrast(bg) >= 3.0);
                }
            }
            assert!(c.primary.distance(c.secondary) > 0.05);
        }
    }
}

#[test]
fn oversized_png_headers_are_rejected_before_decoded_allocation() {
    let root = tempdir().unwrap();
    let path = root.path().join("oversized.png");
    RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    // Rewrite only IHDR and its CRC. No large pixel buffers or image bomb data
    // are created in tests; the decoder must reject the advertised dimensions.
    for (width, height) in [(8192_u32, 8192_u32), (16385, 1)] {
        let mut bytes = original.clone();
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        let mut crc = !0_u32;
        for byte in &bytes[12..29] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320_u32 & 0_u32.wrapping_sub(crc & 1));
            }
        }
        bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
        std::fs::write(&path, bytes).unwrap();
        let error = generate(&path, Mode::Dark, "Oversized").unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("pixel count") || error.contains("limit"),
            "{error}"
        );
    }
}

#[test]
fn compressed_input_size_is_bounded_without_reading_a_large_fixture() {
    let root = tempdir().unwrap();
    let path = root.path().join("large.jpg");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap(); // Sparse file, not a 64 MiB allocation.
    assert!(
        generate(&path, Mode::Dark, "Large")
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}

#[test]
fn an_image_within_the_pixel_budget_is_still_supported() {
    let root = tempdir().unwrap();
    let path = root.path().join("large-valid.png");
    image::GrayImage::from_pixel(4096, 2048, image::Luma([128]))
        .save(&path)
        .unwrap();
    let palette = generate(&path, Mode::Dark, "Large valid").unwrap();
    assert!(
        palette
            .colors
            .foreground
            .contrast(palette.colors.background)
            >= 4.5
    );
}

#[test]
fn five_k_png_and_jpeg_wallpapers_fit_the_unchanged_decoded_byte_budget() {
    let root = tempdir().unwrap();
    // Same dimensions as foggy_forest.jpg; no private wallpaper is a fixture.
    for ext in ["png", "jpg"] {
        let path = root.path().join(format!("five-k.{ext}"));
        RgbImage::from_pixel(5120, 2880, Rgb([52, 94, 73]))
            .save(&path)
            .unwrap();
        let palette = generate(&path, Mode::Dark, "5K wallpaper").unwrap();
        palette.validate().unwrap();
    }
}

#[test]
fn five_k_sixteen_bit_rgba_headers_still_hit_the_decoded_byte_limit() {
    let root = tempdir().unwrap();
    let path = root.path().join("sixteen-bit.png");
    image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_pixel(
        1,
        1,
        image::Rgba([0, 0, 0, 65535]),
    )
    .save(&path)
    .unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[16..20].copy_from_slice(&5120_u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&2880_u32.to_be_bytes());
    let mut crc = !0_u32;
    for byte in &bytes[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320_u32 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
    std::fs::write(&path, bytes).unwrap();
    let error = format!("{:#}", generate(&path, Mode::Dark, "Bounded").unwrap_err());
    assert!(
        error.contains("limit") || error.contains("decoded image exceeds"),
        "{error}"
    );
}
