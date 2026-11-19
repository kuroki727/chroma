use crate::{
    Result, ensure,
    palette::{
        Colors, Palette,
        colors::{Color, distance},
    },
};
use clap::ValueEnum;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

/// A population-weighted OKLab representative, after merging near duplicates.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Representative {
    pub color: Color,
    pub population: f64,
}

/// Bounded, deterministic extraction. Sample actual pixels so resampling cannot
/// blend hidden transparent RGB into visible colors or invent intermediate hues.
pub fn extract(path: &Path) -> Result<Vec<Representative>> {
    let sampled = super::sampling::read(path)?;
    let mut histogram = BTreeMap::<[u8; 3], usize>::new();
    for pixel in sampled {
        if pixel[3] >= 128 {
            *histogram.entry([pixel[0], pixel[1], pixel[2]]).or_default() += usize::from(pixel[3]);
        }
    }
    ensure!(
        !histogram.is_empty(),
        "image has no sufficiently opaque pixels"
    );
    let points: Vec<_> = histogram
        .into_iter()
        .map(|([r, g, b], n)| (Color(r, g, b).oklab(), n))
        .collect();
    let total: usize = points.iter().map(|(_, n)| n).sum();
    let mut mean = [0.0; 3];
    for (p, n) in &points {
        for (value, coordinate) in mean.iter_mut().zip(p) {
            *value += coordinate * *n as f64 / total as f64;
        }
    }
    let mut centers = vec![
        points
            .iter()
            .min_by(|(a, _), (b, _)| distance(*a, mean).total_cmp(&distance(*b, mean)))
            .expect("nonempty points")
            .0,
    ];
    while centers.len() < 12.min(points.len()) {
        let (point, separation, _) = points
            .iter()
            .map(|(p, n)| {
                let separation = centers
                    .iter()
                    .map(|c| distance(*p, *c))
                    .fold(f64::INFINITY, f64::min);
                (
                    *p,
                    separation,
                    separation * (*n as f64 / total as f64).sqrt(),
                )
            })
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .expect("nonempty points");
        if separation < 0.03_f64.powi(2) {
            break;
        }
        centers.push(point);
    }
    let mut counts = vec![0_usize; centers.len()];
    for _ in 0..24 {
        let mut sums = vec![[0.0; 3]; centers.len()];
        counts.fill(0);
        for (point, count) in &points {
            let index = nearest(*point, &centers);
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
    // Counts must describe the final centers, including iteration-limit exits.
    counts.fill(0);
    for (point, count) in &points {
        counts[nearest(*point, &centers)] += count;
    }
    let mut clusters: Vec<_> = centers
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .collect();
    loop {
        let mut closest = None;
        let mut separation = 0.04_f64.powi(2);
        for i in 0..clusters.len() {
            for j in i + 1..clusters.len() {
                let d = distance(clusters[i].0, clusters[j].0);
                if d < separation {
                    separation = d;
                    closest = Some((i, j));
                }
            }
        }
        let Some((i, j)) = closest else { break };
        let (other, n) = clusters.remove(j);
        let (center, count) = &mut clusters[i];
        for (a, b) in center.iter_mut().zip(other) {
            *a = (*a * *count as f64 + b * n as f64) / (*count + n) as f64;
        }
        *count += n;
    }
    // Ignore isolated noise below half a percent, retaining modest real accents.
    clusters.retain(|(_, n)| *n as f64 / total as f64 >= 0.005);
    let kept: usize = clusters.iter().map(|(_, n)| n).sum();
    ensure!(kept > 0, "no representative image colors");
    clusters.sort_by(|(a, an), (b, bn)| {
        bn.cmp(an)
            .then_with(|| a[0].total_cmp(&b[0]))
            .then_with(|| a[1].total_cmp(&b[1]))
            .then_with(|| a[2].total_cmp(&b[2]))
    });
    Ok(clusters
        .into_iter()
        .map(|(lab, n)| Representative {
            color: Color::lch(lab[0], chroma(lab), hue(lab)),
            population: n as f64 / kept as f64,
        })
        .collect())
}

fn nearest(point: [f64; 3], centers: &[[f64; 3]]) -> usize {
    centers
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| distance(point, **a).total_cmp(&distance(point, **b)))
        .expect("nonempty centers")
        .0
}
fn chroma(lab: [f64; 3]) -> f64 {
    lab[1].hypot(lab[2])
}
fn hue(lab: [f64; 3]) -> f64 {
    lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
}
fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    d.min(360.0 - d)
}

