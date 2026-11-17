//! Opt-in profile for the inspected Island C++ renderer. No browser CSS features.
use crate::{
    Context, Result, ensure,
    palette::{Palette, colors::Color},
};
use std::collections::BTreeMap;

pub fn hover(color: Color) -> Color {
    // panel.hpp overlays opaque white at 0.06 alpha in encoded sRGB.
    let blend = |v| (f64::from(v) * 0.94 + 255.0 * 0.06).round() as u8;
    Color(blend(color.0), blend(color.1), blend(color.2))
}

fn fit(seed: Color, targets: &[(Color, f64)]) -> Result<Color> {
    if targets
        .iter()
        .all(|(bg, minimum)| seed.contrast(*bg) >= *minimum)
    {
        return Ok(seed);
    }
    let [light, a, b] = seed.oklab();
    let chroma = a.hypot(b);
    let hue = b.atan2(a).to_degrees();
    let mut best = None;
    for step in 0..=1000 {
        let l = f64::from(step) / 1000.0;
        let color = Color::lch(l, chroma, hue);
        if targets
            .iter()
            .all(|(bg, minimum)| color.contrast(*bg) >= *minimum)
            && best.is_none_or(|(distance, _)| (l - light).abs() < distance)
        {
            best = Some(((l - light).abs(), color));
        }
    }
    best.map(|(_, color)| color)
        .context("Island color cannot meet contrast targets; no output produced")
}

pub fn profile(palette: &Palette) -> Result<BTreeMap<&'static str, Color>> {
    palette.validate()?;
    let c = &palette.colors;
    let border = fit(c.border, &[(c.background, 3.1)])?;
    let track = fit(c.surface_alt, &[(c.background, 3.1)])?;
    // The same field is a DND border and a dangerous-button fill. Choose the
    // closest-to-background error tint reaching 3.1, leaving room for light text.
    let [_, a, b] = c.error.oklab();
    let mut danger = None;
    for step in 0..=1000 {
        let color = Color::lch(
            f64::from(step) / 1000.0,
            a.hypot(b),
            b.atan2(a).to_degrees(),
        );
        if color.contrast(c.background) >= 3.1
            && danger.is_none_or(|(distance, _)| {
                (color.luminance() - c.background.luminance()).abs() < distance
            })
        {
            danger = Some(((color.luminance() - c.background.luminance()).abs(), color));
        }
    }
    let danger = danger.context("no safe Island danger surface")?.1;
    let [surface_l, _, _] = c.surface_alt.oklab();
    let [_, a, b] = c.primary.oklab();
    let on = Color::lch(surface_l, a.hypot(b).min(0.08), b.atan2(a).to_degrees());
    let surfaces = [c.background, c.surface, on, hover(c.surface), hover(on)];
    let mut text_targets: Vec<_> = surfaces.iter().map(|s| (*s, 4.6)).collect();
    let mut foreground_targets = text_targets.clone();
    foreground_targets.extend([(danger, 4.6), (hover(danger), 4.6)]);
    let foreground = fit(c.foreground, &foreground_targets)?;
    // Labels also color the muted slider thumb; titles color the normal thumb.
    text_targets.push((track, 3.1));
    let muted = fit(c.muted, &text_targets)?;
    let title = fit(c.primary, &text_targets)?;
    let fill = fit(c.primary, &[(track, 3.1), (c.background, 3.1)])?;
    Ok(BTreeMap::from([
        ("background", c.background),
        ("border", border),
        ("foreground", foreground),
        ("muted", muted),
        ("title", title),
        ("track", track),
        ("fill", fill),
        ("tile", c.surface),
        ("tile-on", on),
        ("dnd", danger),
    ]))
}

fn role(selector: &str, property: &str) -> Option<&'static str> {
    match (selector, property) {
        ("island" | "window", "background" | "background-color") => Some("background"),
        ("island" | "window", "border-color") => Some("border"),
        ("time" | "text" | "body" | "value", "color") => Some("foreground"),
        ("date" | "label", "color") => Some("muted"),
        ("title" | "flash" | "layout", "color") => Some("title"),
        ("dnd", "color") => Some("dnd"),
        ("track", "background" | "background-color") => Some("track"),
        ("fill", "background" | "background-color") => Some("fill"),
        ("tile", "background" | "background-color") => Some("tile"),
        ("tile-on", "background" | "background-color") => Some("tile-on"),
        _ => None,
    }
}

