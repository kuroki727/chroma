//! Restricted, lossless color edits. Unknown or ambiguous syntax is refused.
use crate::{Context, Result, ensure, palette::Palette};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Island,
    Waybar,
    Fuzzel,
    Niri,
    Kitty,
    Swaylock,
}

impl Kind {
    pub const ALL: [Self; 6] = [
        Self::Niri,
        Self::Waybar,
        Self::Fuzzel,
        Self::Kitty,
        Self::Island,
        Self::Swaylock,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Island => "island",
            Self::Waybar => "waybar",
            Self::Fuzzel => "fuzzel",
            Self::Niri => "niri",
            Self::Kitty => "kitty",
            Self::Swaylock => "swaylock",
        }
    }
    pub fn relative(self) -> &'static str {
        match self {
            Self::Island => "island/style.css",
            Self::Waybar => "waybar/style.css",
            Self::Fuzzel => "fuzzel/fuzzel.ini",
            Self::Niri => "niri/config.kdl",
            Self::Kitty => "kitty/kitty.conf",
            Self::Swaylock => "swaylock/config",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Island | Self::Waybar => "css",
            Self::Fuzzel => "ini",
            Self::Niri => "kdl",
            _ => "conf",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Edit {
    pub range: Range<usize>,
    pub role: String,
}

fn trimmed_range(source: &str, range: Range<usize>) -> Range<usize> {
    let text = &source[range.clone()];
    let start = range.start + text.len() - text.trim_start().len();
    start..start + text.trim().len()
}

/// Mask comments and check quoted CSS strings without changing byte offsets.
pub(crate) fn css_mask(source: &str) -> Result<String> {
    let mut out = source.as_bytes().to_vec();
    let mut i = 0;
    while i < out.len() {
        if source.as_bytes()[i..].starts_with(b"/*") {
            let end = source[i + 2..]
                .find("*/")
                .context("unterminated CSS comment")?
                + i
                + 4;
            for b in &mut out[i..end] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
            i = end;
        } else if matches!(out[i], b'\'' | b'"') {
            let quote = out[i];
            i += 1;
            let mut closed = false;
            while i < out.len() {
                if out[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if out[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            ensure!(closed, "unterminated CSS string");
        } else {
            // Work on bytes; only slice at ASCII structural markers below.
            i += 1;
        }
    }
    Ok(String::from_utf8(out)?)
}

fn island_role(selector: &str, prop: &str) -> Option<&'static str> {
    let selector = selector.trim().trim_start_matches(['.', '#']);
    match (selector, prop) {
        ("island" | "window", "background" | "background-color") => Some("background"),
        ("island" | "window", "border-color") => Some("border"),
        ("date" | "label", "color") => Some("muted"),
        ("title" | "flash" | "layout" | "dnd", "color") => Some("primary"),
        ("time" | "text" | "body" | "value", "color") => Some("foreground"),
        ("track" | "tile", "background" | "background-color") => Some("surface_alt"),
        ("fill" | "tile-on", "background" | "background-color") => Some("primary"),
        _ => None,
    }
}

fn waybar_role(selector: &str, prop: &str) -> &'static str {
    if selector.contains("critical") || selector.contains("urgent") {
        "error"
    } else if selector.contains("warning") {
        "warning"
    } else if prop.starts_with("border") {
        "primary"
    } else if prop.starts_with("background") {
        "background"
    } else if selector.contains("disconnected")
        || (selector.contains("workspaces")
            && !selector.contains("active")
            && !selector.contains("focused"))
    {
        "muted"
    } else {
        "foreground"
    }
}

fn css_edits(kind: Kind, source: &str) -> Result<Vec<Edit>> {
    let mask = css_mask(source)?;
    let mut edits = Vec::new();
    let mut pos = 0;
    while !mask[pos..].trim().is_empty() {
        let open = mask[pos..].find('{').context("CSS rule needs '{'")? + pos;
        let selectors = mask[pos..open].trim();
        ensure!(
            !selectors.is_empty() && !selectors.contains(['@', '}', ';']),
            "CSS imports, at-rules and nested rules require manual integration"
        );
        let close = mask[open + 1..].find('}').context("CSS rule needs '}'")? + open + 1;
        ensure!(
            !mask[open + 1..close].contains('{'),
            "nested CSS rules require manual integration"
        );
        let mut start = open + 1;
        for declaration in mask[open + 1..close].split(';') {
            let end = start + declaration.len();
            if !declaration.trim().is_empty() {
                let colon = declaration.find(':').context("invalid CSS declaration")? + start;
                let prop = mask[start..colon].trim();
                ensure!(!prop.is_empty(), "empty CSS property");
                let value = trimmed_range(&mask, colon + 1..end);
                let color_prop = matches!(
                    prop,
                    "color"
                        | "background"
                        | "background-color"
                        | "border-color"
                        | "border"
                        | "border-bottom"
                        | "border-top"
                        | "border-left"
                        | "border-right"
                );
                if color_prop {
                    let roles: BTreeSet<_> = selectors
                        .split(',')
                        .map(|sel| {
                            if kind == Kind::Island {
                                island_role(sel, prop)
                            } else {
                                Some(waybar_role(sel, prop))
                            }
                        })
                        .collect();
                    ensure!(
                        roles.len() == 1,
                        "selectors map to different palette roles; split rule manually"
                    );
                    if let Some(Some(role)) = roles.first() {
                        let text = &source[value.clone()];
                        if text != "transparent" && text != "none" && text != "inherit" {
                            let offset = if text.starts_with('#')
                                || text.starts_with("rgb(")
                                || text.starts_with("rgba(")
                            {
                                0
                            } else {
                                text.find('#').context(
                                    "unsupported color expression; expected literal hex/rgb/rgba",
                                )?
                            };
                            let range = value.start + offset..value.end;
                            color_template(&source[range.clone()], role, false)?;
                            edits.push(Edit {
                                range,
                                role: (*role).into(),
                            });
                        }
                    } else if kind == Kind::Island {
                        crate::bail!("unsupported Island selector/property: {selectors} / {prop}");
                    }
                }
            }
            start = end + 1;
        }
        pos = close + 1;
    }
    ensure!(!edits.is_empty(), "no supported CSS color literals");
    Ok(edits)
}

/// Template for exactly one literal; alpha spelling/bytes are retained.
fn color_template(value: &str, role: &str, raw: bool) -> Result<String> {
    if raw {
        ensure!(
            matches!(value.len(), 6 | 8) && value.bytes().all(|b| b.is_ascii_hexdigit()),
            "raw application color must be exactly 6 or 8 hex digits without a prefix"
        );
    }
    let hex = value.strip_prefix('#').unwrap_or(value);
    if (raw || value.starts_with('#'))
        && matches!(hex.len(), 3 | 6 | 8)
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        ensure!(
            !raw || hex.len() != 3,
            "raw application colors need 6 or 8 digits"
        );
        return Ok(format!(
            "{}{{{{ {role}.hex_raw }}}}{}",
            if raw { "" } else { "#" },
            if hex.len() == 8 { &hex[6..] } else { "" }
        ));
    }
    if value.starts_with("rgb(") || value.starts_with("rgba(") {
        ensure!(value.ends_with(')'), "invalid RGB color");
        let open = value.find('(').unwrap();
        let fields: Vec<_> = value[open + 1..value.len() - 1].split(',').collect();
        let alpha = value.starts_with("rgba");
        ensure!(
            fields.len() == if alpha { 4 } else { 3 },
            "invalid RGB component count"
        );
        for field in &fields[..3] {
            let n: f64 = field.trim().parse()?;
            ensure!(
                n.is_finite() && (0.0..=255.0).contains(&n),
                "invalid RGB component"
            );
        }
        if alpha {
            let n: f64 = fields[3].trim().parse()?;
            ensure!(n.is_finite() && (0.0..=1.0).contains(&n), "invalid alpha");
            return Ok(format!("rgba({{{{ {role}.rgb_raw }}}},{})", fields[3]));
        }
        return Ok(format!("{{{{ {role}.rgb }}}}"));
    }
    crate::bail!("unsupported or invalid color {value:?}")
}

fn line_role(kind: Kind, section: &str, key: &str) -> Option<String> {
    let role = match kind {
        Kind::Fuzzel if section == "colors" => match key {
            "background" => "background",
            "text" | "input" | "selection-text" => "foreground",
            "prompt" | "border" => "primary",
            "placeholder" => "muted",
            "match" | "selection-match" => "secondary",
            "selection" => "surface",
            _ => return None,
        },
        Kind::Kitty => match key {
            "background" => "background",
            "foreground" | "selection_foreground" => "foreground",
            "cursor" => "primary",
            "cursor_text_color" => "background",
            "selection_background" => "surface_alt",
            "active_border_color" => "primary",
            "inactive_border_color" => "border",
            _ => {
                let index: u8 = key.strip_prefix("color")?.parse().ok()?;
                return (index < 16).then(|| format!("ansi{index}"));
            }
        },
        Kind::Swaylock => match key {
            "color" | "inside-color" | "inside-clear-color" | "inside-ver-color"
            | "inside-wrong-color" => "background",
            "ring-color" => "border",
            "ring-clear-color" | "bs-hl-color" => "warning",
            "ring-ver-color" => "success",
            "ring-wrong-color" => "error",
            "text-color" | "text-clear-color" | "text-ver-color" | "text-wrong-color" => {
                "foreground"
            }
            "key-hl-color" => "primary",
            _ => return None,
        },
        _ => return None,
    };
    Some(role.into())
}

fn line_edits(kind: Kind, source: &str) -> Result<Vec<Edit>> {
    if kind == Kind::Fuzzel {
        let ini = crate::adapters::discovery::ini(source)?;
        ensure!(
            !ini.keys().any(|(_, key)| key == "include"),
            "Fuzzel include chains require manual composition; no files followed"
        );
    }

    let mut section = "";
    let mut offset = 0;
    let mut found = BTreeSet::new();
    let mut edits = Vec::new();
    for line in source.split_inclusive('\n') {
        let text = line.trim();
        if text.starts_with('[') && text.ends_with(']') {
            section = &text[1..text.len() - 1];
        }
        if !text.starts_with('#') {
            let separator = if kind == Kind::Kitty {
                line.find(char::is_whitespace)
            } else {
                line.find('=')
            };
            if let Some(split) = separator {
                let key = line[..split].trim();
                if let Some(role) = line_role(kind, section, key) {
                    ensure!(
                        found.insert(key.to_string()),
                        "duplicate color key {key}; manual integration required"
                    );
                    let range = trimmed_range(source, offset + split + 1..offset + line.len());
                    color_template(&source[range.clone()], &role, kind != Kind::Kitty)?;
                    edits.push(Edit { range, role });
                }
            }
        }
        offset += line.len();
    }
    ensure!(!edits.is_empty(), "no recognized color keys");
    if kind == Kind::Fuzzel {
        ensure!(
            edits.len() == 10,
            "all ten Fuzzel color properties are required"
        );
    }
    if kind == Kind::Kitty {
        for i in 0..16 {
            ensure!(
                found.contains(&format!("color{i}")),
                "missing Kitty color{i}"
            );
        }
    }
    Ok(edits)
}

pub fn edits(kind: Kind, source: &str) -> Result<Vec<Edit>> {
    match kind {
        Kind::Island | Kind::Waybar => css_edits(kind, source),
        Kind::Niri => crate::bail!(
            "Niri uses a native generated include; arbitrary KDL editing is unsupported"
        ),
        _ => line_edits(kind, source),
    }
}

fn replace(
    source: &str,
    edits: &[Edit],
    mut value: impl FnMut(&Edit) -> Result<String>,
) -> Result<String> {
    let mut result = String::new();
    let mut pos = 0;
    for edit in edits {
        ensure!(
            edit.range.start >= pos && edit.range.end <= source.len(),
            "invalid color edit range"
        );
        result.push_str(&source[pos..edit.range.start]);
        result.push_str(&value(edit)?);
        pos = edit.range.end;
    }
    result.push_str(&source[pos..]);
    Ok(result)
}

pub fn recolor(kind: Kind, source: &str, palette: &Palette) -> Result<String> {
    palette.validate()?;
    let edits = edits(kind, source)?;
    // Render each color separately: braces in unrelated source remain literal.
    replace(source, &edits, |e| {
        crate::adapters::template::render(
            &color_template(
                &source[e.range.clone()],
                &e.role,
                matches!(kind, Kind::Fuzzel | Kind::Swaylock),
            )?,
            palette,
        )
    })
}

pub fn waybar_migration(source: &str, import: &str) -> Result<String> {
    ensure!(
        !import.contains(['"', '\\', '\n', '\r']),
        "unsafe CSS import path"
    );
    let edits = edits(Kind::Waybar, source)?;
    Ok(format!(
        "@import \"{import}\";\n{}",
        replace(source, &edits, |e| {
            // Preserve per-declaration alpha using GTK alpha(), never CSS custom properties.
            let original = &source[e.range.clone()];
            let template = color_template(original, &e.role, false)?;
            if original.starts_with("rgba(") {
                let alpha = original
                    .rsplit_once(',')
                    .unwrap()
                    .1
                    .trim_end_matches(')')
                    .trim();
                Ok(format!("alpha(@chroma_{}, {alpha})", e.role))
            } else if original.starts_with('#') && original.len() == 9 {
                let alpha = u8::from_str_radix(&original[7..], 16)?;
                Ok(format!(
                    "alpha(@chroma_{}, {:.9})",
                    e.role,
                    f64::from(alpha) / 255.0
                ))
            } else {
                let _ = template;
                Ok(format!("@chroma_{}", e.role))
            }
        })?
    ))
}

/// A zero-context unified diff: the migration inserts one import, then replaces
/// color tokens within existing lines. Refuse multiline colors for this preview.
pub fn waybar_diff(source: &str, migrated: &str, path: &str) -> Result<String> {
    let old: Vec<_> = source.lines().collect();
    let new: Vec<_> = migrated.lines().collect();
    ensure!(
        new.len() == old.len() + 1,
        "multiline color migration needs manual diff review"
    );
    let mut diff = format!(
        "--- {path}\n+++ {path} (proposed only)\n@@ -0,0 +1 @@\n+{}\n",
        new[0]
    );
    for (index, (before, after)) in old.iter().zip(&new[1..]).enumerate() {
        if before != after {
            diff.push_str(&format!(
                "@@ -{} +{} @@\n-{before}\n+{after}\n",
                index + 1,
                index + 2
            ));
        }
    }
    Ok(diff)
}

pub fn render(kind: Kind, source: &str, palette: &Palette) -> Result<String> {
    match kind {
        Kind::Niri => {
            crate::adapters::template::render(include_str!("../../templates/niri.kdl"), palette)
        }
        Kind::Waybar => {
            // After manual migration the main stylesheet contains @import and
            // named references. Definitions do not depend on its syntax.
            crate::adapters::template::render(include_str!("../../templates/waybar.css"), palette)
        }
        Kind::Kitty => {
            let edits = edits(kind, source)?;
            // Native include contains only recognized colors; unrelated options stay in main config.
            let mut result = String::from("# Chroma colors; include last in kitty.conf\n");
            for e in edits {
                let line_start = source[..e.range.start].rfind('\n').map_or(0, |n| n + 1);
                let key = source[line_start..e.range.start].trim();
                let value = crate::adapters::template::render(
                    &color_template(&source[e.range], &e.role, false)?,
                    palette,
                )?;
                result.push_str(&format!("{key} {value}\n"));
            }
            Ok(result)
        }
        _ => recolor(kind, source, palette),
    }
}
