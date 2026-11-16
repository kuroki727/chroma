//! Read-only inspection. We never follow application includes or rewrite syntax.
use crate::{Context, Result, config::Paths, ensure, safe_fs};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

pub const APPLICATIONS: &[(&str, &str)] = &[
    ("waybar", "waybar/style.css"),
    ("fuzzel", "fuzzel/fuzzel.ini"),
    ("niri", "niri/config.kdl"),
    ("foot", "foot/foot.ini"),
    ("kitty", "kitty/kitty.conf"),
    ("quickshell", "quickshell/shell.qml"),
];

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    SetupRequired,
    Missing,
    Unsupported,
}
impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::SetupRequired => "setup-required",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}
#[derive(Debug)]
pub struct Detection {
    pub name: String,
    pub path: PathBuf,
    pub status: Status,
    pub reason: String,
}

pub fn check_root(root: &Path) -> Result<()> {
    ensure!(
        root.is_absolute()
            && !root
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "desktop root must be absolute and without traversal"
    );
    // Opening a directory walks every component with O_NOFOLLOW, even when an
    // application is missing. A missing root is a valid, empty desktop.
    match safe_fs::Dir::open(root, false) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("unsafe desktop root"),
    }
}

pub fn detect(paths: &Paths, root: &Path) -> Result<Vec<Detection>> {
    check_root(root)?;
    APPLICATIONS
        .iter()
        .map(|(name, relative)| {
            let path = root.join(relative);
            let result = inspect(paths, name, &path);
            let (status, reason) = match result {
                Ok(result) => result,
                Err(error) => (
                    Status::Unsupported,
                    format!("{error:#}; review manually; nothing followed or changed"),
                ),
            };
            Ok(Detection {
                name: (*name).into(),
                path,
                status,
                reason,
            })
        })
        .collect()
}

pub fn inspect(paths: &Paths, name: &str, path: &Path) -> Result<(Status, String)> {
    let Some(bytes) = safe_fs::read_optional(path)? else {
        if name == "waybar" {
            let parent = path.parent().context("stylesheet needs a parent")?;
            for alternative in [
                "style-light.css",
                "style-dark.css",
                "style.scss",
                "config.jsonc",
                "config",
            ] {
                if safe_fs::read_optional(&parent.join(alternative))?.is_some() {
                    return Ok((
                        Status::SetupRequired,
                        format!(
                            "Waybar files present ({alternative}), but no style.css; choose the active stylesheet explicitly; alternate/compiled stylesheet wiring is unsupported"
                        ),
                    ));
                }
            }
        }
        return Ok((
            Status::Missing,
            "no main configuration; create it explicitly before connecting a fragment".into(),
        ));
    };
    let source = std::str::from_utf8(&bytes).context("configuration is not UTF-8")?;
    ensure!(
        !source
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "control characters in configuration"
    );
    let fragment = paths.config.join("generated").join(match name {
        "waybar" => "waybar.css",
        "fuzzel" => "fuzzel.ini",
        _ => "unused",
    });
    let fragment = fragment.to_str().context("fragment path needs UTF-8")?;
    match name {
        "waybar" => waybar(source, fragment),
        "fuzzel" => fuzzel(source, fragment),
        _ => Ok((Status::SetupRequired, "native/custom template available; composition and installed version require manual validation; automatic wiring unsupported".into())),
    }
}

/// Mask CSS comments and strings for balanced structure checks. Restrict the
/// ready classification to a single exact native import and named references.
fn waybar(source: &str, fragment: &str) -> Result<(Status, String)> {
    ensure!(
        !fragment.contains(['"', '\\', '\n', '\r']),
        "fragment path cannot be expressed as a simple CSS import"
    );
    let mask = crate::integration::colors::css_mask(source)?;
    let expected = format!("@import \"{fragment}\";");
    let mut imports = 0;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for c in mask.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '{' => depth += 1,
            '}' => {
                ensure!(depth > 0, "unbalanced CSS braces");
                depth -= 1;
            }
            _ => (),
        }
    }
    ensure!(depth == 0, "unbalanced CSS braces");
    // No arbitrary CSS parser is implied. Unsupported at-rules and alternative
    // import spellings stay manual, even if accepted by GTK.
    for line in mask.lines() {
        let text = line.trim();
        if text.contains("@import") {
            ensure!(
                text == expected,
                "other/noncanonical CSS imports require manual composition review"
            );
            imports += 1;
        }
        ensure!(
            !text.contains("@define-color chroma_"),
            "local Chroma color definitions conflict with the fragment"
        );
    }
    ensure!(imports <= 1, "duplicate Chroma imports; keep exactly one");
    if imports == 1 && mask.contains("@chroma_") {
        Ok((Status::Ready, "one explicit import and named color references; CSS semantics/other literal colors remain user-owned".into()))
    } else {
        Ok((
            Status::SetupRequired,
            format!(
                "manually add {expected} once and choose @chroma_* references; existing literal colors are preserved; arbitrary CSS is never rewritten"
            ),
        ))
    }
}

/// A deliberately restricted INI inspector. Unknown keys are retained; repeated
/// sections, duplicate keys and include chains cannot establish safe composition.
pub fn ini(source: &str) -> Result<BTreeMap<(String, String), String>> {
    let mut section = "main".to_string();
    let mut sections = BTreeSet::new();
    let mut values = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        if text.starts_with('[') {
            ensure!(
                text.ends_with(']') && !text[1..text.len() - 1].contains(['[', ']']),
                "malformed INI section on line {}",
                index + 1
            );
            section = text[1..text.len() - 1].to_string();
            ensure!(
                !section.is_empty() && sections.insert(section.clone()),
                "duplicate/empty INI section {section}"
            );
        } else {
            let (key, value) = text
                .split_once('=')
                .with_context(|| format!("INI line {} needs key=value", index + 1))?;
            let key = key.trim();
            ensure!(!key.is_empty(), "empty INI key");
            ensure!(
                values
                    .insert((section.clone(), key.into()), value.trim().into())
                    .is_none(),
                "duplicate INI key {section}.{key}"
            );
        }
    }
    Ok(values)
}

fn fuzzel(source: &str, fragment: &str) -> Result<(Status, String)> {
    ensure!(!fragment.contains(['\n', '\r']), "unsafe INI include path");
    let values = ini(source)?;
    let include = values.get(&("main".into(), "include".into()));
    ensure!(
        include.is_none_or(|v| v == fragment),
        "other Fuzzel includes need manual composition review (not followed)"
    );
    ensure!(
        !values.keys().any(|(s, k)| s != "main" && k == "include"),
        "Fuzzel include must be in [main]"
    );
    for ((section, key), color) in &values {
        if section == "colors"
            && crate::adapters::FUZ_ALPHA.contains(&key.replace('-', "_").as_str())
        {
            ensure!(
                matches!(color.len(), 6 | 8) && color.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid Fuzzel color {key}; expected RRGGBB or RRGGBBAA"
            );
        }
    }
    let local_colors = values.keys().any(|(s, _)| s == "colors");
    if include.is_some() && !local_colors {
        Ok((Status::Ready, "explicit [main] include, no competing local [colors]; requires Fuzzel >=1.10; version not probed".into()))
    } else {
        Ok((
            Status::SetupRequired,
            format!(
                "manually set [main] include={fragment} once; move/remove local [colors] only after backup and review; fonts/geometry stay in main; requires Fuzzel >=1.10"
            ),
        ))
    }
}
