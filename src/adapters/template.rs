use crate::{Result, bail, ensure, palette::Palette};

/// Only color substitutions are supported. No expressions, includes or code.
/// `{{{{` and `}}}}` escape literal double braces.
pub fn render(source: &str, palette: &Palette) -> Result<String> {
    render_with_alpha(source, palette, &std::collections::BTreeMap::new())
}

pub fn render_with_alpha(
    source: &str,
    palette: &Palette,
    alpha: &std::collections::BTreeMap<String, u8>,
) -> Result<String> {
    let variables = palette.variables();
    let mut result = String::with_capacity(source.len());
    let mut rest = source;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("{{{{") {
            result.push_str("{{");
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("}}}}") {
            result.push_str("}}");
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("{{") {
            let offset = source.len() - rest.len();
            let line = source[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
            let end = tail
                .find("}}")
                .ok_or_else(|| anyhow::anyhow!("template line {line}: unclosed variable"))?;
            let token = tail[..end].trim();
            let mut parts = token.split('.');
            let name = parts.next().unwrap_or_default();
            let format = parts.next().unwrap_or("hex");
            ensure!(
                parts.next().is_none(),
                "template line {line}: invalid variable {token:?}"
            );
            if name == "alpha" {
                let value = alpha.get(format).ok_or_else(|| anyhow::anyhow!("template line {line}: missing local alpha.{format}; copy the existing application's alpha byte into [adapters.NAME.alpha] to preserve opacity"))?;
                result.push_str(&format!("{value:02X}"));
                rest = &tail[end + 2..];
                continue;
            }
            let c = variables.get(name).ok_or_else(|| {
                anyhow::anyhow!("template line {line}: unknown variable {name:?}")
            })?;
            result.push_str(&match format {
                "hex" => c.to_string(),
                "hex_raw" => c.to_string()[1..].to_string(),
                "rgb" => format!("rgb({}, {}, {})", c.0, c.1, c.2),
                "rgb_raw" => format!("{}, {}, {}", c.0, c.1, c.2),
                "r" => c.0.to_string(),
                "g" => c.1.to_string(),
                "b" => c.2.to_string(),
                _ => bail!("template line {line}: unknown color format {format:?}"),
            });
            rest = &tail[end + 2..];
        } else if rest.starts_with("}}") {
            bail!("template: unexpected closing braces; escape literal braces as }}}}}}}}");
        } else {
            let ch = rest.chars().next().expect("nonempty remainder");
            result.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    Ok(result)
}
