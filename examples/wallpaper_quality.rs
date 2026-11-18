//! Reproducible synthetic fixtures and isolated palette/contrast reports.
//! cargo run --locked --example wallpaper_quality -- OUTPUT_DIR [WALLPAPER] [PREVIOUS_REPORT_DIR]
use chroma::adapters;
use chroma::integration::colors::{self, Kind};
use chroma::palette::{
    Palette,
    generator::{self, Mode},
    presets,
};
use image::{Rgb, RgbImage};
use std::{collections::BTreeMap, env, fs, path::Path};

fn save(root: &Path, id: &str, palette: &Palette) -> anyhow::Result<()> {
    chroma::palette::valid_id(id)?;
    fs::write(
        root.join(format!("{id}.toml")),
        toml::to_string_pretty(palette)?,
    )?;
    let c = &palette.colors;
    let backgrounds = [c.background, c.surface, c.surface_alt];
    let text = [
        c.foreground,
        c.muted,
        c.primary,
        c.secondary,
        c.error,
        c.warning,
        c.success,
    ];
    let min_text = text
        .iter()
        .flat_map(|text| backgrounds.map(|bg| text.contrast(bg)))
        .fold(f64::INFINITY, f64::min);
    let min_border = backgrounds
        .map(|bg| c.border.contrast(bg))
        .into_iter()
        .fold(f64::INFINITY, f64::min);
    let min_ansi = (0..16)
        .map(|i| palette.variables()[&format!("ansi{i}")].contrast(c.background))
        .fold(f64::INFINITY, f64::min);
    println!(
        "{id}: bg={} fg={} primary={} secondary={} foreground={:.2} min-text={min_text:.2} border={min_border:.2} ANSI={min_ansi:.2}",
        c.background,
        c.foreground,
        c.primary,
        c.secondary,
        c.foreground.contrast(c.background)
    );
    let c = &palette.colors;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="780" height="650" viewBox="0 0 780 650"><rect width="780" height="650" fill="{}"/><g font-family="monospace" fill="{}"><text x="24" y="32" font-size="20">{id}</text>"#,
        c.background, c.foreground
    );
    for (i, (name, color)) in palette.colors.named().iter().enumerate() {
        let y = 58 + i * 35;
        svg.push_str(&format!(r#"<rect x="24" y="{y}" width="80" height="26" fill="{color}"/><text x="120" y="{}" font-size="16">{name}: {color}</text>"#,y+19));
    }
    let variables = palette.variables();
    for i in 0..16 {
        let x = 24 + (i % 8) * 92;
        let y = 470 + (i / 8) * 80;
        let color = variables[&format!("ansi{i}")];
        svg.push_str(&format!(r#"<rect x="{x}" y="{y}" width="78" height="30" fill="{color}"/><text x="{x}" y="{}" font-size="12">{i}: {color}</text>"#,y+48));
    }
    svg.push_str("</g></svg>\n");
    fs::write(root.join(format!("{id}.svg")), svg)?;
    let fragments = root.join("adapters").join(id);
    fs::create_dir_all(&fragments)?;
    let alpha = adapters::FUZ_ALPHA
        .iter()
        .map(|key| (key.to_string(), 255))
        .collect();
    for (_, filename, source) in adapters::BUILTINS {
        fs::write(
            fragments.join(filename),
            adapters::template::render_with_alpha(source, palette, &alpha)?,
        )?;
    }
    fs::write(
        fragments.join("island.css"),
        colors::render(
            Kind::Island,
            include_str!("../tests/fixtures/desktop/island/style.css"),
            palette,
        )?,
    )?;
    Ok(())
}
fn sources(root: &Path, id: &str, path: &Path) -> anyhow::Result<()> {
    let colors = generator::extract(path)?;
    fs::write(
        root.join(format!("{id}-representatives.toml")),
        toml::to_string_pretty(&BTreeMap::from([("representatives", colors)]))?,
    )?;
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    anyhow::ensure!(args.len() >= 2, "supply an isolated output directory");
    let root = Path::new(&args[1]);
    if root.exists() {
        anyhow::ensure!(
            fs::read_dir(root)?.next().is_none(),
            "choose a new or empty output directory; existing artifacts are preserved"
        );
    }
    fs::create_dir_all(root)?;
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
            "low-contrast",
            [
                [115, 119, 116],
                [119, 123, 120],
                [122, 126, 123],
                [126, 130, 127],
            ],
        ),
        (
            "gray",
            [
                [60, 60, 60],
                [100, 100, 100],
                [150, 150, 150],
                [205, 205, 205],
            ],
        ),
        (
            "saturated",
            [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]],
        ),
    ] {
        let image = RgbImage::from_fn(128, 96, |x, y| {
            let index = if x < 64 {
                0
            } else if y < 60 {
                1
            } else if x < 108 {
                2
            } else {
                3
            };
            Rgb(colors[index])
        });
        let path = root.join(format!("{name}.png"));
        image.save(&path)?;
        sources(root, name, &path)?;
        for (suffix, mode) in [("dark", Mode::Dark), ("light", Mode::Light)] {
            save(
                root,
                &format!("{name}-{suffix}"),
                &generator::generate(&path, mode, name)?,
            )?;
        }
    }
    if let Some(image) = args.get(2) {
        sources(root, "wallpaper", Path::new(image))?;
        for (suffix, mode) in [("dark", Mode::Dark), ("light", Mode::Light)] {
            save(
                root,
                &format!("wallpaper-{suffix}"),
                &generator::generate(Path::new(image), mode, "Foggy Forest")?,
            )?;
        }
    }
    if let Some(previous) = args.get(3) {
        let comparison = root.join("baseline");
        fs::create_dir_all(&comparison)?;
        let mut files = fs::read_dir(previous)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        files.sort();
        for path in files {
            if path.extension().is_some_and(|e| e == "toml")
                && !path
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with("-representatives")
            {
                let id = path.file_stem().unwrap().to_str().expect("UTF-8 report ID");
                save(
                    &comparison,
                    id,
                    &Palette::parse(&fs::read_to_string(&path)?)?,
                )?;
            }
        }
    }
    save(
        root,
        "rustwood",
        &presets::get("rustwood").expect("embedded rustwood"),
    )?;
    Ok(())
}
