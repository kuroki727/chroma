//! Review bundles are separate from palette transactions. No command here edits
//! desktop configuration, enables an adapter, or executes a reload.
pub mod colors;
pub mod niri;
pub mod validate;

use crate::{
    Context, Result,
    config::{Adapter, Paths},
    ensure,
    safe_fs::{self, Dir, Snapshot},
};
use colors::Kind;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<Snapshot>,
    pub after: Snapshot,
}

#[derive(Default, Debug)]
pub struct Plan {
    pub changes: Vec<Change>,
    pub reports: Vec<String>,
    /// Inputs checked again immediately before committing the bundle.
    pub inputs: Vec<(PathBuf, Vec<u8>)>,
    pub guard: safe_fs::Inputs,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    complete: bool,
    restoring: bool,
    changes: Vec<Change>,
}

fn journal_path(paths: &Paths) -> PathBuf {
    paths.state.join("integration.toml")
}

fn load(paths: &Paths) -> Result<Option<Journal>> {
    let Some(bytes) = safe_fs::read_private_optional(&journal_path(paths))? else {
        return Ok(None);
    };
    let journal: Journal = toml::from_str(std::str::from_utf8(&bytes)?)?;
    ensure!(
        journal.version == 1 && !journal.changes.is_empty(),
        "invalid integration journal"
    );
    let mut destinations = BTreeSet::new();
    for entry in &journal.changes {
        check_artifact(paths, &entry.path)?;
        ensure!(
            destinations.insert(&entry.path),
            "duplicate integration destination"
        );
        ensure!(
            entry.before.is_none() && entry.after.mode == 0o600,
            "invalid integration creation snapshot"
        );
        entry.after.validate()?;
    }
    Ok(Some(journal))
}

fn check_artifact(paths: &Paths, path: &Path) -> Result<()> {
    let allowed = Kind::ALL.into_iter().any(|kind| {
        path == output(paths, kind)
            || path == adapter_path(paths, kind)
            || path
                == paths.config.join("generated").join(format!(
                    "preview-{}.{}",
                    kind.id(),
                    kind.extension()
                ))
    });
    ensure!(
        allowed && !path.starts_with(&paths.state),
        "invalid integration artifact {}",
        path.display()
    );
    Ok(())
}

pub fn output(paths: &Paths, kind: Kind) -> PathBuf {
    paths
        .config
        .join("generated")
        .join(format!("desktop-{}.{}", kind.id(), kind.extension()))
}

fn adapter_path(paths: &Paths, kind: Kind) -> PathBuf {
    paths
        .config
        .join("adapters")
        .join(format!("desktop-{}.toml", kind.id()))
}

