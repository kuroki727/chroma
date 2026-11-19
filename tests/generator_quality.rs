use chroma::{
    adapters::{self, template},
    palette::{
        Palette,
        colors::Color,
        generator::{Mode, generate},
    },
};
use image::{Rgb, RgbImage};
use std::path::Path;

fn chroma(color: Color) -> f64 {
    let [_, a, b] = color.oklab();
    a.hypot(b)
}
fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    d.min(360.0 - d)
}
fn check(p: &Palette) {
    let c = &p.colors;
    for bg in [c.background, c.surface, c.surface_alt] {
        for text in [
            c.foreground,
            c.muted,
            c.primary,
            c.secondary,
            c.error,
            c.warning,
            c.success,
        ] {
            assert!(
                text.contrast(bg) >= 4.5,
                "{text} on {bg}: {}",
                text.contrast(bg)
            );
        }
        assert!(c.border.contrast(bg) >= 3.0);
    }
    assert!(c.primary.distance(c.secondary) > 0.05);
    assert_eq!(p.ansi.len(), 16);
    for i in 0..16 {
        assert!(
            p.ansi[&i.to_string()].contrast(c.background) >= 4.5,
            "ANSI {i}"
        );
    }
    assert_eq!(*p, Palette::parse(&toml::to_string(p).unwrap()).unwrap());
    for (_, _, source) in adapters::BUILTINS {
        let alpha = adapters::FUZ_ALPHA
            .iter()
            .map(|k| (k.to_string(), 238))
            .collect();
        assert!(
            !template::render_with_alpha(source, p, &alpha)
                .unwrap()
                .contains("{{")
        );
    }
}
fn fixture(path: &Path, colors: [[u8; 3]; 4]) {
    RgbImage::from_fn(128, 96, |x, y| {
        Rgb(colors[if x < 64 {
            0
        } else if y < 60 {
            1
        } else if x < 108 {
            2
        } else {
            3
        }])
    })
    .save(path)
    .unwrap();
}
#[test]
fn grayscale_wallpapers_do_not_invent_blue_or_purple_accents() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("gray.png");
    fixture(&path, [[40; 3], [100; 3], [170; 3], [220; 3]]);
    for mode in [Mode::Dark, Mode::Light] {
        let p = generate(&path, mode, "Gray").unwrap();
        for color in [p.colors.background, p.colors.primary, p.colors.secondary] {
            assert!(chroma(color) < 0.008, "invented saturation: {color}");
        }
        check(&p);
    }
}
#[test]
fn real_scene_families_keep_source_accent_hues_and_readable_roles() {
    let root = tempfile::tempdir().unwrap();
    for (name, colors) in [
        (
            "forest",
            [[17, 34, 26], [40, 71, 48], [81, 103, 66], [152, 165, 149]],
        ),
        (
            "brown",
            [[46, 29, 20], [100, 60, 36], [161, 104, 54], [214, 175, 119]],
        ),
        (
            "sky",
            [
                [57, 139, 211],
                [108, 186, 226],
                [176, 218, 238],
                [239, 244, 246],
            ],
        ),
        (
            "muted",
            [
                [115, 119, 116],
                [119, 123, 120],
                [122, 126, 123],
                [126, 130, 127],
            ],
        ),
        (
            "saturated",
            [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]],
        ),
    ] {
        let path = root.path().join(format!("{name}.png"));
        fixture(&path, colors);
        for mode in [Mode::Dark, Mode::Light] {
            let p = generate(&path, mode, name).unwrap();
            check(&p);
            assert_eq!(p, generate(&path, mode, name).unwrap());
            for accent in [p.colors.primary, p.colors.secondary] {
                if chroma(accent) > 0.015 {
                    assert!(
                        colors.iter().any(|rgb| hue_distance(
                            accent.hue(),
                            Color(rgb[0], rgb[1], rgb[2]).hue()
                        ) < 14.0),
                        "{name}: unrelated {accent}"
                    );
                }
            }
        }
    }
}
#[test]
fn monochromatic_small_and_extreme_aspect_images_are_readable_and_deterministic() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("small.png");
    for (w, h) in [(1, 1), (1, 513), (513, 1), (3, 257), (257, 3), (131, 129)] {
        for rgb in [[0, 0, 0], [255, 255, 255], [60, 80, 65], [255, 0, 255]] {
            RgbImage::from_pixel(w, h, Rgb(rgb)).save(&path).unwrap();
            for mode in [Mode::Dark, Mode::Light] {
                let p = generate(&path, mode, "Small").unwrap();
                check(&p);
                assert_eq!(p, generate(&path, mode, "Small").unwrap());
            }
        }
    }
}
#[test]
fn terminal_colors_keep_conventional_identities_in_both_modes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("forest.png");
    fixture(
        &path,
        [[10, 25, 15], [30, 70, 40], [60, 100, 60], [160, 170, 155]],
    );
    for mode in [Mode::Dark, Mode::Light] {
        let p = generate(&path, mode, "Forest").unwrap();
        check(&p);
        for (index, anchor, tolerance) in [
            (1, 25.0, 20.0),
            (2, 145.0, 22.0),
            (3, 90.0, 20.0),
            (4, 265.0, 22.0),
            (5, 325.0, 25.0),
            (6, 200.0, 25.0),
        ] {
            for i in [index, index + 8] {
                let color = p.ansi[&i.to_string()];
                assert!(
                    hue_distance(color.hue(), anchor) <= tolerance,
                    "ANSI {i} {color} hue {}",
                    color.hue()
                );
            }
            assert!(p.ansi[&index.to_string()].distance(p.ansi[&(index + 8).to_string()]) > 0.035);
        }
    }
}

