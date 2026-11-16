//! Explicit, previewable changes to Chroma configuration only. Setup shares the
//! palette transaction writer, lock, backups and `rollback --no-reload` recovery.
use crate::{
    Context, Result,
    adapters::{self, discovery},
    config::{Adapter, Config, Paths},
    engine::{self, Entry, State, apply},
    ensure, palette,
    safe_fs::{self, Dir, Snapshot},
};
use std::{collections::BTreeMap, path::Path};
use toml_edit::{DocumentMut, Item, value};

pub fn configure(
    paths: &Paths,
    name: &str,
    enabled: bool,
    desktop: &Path,
    opaque: bool,
    dry_run: bool,
) -> Result<()> {
    ensure!(
        adapters::BUILTINS.iter().any(|(id, _, _)| *id == name),
        "unknown built-in adapter {name}; run chroma adapters list"
    );
    discovery::check_root(desktop)?;
    engine_ready(paths)?;
    let config = Config::load(paths)?;
    let standalone = paths.config.join("adapters").join(format!("{name}.toml"));
    let path = if safe_fs::read_optional(&standalone)?.is_some() {
        standalone
    } else {
        paths.config.join("config.toml")
    };
    let (dir, file) = Dir::parent(&path).context("run chroma init before enabling adapters")?;
    dir.check_writable()?;
    dir.check_target(&file)?;
    let before = dir
        .read(&file)?
        .context("run chroma init before enabling adapters")?;
    let mut document: DocumentMut = std::str::from_utf8(&before.bytes)?.parse()?;
    let mut adapter = config.adapters.get(name).cloned().unwrap_or_default();
    adapter.enabled = enabled;
    if enabled {
        // Built-in onboarding only creates fragments in Chroma's generated/.
        // Existing custom outputs/templates require the established manual flow.
        let output = adapters::output_path(name, &adapter, paths)?;
        let filename = adapters::BUILTINS
            .iter()
            .find(|(id, _, _)| *id == name)
            .unwrap()
            .1;
        ensure!(
            output == paths.config.join("generated").join(filename),
            "guided enablement requires the default generated/{filename} output; review custom adapters manually"
        );
        ensure!(
            adapter.desktop.is_none() && adapter.template.is_none(),
            "use manual configuration for custom template/review-bundle adapters"
        );
        if let Some((_, relative)) = discovery::APPLICATIONS.iter().find(|(id, _)| *id == name) {
            let main = desktop.join(relative);
            let (status, reason) = discovery::inspect(paths, name, &main)?;
            ensure!(status != discovery::Status::Unsupported, "{name}: {reason}");
            println!("{name}: {}: {reason}", status.label());
            // config_path records the inspected integration point; it is never edited.
            if let Some(existing) = &adapter.config_path {
                ensure!(
                    paths.expand(existing)? == main,
                    "configured config_path differs from detected path; review manually"
                );
            } else {
                adapter.config_path =
                    Some(main.to_str().context("config path must be UTF-8")?.into());
            }
        }
        if name == "fuzzel" && adapter.template.is_none() {
            fill_alpha(&mut adapter, paths, opaque)?;
        }
        let selected = palette::load(&paths.config, &config.general.default_palette)?;
        let output = adapters::render(name, &adapter, paths, &selected)?;
        let (output_dir, output_name) =
            Dir::parent(&output.path).context("run chroma init to create generated/")?;
        output_dir.check_writable()?;
        output_dir.check_target(&output_name)?;
    }
    let standalone = path != paths.config.join("config.toml");
    let table = if standalone {
        document.as_item_mut()
    } else {
        &mut document["adapters"][name]
    };
    if table.is_none() {
        *table = Item::Table(toml_edit::Table::new());
    }
    assign(&mut table["enabled"], value(enabled));
    if enabled {
        if let Some(main) = &adapter.config_path {
            assign(&mut table["config_path"], value(main));
        }
        for (key, alpha) in &adapter.alpha {
            assign(&mut table["alpha"][key], value(i64::from(*alpha)));
        }
    }
    let bytes = document.to_string().into_bytes();
    // Parse the complete result before any persistent writes.
    if standalone {
        let _: Adapter = toml::from_str(std::str::from_utf8(&bytes)?)?;
    } else {
        let _: Config = toml::from_str(std::str::from_utf8(&bytes)?)?;
    }
    println!(
        "{} {} in {}\n{}",
        if enabled { "Enable" } else { "Disable" },
        name,
        path.display(),
        String::from_utf8_lossy(&bytes)
    );
    if bytes == before.bytes {
        println!("Already configured; no changes.");
        return Ok(());
    }
    let entry = Entry {
        adapter: "chroma-setup".into(),
        path,
        after: Snapshot {
            bytes,
            mode: before.mode,
        },
        before: Some(before),
    };
    engine::check_entry(paths, &entry)?;
    entry.after.validate()?;
    if dry_run {
        println!("Dry run: modify Chroma configuration only; no persistent writes or reloads.");
        return Ok(());
    }
    let plan = apply::Plan {
        outputs: Vec::new(),
        entries: vec![entry],
        dirs: vec![(dir, file)],
        reports: Vec::new(),
    };
    let state_dir = Dir::open(&paths.state, true)?;
    let _lock = state_dir.lock()?;
    let state = State::load(&paths.state)?;
    ensure!(
        state.pending.is_none(),
        "interrupted operation; run chroma rollback --no-reload"
    );
    let (id, selected) = match &state.current {
        Some(active) => (active.id.clone(), active.palette.clone()),
        None => (
            config.general.default_palette.clone(),
            palette::load(&paths.config, &config.general.default_palette)?,
        ),
    };
    apply::commit(paths, &config, (&id, selected), true, &state, &plan, |_| {
        Ok(())
    })?;
    println!(
        "Saved Chroma configuration. Undo with chroma rollback --no-reload. No theme applied or application configuration changed. Preview with chroma apply --dry-run."
    );
    Ok(())
}

