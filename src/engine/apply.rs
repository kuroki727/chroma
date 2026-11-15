use crate::{
    Context, Result,
    adapters::{self, Output, reload},
    config::{Config, Paths},
    engine::{Active, Entry, Pending, State, Transaction, backup, rollback},
    ensure,
    palette::Palette,
    safe_fs::{Dir, Snapshot},
};
use std::{collections::BTreeSet, ffi::OsString};

pub struct Plan {
    pub outputs: Vec<Output>,
    pub entries: Vec<Entry>,
    pub dirs: Vec<(Dir, OsString)>,
}

pub fn plan(paths: &Paths, config: &Config, palette: &Palette, state: &State) -> Result<Plan> {
    palette.validate()?;
    ensure!(
        state.pending.is_none(),
        "an interrupted operation needs recovery: run chroma rollback"
    );
    let mut outputs = Vec::new();
    let mut entries = Vec::new();
    let mut dirs = Vec::new();
    let mut destinations = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    for adapter in config.adapters.values() {
        for input in [&adapter.template, &adapter.config_path]
            .into_iter()
            .flatten()
        {
            inputs.insert(paths.expand(input)?);
        }
    }
    for (name, adapter) in config.adapters.iter().filter(|(_, a)| a.enabled) {
        let output = adapters::render(name, adapter, paths, palette)?;
        ensure!(
            !inputs.contains(&output.path),
            "adapter {name}: output cannot replace any adapter's template or main config: {}",
            output.path.display()
        );
        ensure!(
            destinations.insert(output.path.clone()),
            "duplicate output destination {}",
            output.path.display()
        );
        let (dir, file) = Dir::parent(&output.path).with_context(|| {
            format!(
                "adapter {name}: destination parent must exist: {}",
                output.path.display()
            )
        })?;
        dir.check_writable()?;
        dir.check_target(&file).with_context(|| {
            format!(
                "adapter {name}: unsafe destination {}",
                output.path.display()
            )
        })?;
        let before = dir.read(&file)?;
        let managed = state
            .current
            .as_ref()
            .and_then(|a| a.files.get(&output.path));
        if before.is_some() && before.as_ref() != managed {
            ensure!(
                output.allow_overwrite,
                "adapter {name}: existing unmanaged or edited file {}; set allow_overwrite = true in trusted local adapter configuration to authorize replacement",
                output.path.display()
            );
        }
        let after = Snapshot {
            bytes: output.bytes.clone(),
            mode: before.as_ref().map_or(0o600, |s| s.mode),
        };
        entries.push(Entry {
            adapter: name.clone(),
            path: output.path.clone(),
            before,
            after,
        });
        outputs.push(output);
        dirs.push((dir, file));
    }
    ensure!(
        !outputs.is_empty(),
        "no enabled adapters; run chroma init, then enable an adapter in config.toml"
    );
    Ok(Plan {
        outputs,
        entries,
        dirs,
    })
}

pub fn apply(
    paths: &Paths,
    config: &Config,
    id: &str,
    palette: Palette,
    no_reload: bool,
) -> Result<usize> {
    apply_inner(paths, config, id, palette, no_reload, |_| Ok(()))
}

fn apply_inner(
    paths: &Paths,
    config: &Config,
    id: &str,
    palette: Palette,
    no_reload: bool,
    after_replace: impl Fn(usize) -> Result<()>,
) -> Result<usize> {
    crate::palette::valid_id(id)?;
    let state_dir = Dir::open(&paths.state, true)?;
    let _lock = state_dir
        .lock()
        .context("another Chroma operation is running, or state is unsafe")?;
    let mut state = State::load(&paths.state)?;
    let plan = plan(paths, config, &palette, &state)?;
    // All outputs are fully written and fsynced in their destination directories
    // before a backup is committed or any target is replaced.
    let mut staged = Vec::new();
    for ((dir, _), entry) in plan.dirs.iter().zip(&plan.entries) {
        staged.push(dir.stage(&entry.after.bytes, entry.after.mode)?);
    }
    let mut files = state
        .current
        .as_ref()
        .map(|a| a.files.clone())
        .unwrap_or_default();
    for entry in &plan.entries {
        files.insert(entry.path.clone(), entry.after.clone());
    }
    let after = Active {
        id: id.to_string(),
        palette,
        files,
    };
    let transaction = Transaction {
        parent: state.head.clone(),
        before: state.current.clone(),
        after: Some(after.clone()),
        entries: plan.entries.clone(),
    };
    let tx_id = backup::create(&paths.state, &transaction)?;
    state.pending = Some(Pending {
        transaction: tx_id.clone(),
        restoring: false,
    });
    state.save(&paths.state)?;
    let reload_enabled = config.general.auto_reload && !no_reload;
    let result = (|| -> Result<()> {
        for (index, (((_, file), entry), staged)) in
            plan.dirs.iter().zip(&plan.entries).zip(staged).enumerate()
        {
            staged
                .commit_checked(file, entry.before.as_ref())
                .with_context(|| format!("replacing {}", entry.path.display()))?;
            after_replace(index + 1)?;
        }
        if reload_enabled {
            for output in &plan.outputs {
                reload::run(&output.reload, output.timeout_ms)
                    .with_context(|| format!("adapter {}", output.name))?;
            }
        }
        let next = State {
            version: 1,
            current: Some(after),
            head: Some(tx_id),
            pending: None,
        };
        next.save(&paths.state)?;
        Ok(())
    })();
    if let Err(error) = result {
        return rollback::recover_failed(paths, config, &transaction, reload_enabled, error);
    }
    Ok(plan.entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Adapter;

    #[test]
    fn injected_failures_after_first_and_multiple_replacements_restore_real_transaction() {
        for fail_at in [1, 2, 3] {
            let root = tempfile::tempdir().unwrap();
            let paths = Paths::discover(Some(root.path().join("config")), None).unwrap();
            Dir::open(&paths.config.join("generated"), true).unwrap();
            let mut config = Config::default();
            for name in ["niri", "kitty", "foot"] {
                config.adapters.insert(
                    name.into(),
                    Adapter {
                        enabled: true,
                        ..Default::default()
                    },
                );
            }
            apply(
                &paths,
                &config,
                "rustwood",
                crate::palette::presets::get("rustwood").unwrap(),
                true,
            )
            .unwrap();
            let before = State::load(&paths.state).unwrap();
            let original_manifest = std::fs::read(paths.state.join("state.toml")).unwrap();
            let error = apply_inner(
                &paths,
                &config,
                "nord",
                crate::palette::presets::get("nord").unwrap(),
                true,
                |count| {
                    let pending = State::load(&paths.state)?
                        .pending
                        .expect("journal precedes replacement");
                    backup::load(&paths.state, &pending.transaction)?;
                    crate::ensure!(
                        count != fail_at,
                        "injected failure after {count} replacements"
                    );
                    Ok(())
                },
            )
            .unwrap_err();
            assert!(format!("{error:#}").contains("injected failure"));
            assert_eq!(
                std::fs::read(paths.state.join("state.toml")).unwrap(),
                original_manifest
            );
            for (path, snapshot) in &before.current.unwrap().files {
                let (dir, name) = Dir::parent(path).unwrap();
                assert_eq!(dir.read(&name).unwrap().as_ref(), Some(snapshot));
            }
            assert!(State::load(&paths.state).unwrap().pending.is_none());
        }
    }
}