pub fn render(source: &str, palette: &Palette) -> Result<String> {
    let colors = profile(palette)?;
    let mask = super::colors::css_mask(source)?;
    let mut edits = Vec::new();
    let mut present = std::collections::BTreeSet::new();
    let mut pos = 0;
    while !mask[pos..].trim().is_empty() {
        let open = mask[pos..].find('{').context("Island CSS needs a rule")? + pos;
        let close = mask[open + 1..]
            .find('}')
            .context("Island CSS missing closing brace")?
            + open
            + 1;
        ensure!(
            !mask[open + 1..close].contains('{'),
            "nested Island CSS is unsupported"
        );
        let selectors: Vec<_> = mask[pos..open]
            .trim()
            .split(',')
            .map(|s| {
                let s = s.trim();
                s.strip_prefix('.')
                    .or_else(|| s.strip_prefix('#'))
                    .unwrap_or(s)
                    .to_ascii_lowercase()
            })
            .collect();
        let mut start = open + 1;
        for declaration in mask[open + 1..close].split(';') {
            let end = start + declaration.len();
            if !declaration.trim().is_empty() {
                let colon = declaration
                    .find(':')
                    .context("Island declaration needs colon")?
                    + start;
                let prop = mask[start..colon].trim().to_ascii_lowercase();
                let value = mask[colon + 1..end].trim();
                let mut roles = std::collections::BTreeSet::new();
                for selector in &selectors {
                    if let Some(r) = role(selector, &prop) {
                        roles.insert(r);
                        let canonical = match selector.as_str() {
                            "window" => "island",
                            "layout" => "flash",
                            other => other,
                        };
                        present.insert((
                            canonical.to_string(),
                            if prop == "background-color" {
                                "background".into()
                            } else {
                                prop.clone()
                            },
                        ));
                    } else {
                        ensure!(
                            (matches!(selector.as_str(), "island" | "window")
                                && prop == "border-width")
                                || (matches!(
                                    selector.as_str(),
                                    "time"
                                        | "date"
                                        | "text"
                                        | "title"
                                        | "body"
                                        | "label"
                                        | "value"
                                        | "flash"
                                        | "layout"
                                        | "dnd"
                                ) && prop == "font-size"),
                            "unsupported Island selector/property: {selector}.{prop}"
                        );
                        let number: f64 = value.strip_suffix("px").unwrap_or(value).parse()?;
                        ensure!(
                            number.is_finite()
                                && if prop == "font-size" {
                                    (4.0..=200.0).contains(&number)
                                } else {
                                    number >= 0.0
                                },
                            "invalid Island size"
                        );
                    }
                }
                ensure!(
                    roles.len() <= 1,
                    "split Island selectors with different semantic roles"
                );
                if let Some(r) = roles.first() {
                    // This profile certifies opaque surfaces only. Legacy Island
                    // rendering remains available unchanged for translucent CSS.
                    let value_start = colon + 1 + mask[colon + 1..end].len()
                        - mask[colon + 1..end].trim_start().len();
                    let expanded = if value.len() == 4
                        && value.starts_with('#')
                        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
                    {
                        format!(
                            "#{}",
                            value[1..].chars().flat_map(|c| [c, c]).collect::<String>()
                        )
                    } else {
                        value.to_string()
                    };
                    let opaque = if expanded.len() == 9
                        && expanded.is_ascii()
                        && expanded[7..].eq_ignore_ascii_case("ff")
                    {
                        &expanded[..7]
                    } else {
                        &expanded
                    };
                    let _: Color = opaque
                        .parse()
                        .context("readable Island profile requires opaque literal hex colors")?;
                    edits.push((
                        value_start..value_start + value.len(),
                        colors[r].to_string(),
                    ));
                }
            }
            start = end + 1;
        }
        pos = close + 1;
    }
    let mut result = source.to_string();
    for (range, value) in edits.into_iter().rev() {
        result.replace_range(range, &value);
    }
    // Missing user declarations currently fall back to these actual C++ defaults.
    // Override colors only; default fonts, geometry and behavioral JSON stay intact.
    for (selector, prop, r) in [
        ("island", "background", "background"),
        ("island", "border-color", "border"),
        ("time", "color", "foreground"),
        ("date", "color", "muted"),
        ("text", "color", "foreground"),
        ("title", "color", "title"),
        ("body", "color", "foreground"),
        ("label", "color", "muted"),
        ("value", "color", "foreground"),
        ("track", "background", "track"),
        ("fill", "background", "fill"),
        ("flash", "color", "title"),
        ("dnd", "color", "dnd"),
        ("tile", "background", "tile"),
        ("tile-on", "background", "tile-on"),
    ] {
        if !present.contains(&(selector.into(), prop.into())) {
            result.push_str(&format!("\n{selector} {{ {prop}: {}; }}\n", colors[r]));
        }
    }
    Ok(result)
}
