//! Only explicitly noninteractive validators. Never invoke the compositor,
//! lock screen, bar, or Island application without its validation subcommand.
use super::colors::Kind;
use crate::{Result, ensure};

pub fn method(kind: Kind) -> &'static str {
    match kind {
        Kind::Niri => {
            "Validation: niri validate -c TEMP_COMPOSED_CONFIG (required at bundle installation)"
        }
        Kind::Fuzzel => "Validation: fuzzel --check-config --config TEMP (when available)",
        Kind::Island => {
            "Validation: restricted literal parser; scripts/check-island-parser.cpp can test the actual source parser offline"
        }
        Kind::Waybar => {
            "Validation: restricted GTK CSS migration; no standalone Waybar validator; imports/complex CSS refused"
        }
        Kind::Kitty => {
            "Validation: known directive names plus strict color/ANSI syntax; native parser checked separately offline"
        }
        Kind::Swaylock => {
            "Validation: color syntax only; no safe installed validator; manual variant check required"
        }
    }
}

pub fn generated(kind: Kind, source: &[u8], generated: &[u8]) -> Result<()> {
    if !matches!(kind, Kind::Niri | Kind::Fuzzel) {
        return Ok(());
    }
    let program = if kind == Kind::Niri { "niri" } else { "fuzzel" };
    if !crate::adapters::reload::available(program) {
        ensure!(
            kind != Kind::Niri,
            "Niri validator unavailable; native include must be validated manually before installation"
        );
        return Ok(());
    }
    let temp = tempfile::tempdir()?;
    let file = temp.path().join("config");
    let mut argv = vec![program.to_string()];
    if kind == Kind::Niri {
        let colors = temp.path().join("colors.kdl");
        std::fs::write(&colors, generated)?;
        let source = std::str::from_utf8(source)?;
        // Relative includes would resolve differently in the temporary directory.
        // Refuse them instead of validating a different effective configuration.
        ensure!(
            !source
                .lines()
                .any(|l| l.trim_start().starts_with("include ")),
            "existing Niri includes require manual composed validation"
        );
        std::fs::write(
            &file,
            format!("{source}\ninclude \"{}\"\n", colors.display()),
        )?;
        argv.extend([
            "validate".into(),
            "-c".into(),
            file.to_string_lossy().into_owned(),
        ]);
    } else {
        std::fs::write(&file, generated)?;
        argv.extend([
            "--check-config".into(),
            "--config".into(),
            file.to_string_lossy().into_owned(),
            "--log-no-syslog".into(),
        ]);
    }
    crate::adapters::reload::run(&argv, 3000).map_err(|error| {
        anyhow::anyhow!("{program} isolated validation failed; no bundle installed: {error:#}")
    })?;
    Ok(())
}