#[test]
fn extraction_retains_supported_accents_merges_duplicates_and_rejects_noise() {
    use chroma::palette::generator::extract;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("accent.png");
    let green = Color(35, 75, 48);
    let red = Color(180, 58, 45);
    let mut image = RgbImage::from_fn(128, 128, |x, _| {
        if x < 120 {
            Rgb([green.0, green.1, green.2])
        } else {
            Rgb([red.0, red.1, red.2])
        }
    });
    image.put_pixel(0, 0, Rgb([0, 0, 255])); // Single-pixel noise must not define an accent.
    image.save(&path).unwrap();
    let colors = extract(&path).unwrap();
    assert_eq!(colors, extract(&path).unwrap());
    assert_eq!(colors.len(), 2);
    assert!(colors[0].color.distance(green) < 0.01);
    assert!(colors[1].color.distance(red) < 0.01);
    assert!((colors[1].population - 0.0625).abs() < 0.002);
    assert!((colors.iter().map(|c| c.population).sum::<f64>() - 1.0).abs() < 1e-10);
    for mode in [Mode::Dark, Mode::Light] {
        let palette = generate(&path, mode, "Accent").unwrap();
        check(&palette);
        assert!(hue_distance(palette.colors.primary.hue(), green.hue()) < 5.0);
        assert!(hue_distance(palette.colors.secondary.hue(), red.hue()) < 5.0);
    }
    RgbImage::from_fn(128, 128, |x, _| {
        Rgb(if x < 64 {
            [100, 120, 105]
        } else {
            [101, 121, 106]
        })
    })
    .save(&path)
    .unwrap();
    assert_eq!(extract(&path).unwrap().len(), 1);
}

#[test]
fn hidden_transparent_rgb_never_contaminates_representatives() {
    use chroma::palette::generator::extract;
    use image::{Rgba, RgbaImage};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("alpha.png");
    for phase in [0, 1] {
        RgbaImage::from_fn(256, 256, |x, y| {
            if (x + y + phase) % 2 == 0 {
                Rgba([35, 75, 48, 255])
            } else {
                Rgba([255, 0, 255, 0])
            }
        })
        .save(&path)
        .unwrap();
        let colors = extract(&path).unwrap();
        assert_eq!(colors.len(), 1);
        assert!(colors[0].color.distance(Color(35, 75, 48)) < 0.005);
    }
}

#[test]
fn independent_axis_sampling_covers_both_extreme_aspect_ratios() {
    use chroma::palette::generator::extract;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("thin.png");
    for (w, h) in [(4096, 1), (1, 4096)] {
        RgbImage::from_fn(w, h, |x, y| {
            if x + y < 2048 {
                Rgb([35, 75, 48])
            } else {
                Rgb([180, 58, 45])
            }
        })
        .save(&path)
        .unwrap();
        let colors = extract(&path).unwrap();
        assert_eq!(colors.len(), 2);
        assert!(colors.iter().all(|c| (c.population - 0.5).abs() < 0.01));
    }
}

#[test]
fn generated_palettes_render_all_dedicated_desktop_formats() {
    use chroma::integration::colors::{self, Kind};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("desktop.png");
    fixture(
        &path,
        [[17, 34, 26], [40, 71, 48], [81, 103, 66], [152, 165, 149]],
    );
    let fuzzel = "[main]\nfont=monospace:size=12\n[colors]\nbackground=112233ee\ntext=112233ff\nprompt=112233ff\nplaceholder=112233ff\ninput=112233ff\nmatch=112233ff\nselection=112233ff\nselection-text=112233ff\nselection-match=112233ff\nborder=112233ff\n";
    let mut kitty = String::from("font_size 12\nbackground #112233\nforeground #dddddd\n");
    for i in 0..16 {
        kitty.push_str(&format!("color{i} #112233\n"));
    }
    for mode in [Mode::Dark, Mode::Light] {
        let p = generate(&path, mode, "Desktop").unwrap();
        for (kind, source) in [
            (
                Kind::Island,
                include_str!("fixtures/desktop/island/style.css"),
            ),
            (
                Kind::Waybar,
                include_str!("fixtures/desktop/waybar/style.css"),
            ),
            (Kind::Fuzzel, fuzzel),
            (Kind::Kitty, &kitty),
            (Kind::Niri, "layout {}"),
            (
                Kind::Swaylock,
                "color=112233ee\nring-color=112233ff\nclock\n",
            ),
        ] {
            let text = colors::render(kind, source, &p).unwrap();
            assert!(!text.contains("{{"));
            match kind {
                Kind::Fuzzel => {
                    assert!(text.contains("font=monospace:size=12"));
                    assert!(text.contains("ee"));
                }
                Kind::Island => assert!(text.contains("font-size")),
                Kind::Kitty => {
                    assert!(!text.contains("font_size"));
                    assert!(text.contains(&p.variables()["ansi5"].to_string()));
                }
                Kind::Swaylock => assert!(text.contains("clock")),
                _ => (),
            }
        }
    }
}
