//! Minimal native Niri connection: preserve the complete source, append one include.
use crate::{Result, ensure};
use std::path::{Component, Path};

pub fn candidate(source: &str, fragment: &Path) -> Result<String> {
    ensure!(
        !super::validate::kdl_has_include(source)?,
        "existing Niri includes require manual composed validation"
    );
    ensure!(
        fragment.is_absolute()
            && fragment
                .components()
                .all(|c| matches!(c, Component::RootDir | Component::Normal(_))),
        "Niri fragment requires an absolute path without traversal"
    );
    let path = fragment
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 include path"))?;
    ensure!(
        !path
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\\')),
        "unsafe Niri include path"
    );
    // Always terminate a trailing line comment. No source byte is removed or moved.
    Ok(format!("{source}\ninclude \"{path}\"\n"))
}
