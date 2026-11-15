pub mod reload;
pub mod template;

use crate::{
    Context, Result,
    config::{Adapter, Paths},
    ensure,
    palette::Palette,
    safe_fs,
};
use std::path::PathBuf;

pub const BUILTINS: &[(&str, &str, &str)] = &[
    ("niri", "niri.kdl", include_str!("../../templates/niri.kdl")),
    (
        "waybar",
        "waybar.css",
        include_str!("../../templates/waybar.css"),
    ),
    (
        "fuzzel",
        "fuzzel.ini",
        include_str!("../../templates/fuzzel.ini"),
    ),
    (
        "kitty",
        "kitty.conf",
        include_str!("../../templates/kitty.conf"),
    ),
    ("foot", "foot.ini", include_str!("../../templates/foot.ini")),
    (
        "quickshell",
        "colors.json",
        include_str!("../../templates/colors.json"),
    ),
];

pub const FUZ_ALPHA: &[&str] = &[
    "background",
    "text",
    "prompt",
    "placeholder",
    "input",
    "match",
    "selection",
    "selection_text",
    "selection_match",
    "border",
];

pub struct Output {
    pub name: String,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub allow_overwrite: bool,
    pub reload: Vec<String>,
    pub timeout_ms: u64,
}

/// Resolve the destination without reading a template or requiring a palette.
/// Recovery must be able to reload restored files even if templates were removed.
pub fn output_path(name: &str, adapter: &Adapter, paths: &Paths) -> Result<PathBuf> {
    let path = match &adapter.output {
        Some(output) => paths.expand(output)?,
        None => paths.config.join("generated").join(
            BUILTINS
                .iter()
                .find(|(id, _, _)| *id == name)
                .with_context(|| format!("adapter {name}: custom adapters require an output"))?
                .1,
        ),
    };
    paths.check_output(&path)?;
    Ok(path)
}

pub fn render(name: &str, adapter: &Adapter, paths: &Paths, palette: &Palette) -> Result<Output> {
    if let Some(kind) = adapter.desktop {
        ensure!(
            adapter.template.is_none() && adapter.alpha.is_empty(),
            "desktop adapters cannot override templates or alpha"
        );
        let source = paths.expand(
            adapter
                .config_path
                .as_deref()
                .context("desktop adapter needs config_path")?,
        )?;
        let path = output_path(name, adapter, paths)?;
        ensure!(
            path.starts_with(paths.config.join("generated")) && path != source,
            "desktop adapters write only separate generated files"
        );
        let bytes = safe_fs::read_optional(&source)?.context("desktop source is missing")?;
        let rendered =
            crate::integration::colors::render(kind, std::str::from_utf8(&bytes)?, palette)?;
        return Ok(Output {
            name: name.into(),
            path,
            bytes: rendered.into_bytes(),
            allow_overwrite: adapter.allow_overwrite,
            reload: adapter.reload.clone(),
            timeout_ms: adapter.reload_timeout_ms.unwrap_or(3000),
        });
    }
    let builtin = BUILTINS.iter().find(|(id, _, _)| *id == name);
    let source = if let Some(file) = &adapter.template {
        let path = paths.expand(file)?;
        let bytes = safe_fs::read_optional(&path)?
            .with_context(|| format!("adapter {name}: missing template {}", path.display()))?;
        String::from_utf8(bytes).context("template must be UTF-8")?
    } else {
        let (_, filename, embedded) = builtin
            .with_context(|| format!("adapter {name}: custom adapters require a template"))?;
        let local = paths.config.join("templates").join(filename);
        match safe_fs::read_optional(&local)? {
            Some(bytes) => String::from_utf8(bytes).context("template must be UTF-8")?,
            None => embedded.to_string(),
        }
    };
    let path = output_path(name, adapter, paths)?;
    if let Some(config) = &adapter.config_path {
        ensure!(
            paths.expand(config)? != path,
            "adapter {name}: output must be separate from config_path"
        );
    }
    if let Some(file) = &adapter.template {
        ensure!(
            paths.expand(file)? != path,
            "adapter {name}: template and output must differ"
        );
    }
    let mut alpha = adapter.alpha.clone();
    // Disabled examples can be syntax-checked without choosing transparency.
    // An enabled adapter must supply every alpha byte used by its template.
    if name == "fuzzel" && !adapter.enabled {
        for key in FUZ_ALPHA {
            alpha.entry(key.to_string()).or_insert(255);
        }
    }
    Ok(Output {
        name: name.to_string(),
        path,
        bytes: template::render_with_alpha(&source, palette, &alpha)
            .with_context(|| format!("adapter {name}"))?
            .into_bytes(),
        allow_overwrite: adapter.allow_overwrite,
        reload: adapter.reload.clone(),
        timeout_ms: adapter.reload_timeout_ms.unwrap_or(3000),
    })
}
