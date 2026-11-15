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
