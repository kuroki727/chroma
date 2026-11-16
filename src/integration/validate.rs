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
    if kind == Kind::Niri {
        ensure!(
            !kdl_has_include(std::str::from_utf8(source)?)?,
            "existing Niri includes require manual composed validation"
        );
    } else {
        let ini = crate::adapters::discovery::ini(std::str::from_utf8(generated)?)?;
        ensure!(
            !ini.keys().any(|(_, key)| key == "include"),
            "Fuzzel include chains require manual validation; no external files followed"
        );
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

/// Identify bare include tokens outside KDL comments/strings. Include spellings
/// separated by tabs, newlines or comments must not escape offline validation.
fn kdl_has_include(source: &str) -> Result<bool> {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            i += source[i..].find('\n').unwrap_or(bytes.len() - i);
        } else if bytes[i..].starts_with(b"/*") {
            let mut depth = 1;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            ensure!(depth == 0, "unterminated KDL comment");
        } else if bytes[i] == b'"' {
            ensure!(
                i == 0 || bytes[i - 1] != b'#',
                "raw KDL strings require manual composed validation"
            );
            ensure!(
                !bytes[i..].starts_with(b"\"\"\""),
                "multiline KDL strings require manual composed validation"
            );
            i += 1;
            let start = i;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    crate::bail!("escaped KDL strings require manual composed validation");
                } else if bytes[i] == b'"' {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            ensure!(closed, "unterminated KDL string");
            if &bytes[start..i - 1] == b"include" {
                return Ok(true);
            }
        } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if &bytes[start..i] == b"include" {
                return Ok(true);
            }
        } else {
            i += 1;
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_validation_refuses_include_chains_before_invoking_parsers() {
        for source in [
            "include\t\"/untrusted/config\"",
            "include/*comment*/\"/untrusted/config\"",
            "include\n\"/untrusted/config\"",
            "\"include\" \"/untrusted/config\"",
        ] {
            assert!(
                generated(Kind::Niri, source.as_bytes(), b"layout {}\n")
                    .unwrap_err()
                    .to_string()
                    .contains("includes require manual")
            );
        }
        assert!(
            !kdl_has_include(
                "// include \"ignored\"\nlayout {} /* include ignored */\nbinds { spawn \"foot\"; }"
            )
            .unwrap()
        );
        assert!(
            generated(Kind::Fuzzel, b"", b"[main]\ninclude=/untrusted/config\n")
                .unwrap_err()
                .to_string()
                .contains("include chains")
        );
    }
}
