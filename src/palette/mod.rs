pub mod colors;
pub mod generator;
pub mod presets;
mod sampling;

use crate::{Context, Result, ensure, safe_fs};
use colors::Color;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub foreground: Color,
    pub muted: Color,
    pub primary: Color,
    pub secondary: Color,
    pub border: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
}

impl Colors {
    pub fn named(&self) -> BTreeMap<String, Color> {
        [
            ("background", self.background),
            ("surface", self.surface),
            ("surface_alt", self.surface_alt),
            ("foreground", self.foreground),
            ("muted", self.muted),
            ("primary", self.primary),
            ("secondary", self.secondary),
            ("border", self.border),
            ("error", self.error),
            ("warning", self.warning),
            ("success", self.success),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub colors: Colors,
    /// Missing entries inherit deterministic semantic defaults.
    #[serde(default)]
    pub ansi: BTreeMap<String, Color>,
}

impl Palette {
    pub fn parse(text: &str) -> Result<Self> {
        let palette: Self = toml::from_str(text).context("invalid palette TOML")?;
        palette.validate()?;
        Ok(palette)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 120
                && !self.name.chars().any(char::is_control),
            "palette name must be printable, 1–120 bytes"
        );
        ensure!(
            self.ansi
                .keys()
                .all(|k| (0..16).any(|i| i.to_string() == *k)),
            "ANSI color indices must be 0–15"
        );
        ensure!(
            self.colors.foreground.contrast(self.colors.background) >= 4.5,
            "foreground/background contrast must be at least 4.5:1"
        );
        Ok(())
    }

    pub fn variables(&self) -> BTreeMap<String, Color> {
        let mut result = self.colors.named();
        let c = &self.colors;
        let hue = c.primary.hue();
        let light = c.background.luminance() > 0.4;
        let blue = Color::lch(if light { 0.45 } else { 0.72 }, 0.12, 260.0);
        let magenta = Color::lch(if light { 0.45 } else { 0.72 }, 0.12, hue + 60.0);
        let cyan = Color::lch(if light { 0.45 } else { 0.72 }, 0.1, 195.0);
        let base = [
            c.background,
            c.error,
            c.success,
            c.warning,
            blue,
            magenta,
            cyan,
            c.foreground,
        ];
        for i in 0..16_u8 {
            let fallback = if i < 8 {
                base[usize::from(i)]
            } else {
                base[usize::from(i - 8)].mix(
                    if light {
                        Color(0, 0, 0)
                    } else {
                        Color(255, 255, 255)
                    },
                    0.18,
                )
            };
            result.insert(
                format!("ansi{i}"),
                self.ansi.get(&i.to_string()).copied().unwrap_or(fallback),
            );
        }
        result
    }
}

pub fn valid_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 80
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'),
        "palette ID must contain 1–80 lowercase letters, digits, '-' or '_'"
    );
    Ok(())
}

pub fn load(config_dir: &Path, id: &str) -> Result<Palette> {
    valid_id(id)?;
    let path = config_dir.join("palettes").join(format!("{id}.toml"));
    if let Some(text) = safe_fs::read_optional(&path)? {
        return Palette::parse(std::str::from_utf8(&text)?)
            .with_context(|| path.display().to_string());
    }
    presets::get(id).with_context(|| format!("palette {id:?} not found"))
}

pub fn list(config_dir: &Path) -> Result<BTreeMap<String, Palette>> {
    let mut palettes = presets::all()?;
    let dir = config_dir.join("palettes");
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "toml") {
                    let id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .context("non-UTF8 palette filename")?;
                    valid_id(id)?;
                    palettes.insert(id.to_string(), load(config_dir, id)?);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("reading palette directory"),
    }
    Ok(palettes)
}