fn add(plan: &mut Plan, path: PathBuf, bytes: Vec<u8>) -> Result<()> {
    // A bundle never takes ownership of an existing unrelated file.
    let before = match Dir::parent(&path) {
        Ok((dir, file)) => {
            dir.check_target(&file)?;
            dir.read(&file)?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    ensure!(
        before.is_none(),
        "existing bundle artifact {}; use --status or --rollback; no overwrite",
        path.display()
    );
    plan.changes.push(Change {
        path,
        before,
        after: Snapshot { bytes, mode: 0o600 },
    });
    Ok(())
}

pub fn plan(paths: &Paths, desktop: &Path, palette: &crate::palette::Palette) -> Result<Plan> {
    crate::adapters::discovery::check_root(desktop)?;
    ensure!(
        crate::engine::State::load(&paths.state)?.pending.is_none(),
        "pending palette transaction; run chroma rollback first"
    );
    let config = crate::config::Config::load(paths)?;
    let mut plan = Plan {
        guard: config.inputs.clone(),
        ..Plan::default()
    };
    if let Some(journal) = load(paths)? {
        ensure!(
            journal.complete && !journal.restoring,
            "interrupted integration; run chroma integrate --rollback"
        );
        plan.reports
            .push("Integration bundle already installed; use --status. No changes planned.".into());
        return Ok(plan);
    }
    for kind in Kind::ALL {
        let source_path = desktop.join(kind.relative());
        let Some(bytes) = plan.guard.read(&source_path)? else {
            plan.reports.push(format!(
                "{}: missing {}; skipped",
                kind.id(),
                source_path.display()
            ));
            continue;
        };
        plan.inputs.push((source_path.clone(), bytes.clone()));
        let rendered = (|| -> Result<(Vec<u8>, Option<Vec<u8>>)> {
            let source = std::str::from_utf8(&bytes)?;
            if kind == Kind::Swaylock {
                ensure!(
                    !source.lines().any(
                        |l| l.trim_start().starts_with("exec ") || l.trim_end().ends_with('\\')
                    ),
                    "exec/shell continuation is not swaylock-effects config syntax; correct it manually first (nothing executed)"
                );
            }
            let rendered = colors::render(kind, source, palette)?;
            let preview = match kind {
                Kind::Waybar => Some(
                    colors::waybar_migration(
                        source,
                        output(paths, kind)
                            .to_str()
                            .context("non-UTF8 import path")?,
                    )?
                    .into_bytes(),
                ),
                Kind::Niri => {
                    // Includes are positional. Keep all original settings, append colors only.
                    let path = output(paths, kind);
                    let path = path.to_str().context("non-UTF8 include path")?;
                    ensure!(
                        !path.contains(['"', '\\', '\n', '\r']),
                        "unsafe KDL include path"
                    );
                    Some(format!("{source}\n// Chroma colors (Niri >=25.11); include last\ninclude \"{path}\"\n").into_bytes())
                }
                Kind::Kitty => {
                    let path = output(paths, kind);
                    let path = path.to_str().context("non-UTF8 include path")?;
                    ensure!(
                        !path.chars().any(char::is_whitespace)
                            && !path.contains(['*', '?', '[', ']', '\\']),
                        "Kitty include path requires manual escaping"
                    );
                    Some(
                        format!("{source}\n# Chroma colors; include last\ninclude {path}\n")
                            .into_bytes(),
                    )
                }
                _ => None,
            };
            Ok((rendered.into_bytes(), preview))
        })();
        let (rendered, preview) = match rendered {
            Ok(result) => result,
            Err(error) => {
                plan.reports
                    .push(format!("{}: unsupported/manual: {error:#}", kind.id()));
                continue;
            }
        };
        // A named adapter must not shadow a main-config entry.
        ensure!(
            !config
                .adapters
                .contains_key(&format!("desktop-{}", kind.id())),
            "desktop adapter already configured; review manually"
        );
        let adapter = Adapter {
            desktop: Some(kind),
            config_path: Some(source_path.to_str().context("non-UTF8 config path")?.into()),
            output: Some(
                output(paths, kind)
                    .to_str()
                    .context("non-UTF8 output path")?
                    .into(),
            ),
            ..Default::default()
        };
        add(&mut plan, output(paths, kind), rendered)?;
        add(
            &mut plan,
            adapter_path(paths, kind),
            toml::to_string_pretty(&adapter)?.into_bytes(),
        )?;
        if let Some(preview) = preview {
            if kind == Kind::Waybar {
                plan.reports.push(colors::waybar_diff(
                    std::str::from_utf8(&bytes)?,
                    std::str::from_utf8(&preview)?,
                    &source_path.display().to_string(),
                )?);
            }
            add(
                &mut plan,
                paths.config.join("generated").join(format!(
                    "preview-{}.{}",
                    kind.id(),
                    kind.extension()
                )),
                preview,
            )?;
        }
        plan.reports.push(format!(
            "{}: detected {} -> {}\n  adapter disabled; backup journal required; {}\n  {}",
            kind.id(),
            source_path.display(),
            output(paths, kind).display(),
            validate::method(kind),
            activation(kind)
        ));
    }
    plan.reports.push(format!("GTK: {}: manual/application-specific CSS only; no universal GTK/libadwaita recoloring. Qt6ct: {}: generator deferred (KDE .colors and upstream QPalette formats differ); active selection and existing schemes are never changed.", desktop.join("gtk-3.0").display(), desktop.join("qt6ct/qt6ct.conf").display()));
    plan.reports.push("--apply installs a review bundle only, with private recovery journal. It does not wire desktop configs or enable adapters. Native includes and Island -s require manual setup. No reload commands run. Niri watches includes once wired; future palette writes then live-reload colors automatically.".into());
    Ok(plan)
}

fn activation(kind: Kind) -> &'static str {
    match kind {
        Kind::Niri => {
            "Manual: review preview-niri.kdl and append its final native include; niri validate on a temporary composed copy first. Never restart Niri."
        }
        Kind::Kitty => {
            "Manual: review preview-kitty.conf and append its final native include. New Kitty processes use it; remote reload requires separate trusted authorization."
        }
        Kind::Waybar => {
            "Manual: review preview-waybar.css; only color literals change to GTK @chroma_* references plus @import. Back up style.css before accepting the migration; config.jsonc stays untouched."
        }
        Kind::Island => {
            "Manual: use island -s GENERATED_STYLESHEET on the next normal launch. No CSS imports exist in the inspected parser. config.jsonc stays untouched. Optional trusted reload argv: [\"island\", \"ctl\", \"reload\"], disabled by default."
        }
        Kind::Fuzzel => {
            "Manual: use fuzzel --config=GENERATED_INI, or review a native colors include for your version. Generated copy preserves all ten alpha bytes and non-color settings."
        }
        Kind::Swaylock => {
            "Manual: use the generated config only after checking the installed variant. No validator/launcher is executed; lock/idle commands are never run."
        }
    }
}

// Preview retains exact artifact bytes in the plan, but never emits source
// control sequences to a terminal (e.g. OSC clipboard writes in CSS comments).
fn terminal_safe(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() && !matches!(c, '\n' | '\t') {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn print(plan: &Plan) {
    for report in &plan.reports {
        println!("{}", terminal_safe(report));
    }
    for change in &plan.changes {
        println!(
            "\n+++ proposed {} (new, private file; {} bytes)\n{}",
            change.path.display(),
            change.after.bytes.len(),
            terminal_safe(&String::from_utf8_lossy(&change.after.bytes))
        );
    }
    println!(
        "\nRead-only plan: {} proposed files; no files, adapters, reloads or state changed.",
        plan.changes.len()
    );
}

pub fn status(paths: &Paths) -> Result<()> {
    match load(paths)? {
        None => println!("No integration bundle installed. Run chroma integrate --plan."),
        Some(journal) => {
            println!(
                "Bundle: {}; adapters must be enabled manually",
                if journal.complete && !journal.restoring {
                    "installed"
                } else {
                    "interrupted; run integrate --rollback"
                }
            );
            for entry in journal.changes {
                let current = match Dir::parent(&entry.path) {
                    Ok((dir, name)) => dir.read(&name)?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.into()),
                };
                println!(
                    "{}: {}",
                    entry.path.display(),
                    if current.as_ref() == Some(&entry.after) {
                        "unchanged"
                    } else {
                        "changed/missing; rollback will preserve it"
                    }
                );
            }
        }
    }
    Ok(())
}

/// Initial palette ownership applies only to an exact, completed bundle snapshot.
pub fn owns(paths: &Paths, path: &Path, actual: Option<&Snapshot>) -> Result<bool> {
    Ok(load(paths)?.is_some_and(|j| {
        j.complete
            && !j.restoring
            && j.changes
                .iter()
                .any(|e| e.path == path && Some(&e.after) == actual)
    }))
}

pub fn ensure_ready(paths: &Paths) -> Result<()> {
    if let Some(journal) = load(paths)? {
        ensure!(
            journal.complete && !journal.restoring,
            "interrupted integration; run chroma integrate --rollback before applying palettes"
        );
    }
    Ok(())
}

pub fn apply(paths: &Paths, plan: Plan, confirmed: bool) -> Result<usize> {
    ensure!(
        confirmed,
        "integration requires explicit --confirm after reviewing --plan; installs disabled review artifacts only"
    );
    if plan.changes.is_empty() {
        return Ok(0);
    }
    let state_dir = Dir::open(&paths.state, true)?;
    let _lock = state_dir.lock()?;
    ensure!(
        load(paths)?.is_none(),
        "integration already exists; use --status or --rollback"
    );
    plan.guard
        .check()
        .context("source changed or configuration changed during planning; plan again")?;
    ensure!(
        crate::engine::State::load(&paths.state)?.pending.is_none(),
        "pending palette transaction; run chroma rollback first"
    );
    for (path, expected) in &plan.inputs {
        ensure!(
            safe_fs::read_optional(path)?.as_deref() == Some(expected),
            "source changed during planning: {}; plan again",
            path.display()
        );
    }
    for entry in &plan.changes {
        check_artifact(paths, &entry.path)?;
    }
    for kind in Kind::ALL {
        if let Some(entry) = plan.changes.iter().find(|e| e.path == output(paths, kind)) {
            let source = plan
                .inputs
                .iter()
                .find(|(p, _)| p.ends_with(kind.relative()))
                .map(|(_, b)| b.as_slice())
                .unwrap_or_default();
            validate::generated(kind, source, &entry.after.bytes)?;
        }
    }
    // Native validation may take seconds; recheck its inputs before any journal.
    plan.guard.check()?;
    state_dir.check_path(&paths.state)?;
    let journal = Journal {
        version: 1,
        complete: false,
        restoring: false,
        changes: plan.changes,
    };
    crate::engine::backup::save(&journal_path(paths), &journal, false)?;
    // Journal is durable before any artifact. On interruption --rollback is retryable.
    write_bundle(paths, journal, |_| Ok(())).context(
        "integration installation incomplete; run chroma integrate --rollback; journal retained",
    )
}

fn write_bundle(
    paths: &Paths,
    mut journal: Journal,
    after_write: impl Fn(usize) -> Result<()>,
) -> Result<usize> {
    for (index, entry) in journal.changes.iter().enumerate() {
        Dir::open(
            entry.path.parent().context("missing artifact parent")?,
            true,
        )?;
        let (dir, name) = Dir::parent(&entry.path)?;
        dir.stage(&entry.after.bytes, entry.after.mode)?
            .commit_checked(&name, entry.before.as_ref())?;
        after_write(index + 1)?;
    }
    journal.complete = true;
    crate::engine::backup::save(&journal_path(paths), &journal, true)?;
    Ok(journal.changes.len())
}

pub fn rollback(paths: &Paths) -> Result<()> {
    let state_dir = Dir::open(&paths.state, false).context("no integration to roll back")?;
    let _lock = state_dir.lock()?;
    ensure!(
        crate::engine::State::load(&paths.state)?.pending.is_none(),
        "pending palette transaction; run chroma rollback first"
    );
    let mut journal = load(paths)?.context("no integration to roll back")?;
    journal.restoring = true;
    crate::engine::backup::save(&journal_path(paths), &journal, true)?;
    let mut conflicts = Vec::new();
    for entry in journal.changes.iter().rev() {
        let result = (|| -> Result<()> {
            let (directory, actual) = match Dir::parent(&entry.path) {
                Ok((dir, name)) => {
                    dir.check_target(&name)?;
                    let actual = dir.read(&name)?;
                    (Some((dir, name)), actual)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (None, None),
                Err(e) => return Err(e.into()),
            };
            if actual == entry.before {
                return Ok(());
            }
            ensure!(
                actual.as_ref() == Some(&entry.after),
                "changed outside integration; preserve it and resolve manually (palette rollback may be needed first)"
            );
            let (dir, name) = directory.context("artifact directory disappeared")?;
            dir.check_path(entry.path.parent().context("missing artifact parent")?)?;
            if let Some(before) = &entry.before {
                dir.stage(&before.bytes, before.mode)?
                    .commit_checked(&name, Some(&entry.after))?;
            } else {
                dir.remove(&name)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            conflicts.push(format!("{}: {error:#}", entry.path.display()));
        }
    }
    ensure!(
        conflicts.is_empty(),
        "integration rollback incomplete; journal retained:\n{}",
        conflicts.join("\n")
    );
    let (dir, name) = Dir::parent(&journal_path(paths))?;
    dir.remove(&name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_escapes_source_terminal_sequences_without_changing_artifacts() {
        let source = "/* \u{1b}]52;c;dGVzdA==\u{7} */\nisland { background: #123456; }";
        let rendered = colors::render(
            Kind::Island,
            source,
            &crate::palette::presets::get("nord").unwrap(),
        )
        .unwrap();
        assert!(rendered.contains('\u{1b}'));
        let preview = terminal_safe(&rendered);
        assert!(!preview.contains(['\u{1b}', '\u{7}']));
        assert!(preview.contains("\\u{1b}"));
    }

    #[test]
    fn interrupted_bundle_is_recoverable_at_every_write_boundary() {
        for stop in 1..=3 {
            let root = tempfile::tempdir().unwrap();
            let paths = Paths::discover(Some(root.path().join("chroma")), None).unwrap();
            Dir::open(&paths.state, true).unwrap();
            let changes = [Kind::Island, Kind::Kitty, Kind::Waybar]
                .into_iter()
                .map(|kind| Change {
                    path: output(&paths, kind),
                    before: None,
                    after: Snapshot {
                        bytes: b"generated".to_vec(),
                        mode: 0o600,
                    },
                })
                .collect();
            let journal = Journal {
                version: 1,
                complete: false,
                restoring: false,
                changes,
            };
            crate::engine::backup::save(&journal_path(&paths), &journal, false).unwrap();
            let error = write_bundle(&paths, journal, |count| {
                ensure!(count != stop, "simulated power loss");
                Ok(())
            })
            .unwrap_err();
            assert!(error.to_string().contains("power loss"));
            assert!(!load(&paths).unwrap().unwrap().complete);
            rollback(&paths).unwrap();
            for kind in [Kind::Island, Kind::Kitty, Kind::Waybar] {
                assert!(!output(&paths, kind).exists());
            }
            assert!(load(&paths).unwrap().is_none());
        }
    }

    #[test]
    fn destination_race_is_preserved_and_bad_journal_paths_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::discover(Some(root.path().join("chroma")), None).unwrap();
        Dir::open(&paths.state, true).unwrap();
        Dir::open(&paths.config.join("generated"), true).unwrap();
        let target = output(&paths, Kind::Island);
        let mut journal = Journal {
            version: 1,
            complete: false,
            restoring: false,
            changes: vec![Change {
                path: target.clone(),
                before: None,
                after: Snapshot {
                    bytes: b"generated".to_vec(),
                    mode: 0o600,
                },
            }],
        };
        crate::engine::backup::save(&journal_path(&paths), &journal, false).unwrap();
        fs_write(&target, b"user created concurrently");
        assert!(write_bundle(&paths, load(&paths).unwrap().unwrap(), |_| Ok(())).is_err());
        assert_eq!(
            safe_fs::read_optional(&target).unwrap().unwrap(),
            b"user created concurrently"
        );
        journal.changes[0].path = root.path().join("unrelated");
        crate::engine::backup::save(&journal_path(&paths), &journal, true).unwrap();
        assert!(rollback(&paths).is_err());
        assert_eq!(
            safe_fs::read_optional(&target).unwrap().unwrap(),
            b"user created concurrently"
        );
    }

    fn fs_write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