fn assign(slot: &mut Item, mut replacement: Item) {
    if let (Some(before), Some(after)) = (slot.as_value(), replacement.as_value_mut()) {
        *after.decor_mut() = before.decor().clone();
    }
    *slot = replacement;
}

fn engine_ready(paths: &Paths) -> Result<()> {
    crate::integration::ensure_ready(paths)?;
    ensure!(
        State::load(&paths.state)?.pending.is_none(),
        "interrupted operation; run chroma rollback --no-reload"
    );
    Ok(())
}

fn fill_alpha(adapter: &mut Adapter, paths: &Paths, opaque: bool) -> Result<()> {
    let mut inferred = BTreeMap::new();
    if let Some(main) = &adapter.config_path {
        if let Some(bytes) = safe_fs::read_optional(&paths.expand(main)?)? {
            for ((section, key), color) in discovery::ini(std::str::from_utf8(&bytes)?)? {
                if section == "colors"
                    && adapters::FUZ_ALPHA.contains(&key.replace('-', "_").as_str())
                {
                    ensure!(
                        matches!(color.len(), 6 | 8)
                            && color.bytes().all(|b| b.is_ascii_hexdigit()),
                        "Fuzzel {key} must be literal RRGGBB or RRGGBBAA to preserve alpha"
                    );
                    inferred.insert(
                        key.replace('-', "_"),
                        if color.len() == 8 {
                            u8::from_str_radix(&color[6..], 16)?
                        } else {
                            255
                        },
                    );
                }
            }
        }
    }
    for key in adapters::FUZ_ALPHA {
        if !adapter.alpha.contains_key(*key) {
            let alpha = if opaque {
                255
            } else {
                *inferred.get(*key).with_context(|| format!("Fuzzel alpha for {key} is unknown; set it in local configuration or explicitly choose --opaque"))?
            };
            adapter.alpha.insert((*key).into(), alpha);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_failure_uses_existing_journal_and_automatic_recovery() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::discover(Some(root.path().join("config")), None).unwrap();
        Dir::open(&paths.config, true).unwrap();
        Dir::open(&paths.state, true).unwrap();
        let path = paths.config.join("config.toml");
        std::fs::write(&path, b"# original\n[adapters.waybar]\nenabled=false\n").unwrap();
        let (dir, file) = Dir::parent(&path).unwrap();
        let before = dir.read(&file).unwrap().unwrap();
        let plan = apply::Plan {
            outputs: vec![],
            reports: vec![],
            dirs: vec![(dir, file)],
            entries: vec![Entry {
                adapter: "chroma-setup".into(),
                path: path.clone(),
                before: Some(before.clone()),
                after: Snapshot {
                    bytes: b"[adapters.waybar]\nenabled=true\n".to_vec(),
                    mode: before.mode,
                },
            }],
        };
        let state_dir = Dir::open(&paths.state, false).unwrap();
        let _lock = state_dir.lock().unwrap();
        let error = apply::commit(
            &paths,
            &Config::default(),
            ("rustwood", palette::presets::get("rustwood").unwrap()),
            true,
            &State::load(&paths.state).unwrap(),
            &plan,
            |_| {
                let pending = State::load(&paths.state)?
                    .pending
                    .context("missing durable pending setup")?;
                engine::backup::load(&paths.state, &pending.transaction)?;
                crate::bail!("injected setup replacement failure")
            },
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("previous files and state restored"));
        assert_eq!(std::fs::read(path).unwrap(), before.bytes);
        let state = State::load(&paths.state).unwrap();
        assert!(state.current.is_none() && state.pending.is_none());
    }
}