pub fn generate(path: &Path, mode: Mode, name: &str) -> Result<Palette> {
    let representatives = extract(path)?;
    let mut mean = [0.0; 3];
    for source in &representatives {
        for (value, coordinate) in mean.iter_mut().zip(source.color.oklab()) {
            *value += coordinate * source.population;
        }
    }
    let light = mode == Mode::Light;
    let tone_c = (chroma(mean) * 0.25).min(0.025);
    let tone_h = hue(mean);
    let neutral = |l| Color::lch(l, tone_c, tone_h);
    let bg_l = if light {
        0.94 + 0.04 * mean[0]
    } else {
        0.12 + 0.08 * mean[0]
    };
    let background = neutral(bg_l);
    let surface = neutral(bg_l + if light { -0.05 } else { 0.06 });
    let surface_alt = neutral(bg_l + if light { -0.10 } else { 0.12 });
    let backgrounds = [background, surface, surface_alt];
    let readable = |l, c, h, target| readable_color(l, c, h, backgrounds, light, target);
    let score =
        |s: &Representative| chroma(s.color.oklab()).max(0.008).powf(0.7) * s.population.sqrt();
    let primary_source = representatives
        .iter()
        .max_by(|a, b| score(a).total_cmp(&score(b)))
        .expect("nonempty representatives");
    let primary_lab = primary_source.color.oklab();
    let primary_c = if chroma(primary_lab) < 0.008 {
        0.0
    } else {
        (chroma(primary_lab) * 1.1).min(0.18)
    };
    let primary = readable(
        if light { 0.46 } else { 0.74 } + (primary_lab[0] - 0.5) * 0.06,
        primary_c,
        hue(primary_lab),
        4.5,
    );
    // Compare chromatic coordinates, not just hue: near-neutral hue is unstable.
    let secondary_source = representatives
        .iter()
        .filter(|s| {
            let lab = s.color.oklab();
            (lab[1] - primary_lab[1]).hypot(lab[2] - primary_lab[2]) >= 0.015
                && chroma(lab) >= 0.008
        })
        .max_by(|a, b| score(a).total_cmp(&score(b)));
    let secondary_lab = secondary_source.map_or(primary_lab, |s| s.color.oklab());
    let secondary_c = if chroma(secondary_lab) < 0.008 {
        0.0
    } else {
        (chroma(secondary_lab)
            * if secondary_source.is_some() {
                1.0
            } else {
                0.55
            })
        .min(0.16)
    };
    let secondary = readable(
        if light { 0.27 } else { 0.85 },
        secondary_c,
        hue(secondary_lab),
        4.5,
    );
    let saturation = representatives
        .iter()
        .map(|s| chroma(s.color.oklab()) * s.population)
        .sum::<f64>();
    // Status and ANSI roles retain conventional identities. Nearby source hues
    // and scene saturation harmonize them without moving red into green, etc.
    let family = |anchor: f64, window: f64| {
        let source = representatives
            .iter()
            .filter(|s| {
                chroma(s.color.oklab()) >= 0.015 && hue_distance(s.color.hue(), anchor) <= window
            })
            .max_by(|a, b| a.population.total_cmp(&b.population));
        let c = (0.055 + saturation * 0.55).clamp(0.055, 0.15);
        source.map_or((anchor, c), |s| {
            (
                s.color.hue(),
                ((c + chroma(s.color.oklab())) * 0.5).clamp(0.055, 0.17),
            )
        })
    };
    let status = |anchor, window, l| {
        let (h, c) = family(anchor, window);
        readable(l, c, h, 4.5)
    };
    let colors = Colors {
        background,
        surface,
        surface_alt,
        foreground: readable(if light { 0.14 } else { 0.96 }, tone_c * 0.5, tone_h, 4.5),
        muted: readable(if light { 0.35 } else { 0.65 }, tone_c, tone_h, 4.5),
        primary,
        secondary,
        border: readable(if light { 0.58 } else { 0.55 }, tone_c, tone_h, 3.0),
        error: status(25.0, 15.0, if light { 0.36 } else { 0.67 }),
        warning: status(90.0, 15.0, if light { 0.48 } else { 0.88 }),
        success: status(145.0, 20.0, if light { 0.40 } else { 0.72 }),
    };
    let mut ansi = BTreeMap::new();
    for (index, anchor, window) in [
        (1, 25.0, 15.0),
        (2, 145.0, 20.0),
        (3, 90.0, 15.0),
        (4, 265.0, 20.0),
        (5, 325.0, 20.0),
        (6, 200.0, 20.0),
    ] {
        let (h, c) = family(anchor, window);
        for bright in [false, true] {
            let l = if light {
                if bright { 0.30 } else { 0.43 }
            } else if bright {
                0.84
            } else {
                0.71
            };
            let color = readable_color(l, c, h, [background; 3], light, 4.5);
            ansi.insert((index + if bright { 8 } else { 0 }).to_string(), color);
        }
    }
    for (index, l) in [
        (0, if light { 0.21 } else { 0.59 }),
        (8, if light { 0.43 } else { 0.75 }),
        (7, if light { 0.30 } else { 0.87 }),
        (15, if light { 0.12 } else { 0.98 }),
    ] {
        ansi.insert(
            index.to_string(),
            readable_color(l, tone_c, tone_h, [background; 3], light, 4.5),
        );
    }
    let palette = Palette {
        name: name.into(),
        author: Some("Chroma wallpaper generator".into()),
        colors,
        ansi,
    };
    palette.validate()?;
    for bg in backgrounds {
        for color in [
            palette.colors.foreground,
            palette.colors.muted,
            primary,
            secondary,
            palette.colors.error,
            palette.colors.warning,
            palette.colors.success,
        ] {
            ensure!(
                color.contrast(bg) >= 4.5,
                "could not generate readable text colors"
            );
        }
        ensure!(
            palette.colors.border.contrast(bg) >= 3.0,
            "could not generate readable border"
        );
    }
    ensure!(
        primary.distance(secondary) > 0.05,
        "could not generate distinct accents"
    );
    ensure!(
        palette.ansi.values().all(|c| c.contrast(background) >= 4.5),
        "could not generate readable ANSI colors"
    );
    Ok(palette)
}

fn readable_color(
    mut l: f64,
    c: f64,
    h: f64,
    backgrounds: [Color; 3],
    light: bool,
    target: f64,
) -> Color {
    for _ in 0..200 {
        let color = Color::lch(l, c, h);
        if backgrounds.iter().all(|bg| color.contrast(*bg) >= target) {
            return color;
        }
        l = (l + if light { -0.005 } else { 0.005 }).clamp(0.0, 1.0);
    }
    if light {
        Color(0, 0, 0)
    } else {
        Color(255, 255, 255)
    }
}
