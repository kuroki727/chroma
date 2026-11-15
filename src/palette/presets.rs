use crate::{Result, palette::Palette};
use std::collections::BTreeMap;

pub const SOURCES: &[(&str, &str)] = &[
    ("rustwood", include_str!("../../palettes/rustwood.toml")),
    (
        "gruvbox-dark",
        include_str!("../../palettes/gruvbox-dark.toml"),
    ),
    ("nord", include_str!("../../palettes/nord.toml")),
    (
        "catppuccin-mocha",
        include_str!("../../palettes/catppuccin-mocha.toml"),
    ),
    (
        "tokyo-night",
        include_str!("../../palettes/tokyo-night.toml"),
    ),
    ("paper", include_str!("../../palettes/paper.toml")),
];

pub fn all() -> Result<BTreeMap<String, Palette>> {
    SOURCES
        .iter()
        .map(|(id, source)| Ok((id.to_string(), Palette::parse(source)?)))
        .collect()
}

pub fn get(id: &str) -> Option<Palette> {
    SOURCES
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, source)| Palette::parse(source).expect("tested embedded palette"))
}
