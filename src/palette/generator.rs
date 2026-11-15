use crate::{
    Context, Result, ensure,
    palette::{
        Colors, Palette,
        colors::{Color, distance},
    },
};
use clap::ValueEnum;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Cursor, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const MAX_INPUT: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: u64 = 8_388_608; // Includes 3840×2160, rejects decode bombs before allocation.
const MAX_DECODED: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

/// Fixed-seed, farthest-point initialized Lloyd k-means in OKLab.
/// Sampling is bounded to 128×128; no random state or hash iteration order.
pub fn generate(path: &Path, mode: Mode, name: &str) -> Result<Palette> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
        .with_context(|| format!("opening image {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_INPUT,
        "image must be a regular file no larger than 64 MiB"
    );
    // A metadata length check alone does not bound a growing or virtual file.
    // Freeze bounded input before passing it to codecs which read to EOF.
    let mut input = Vec::new();
    file.take(MAX_INPUT + 1).read_to_end(&mut input)?;
    ensure!(
        input.len() as u64 <= MAX_INPUT,
        "compressed image exceeds 64 MiB"
    );
    let mut reader = ImageReader::new(Cursor::new(input)).with_guessed_format()?;
    ensure!(
        matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)),
        "only PNG and JPEG images are supported"
    );
    let mut limits = Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(MAX_DECODED);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .context("reading image headers within limits")?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "image pixel count exceeds 8,388,608 or is zero"
    );
    ensure!(
        decoder.total_bytes() <= MAX_DECODED,
        "decoded image exceeds 64 MiB"
    );
    let sampled = DynamicImage::from_decoder(decoder)
        .context("decoding image within pixel/output limits")?
        .thumbnail(128, 128)
        .to_rgba8();
    // Sort and collapse repeated colors to reduce work on flat wallpapers.
    let mut histogram = BTreeMap::<[u8; 3], usize>::new();
    for pixel in sampled.pixels().filter(|p| p.0[3] >= 128) {
        *histogram
            .entry([pixel.0[0], pixel.0[1], pixel.0[2]])
            .or_default() += 1;
    }
    ensure!(
        !histogram.is_empty(),
        "image has no sufficiently opaque pixels"
    );
    let points: Vec<([f64; 3], usize)> = histogram
        .into_iter()
        .map(|([r, g, b], n)| (Color(r, g, b).oklab(), n))
        .collect();
    let mut centers = vec![
        points
            .iter()
            .max_by_key(|(_, n)| *n)
            .expect("nonempty points")
            .0,
    ];
    while centers.len() < 12.min(points.len()) {
        let best = points
            .iter()
            .map(|(p, _)| {
                (
                    *p,
                    centers
                        .iter()
                        .map(|c| distance(*p, *c))
                        .fold(f64::INFINITY, f64::min),
                )
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("nonempty points");
        if best.1 < 0.0004 {
            break;
        }
        centers.push(best.0);
    }
    let mut counts = vec![0_usize; centers.len()];
    for _ in 0..24 {
        let mut sums = vec![[0.0; 3]; centers.len()];
        counts.fill(0);
        for (point, count) in &points {
            let index = centers
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| distance(*point, **a).total_cmp(&distance(*point, **b)))
                .expect("nonempty centers")
                .0;
            counts[index] += count;
            for (sum, value) in sums[index].iter_mut().zip(point) {
                *sum += value * *count as f64;
            }
        }
        let mut movement: f64 = 0.0;
        for ((center, sum), count) in centers.iter_mut().zip(sums).zip(&counts) {
            if *count > 0 {
                let next = sum.map(|v| v / *count as f64);
                movement = movement.max(distance(*center, next));
                *center = next;
            }
        }
        if movement < 1e-8 {
            break;
        }
    }
    let chroma = |c: [f64; 3]| c[1].hypot(c[2]);
    let hue = |c: [f64; 3]| c[2].atan2(c[1]).to_degrees().rem_euclid(360.0);
    let primary = centers
        .iter()
        .zip(&counts)
        .max_by(|(a, an), (b, bn)| {
            (chroma(**a) * (**an as f64).sqrt()).total_cmp(&(chroma(**b) * (**bn as f64).sqrt()))
        })
        .expect("nonempty centers")
        .0;
    let h = if chroma(*primary) < 0.02 {
        220.0
    } else {
        hue(*primary)
    };
    let hue_distance = |candidate: f64| {
        let d = (candidate - h).abs();
        d.min(360.0 - d)
    };
    let secondary = centers
        .iter()
        .zip(&counts)
        .filter(|(c, n)| **n > 0 && chroma(**c) > 0.03 && hue_distance(hue(**c)) >= 45.0)
        .max_by(|(a, an), (b, bn)| {
            (chroma(**a) * (**an as f64).sqrt()).total_cmp(&(chroma(**b) * (**bn as f64).sqrt()))
        })
        .map(|(c, _)| *c);
    let sh = secondary.map_or((h + 75.0).rem_euclid(360.0), hue);
    let light = mode == Mode::Light;
    let neutral = |l| Color::lch(l, 0.018, h);
    let background = neutral(if light { 0.97 } else { 0.16 });
    let surface = neutral(if light { 0.92 } else { 0.23 });
    let surface_alt = neutral(if light { 0.87 } else { 0.30 });
    let accent = |h, c| {
        readable_accent(
            if light { 0.45 } else { 0.76 },
            c,
            h,
            [background, surface, surface_alt],
            light,
        )
    };
    // Status colors keep their conventional hues and have distinct lightness
    // levels, even when the wallpaper's main accent is itself red/green/yellow.
    let status = |h, c, dark_l, light_l| {
        readable_accent(
            if light { light_l } else { dark_l },
            c,
            h,
            [background, surface, surface_alt],
            light,
        )
    };
    let colors = Colors {
        background,
        surface,
        surface_alt,
        foreground: neutral(if light { 0.19 } else { 0.96 }),
        muted: neutral(if light { 0.43 } else { 0.75 }),
        primary: accent(h, chroma(*primary).clamp(0.08, 0.18)),
        secondary: accent(sh, secondary.map_or(0.10, chroma).clamp(0.08, 0.16)),
        border: neutral(if light { 0.66 } else { 0.43 }),
        error: status(25.0, 0.17, 0.67, 0.35),
        warning: status(85.0, 0.15, 0.84, 0.52),
        success: status(145.0, 0.13, 0.69, 0.38),
    };
    let palette = Palette {
        name: name.to_string(),
        author: Some("Chroma wallpaper generator".into()),
        colors,
        ansi: BTreeMap::new(),
    };
    palette.validate()?;
    ensure!(
        palette.colors.primary.distance(palette.colors.secondary) > 0.05,
        "could not generate distinct accents"
    );
    Ok(palette)
}

fn readable_accent(mut l: f64, c: f64, h: f64, backgrounds: [Color; 3], light: bool) -> Color {
    for _ in 0..50 {
        let color = Color::lch(l, c, h);
        if backgrounds.iter().all(|bg| color.contrast(*bg) >= 3.0) {
            return color;
        }
        l = (l + if light { -0.01 } else { 0.01 }).clamp(0.05, 0.95);
    }
    if light {
        Color(0, 0, 0)
    } else {
        Color(255, 255, 255)
    }
}
