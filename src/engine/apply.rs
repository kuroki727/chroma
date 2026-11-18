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
    pub reports: Vec<String>,
    pub inputs: crate::safe_fs::Inputs,
}

pub fn plan(paths: &Paths, config: &Config, palette: &Palette, state: &State) -> Result<Plan> {
    palette.validate()?;
    let mut integration_input = crate::safe_fs::Inputs::default();
    integration_input.read(&paths.state.join("integration.toml"))?;
    crate::integration::ensure_ready(paths)?;
    ensure!(
        state.pending.is_none(),
        "an interrupted operation needs recovery: run chroma rollback"
    );
    let mut sources = config.inputs.clone();
    sources.extend(&integration_input)?;
    sources.check()?;
    state.check(&paths.state)?;
    let mut outputs = Vec::new();
    let mut entries = Vec::new();
    let mut dirs = Vec::new();
    let mut reports = Vec::new();
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
        if let Some(main) = &adapter.config_path {
            sources.read(&paths.expand(main)?)?;
        }
        let output = adapters::render(name, adapter, paths, palette)?;
        sources.extend(&output.inputs)?;
        if let Some(main) = &adapter.config_path {
            if adapters::discovery::APPLICATIONS
                .iter()
                .any(|(id, _)| id == name)
            {
                let main = paths.expand(main)?;
                let (status, reason) = adapters::discovery::inspect(paths, name, &main)
                    .with_context(|| {
                        format!(
                            "adapter {name}: incompatible integration point {}",
                            main.display()
                        )
                    })?;
                reports.push(format!("{name}: integration {}: {reason}", status.label()));
            }
        } else {
            reports.push(format!("{name}: integration not inspected: no config_path; review imports and possible local color conflicts manually"));
        }

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
                output.allow_overwrite
                    || crate::integration::owns(paths, &output.path, before.as_ref())?,
                "adapter {name}: existing unmanaged or edited file {}; set allow_overwrite = true in trusted local adapter configuration to authorize replacement",
                output.path.display()
            );
        }
        let after = Snapshot {
            bytes: output.bytes.clone(),
            mode: before.as_ref().map_or(0o600, |s| s.mode),
        };
        after.validate()?;
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
        "no enabled adapters; nothing applied. Run chroma adapters list, adapters detect, and adapters enable NAME --dry-run, or preview your desktop with chroma integrate --plan. Integration bundles also start disabled; see docs/DESKTOP_INTEGRATION.md"
    );
    Ok(Plan {
        outputs,
        entries,
        dirs,
        reports,
        inputs: sources,
    })
}

fn check_committed_outputs(plan: &Plan) -> Result<()> {
    for ((dir, file), entry) in plan.dirs.iter().zip(&plan.entries) {
        dir.check_path(entry.path.parent().context("missing output parent")?)?;
        dir.check_target(file)?;
        ensure!(
            dir.read(file)?.as_ref() == Some(&entry.after),
            "managed output changed during operation: {}; external edits preserved",
            entry.path.display()
        );
    }
    Ok(())
}

/// Validate the exact backup payload without writing it. Dry-run and commit use
/// this together so serialization limits cannot turn a passing preview into an
/// application failure after staging.
pub fn preview_transaction(
    plan: &Plan,
    state: &State,
    id: &str,
    palette: &Palette,
) -> Result<Transaction> {
    let mut files = state
        .current
        .as_ref()
        .map(|a| a.files.clone())
        .unwrap_or_default();
    for entry in &plan.entries {
        files.insert(entry.path.clone(), entry.after.clone());
    }
    let transaction = Transaction {
        parent: state.head.clone(),
        before: state.current.clone(),
        after: Some(Active {
            id: id.into(),
            palette: palette.clone(),
            files,
        }),
        entries: plan.entries.clone(),
    };
    transaction.validate()?;
    backup::encode(&transaction)?;
    Ok(transaction)
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
    let state = State::load(&paths.state)?;
    let plan = plan(paths, config, &palette, &state)?;
    // Manual palette selection supersedes wallpaper hooks already waiting on
    // this writer lock. Wallpaper commits call commit directly, not apply_inner.
    _lock.advance_epoch()?;
    commit(
        paths,
        config,
        (id, palette),
        no_reload,
        &state,
        &plan,
        after_replace,
    )
}

/// Commit a fully validated plan using the v0.1 durable journal and recovery.
/// Setup uses this same writer and history, with no reload commands.
pub(crate) fn commit(
    paths: &Paths,
    config: &Config,
    selection: (&str, Palette),
    no_reload: bool,
    initial: &State,
    plan: &Plan,
    after_replace: impl Fn(usize) -> Result<()>,
) -> Result<usize> {
    ensure!(
        initial.pending.is_none(),
        "interrupted operation; run chroma rollback --no-reload"
    );
    ensure!(
        !plan.entries.is_empty() && plan.entries.len() == plan.dirs.len(),
        "invalid transaction plan"
    );
    for entry in &plan.entries {
        crate::engine::check_entry(paths, entry)?;
        entry.after.validate()?;
    }
    plan.inputs.check()?;
    initial.check(&paths.state)?;
    for ((dir, _), entry) in plan.dirs.iter().zip(&plan.entries) {
        dir.check_path(entry.path.parent().context("missing output parent")?)?;
    }
    let state_directory = Dir::open(&paths.state, false)?;
    let (id, palette) = selection;
    let mut state = initial.clone();
    let transaction = preview_transaction(plan, initial, id, &palette)?;
    // All outputs are fully written and fsynced in their destination directories
    // before a backup is committed or any target is replaced.
    let mut staged = Vec::new();
    for ((dir, _), entry) in plan.dirs.iter().zip(&plan.entries) {
        staged.push(dir.stage(&entry.after.bytes, entry.after.mode)?);
    }
    let after = transaction
        .after
        .as_ref()
        .context("missing planned after state")?
        .clone();
    let tx_id = backup::create(&paths.state, &transaction)?;
    plan.inputs.check()?;
    state_directory.check_path(&paths.state)?;
    state.pending = Some(Pending {
        transaction: tx_id.clone(),
        restoring: false,
    });
    state.save_if(&paths.state, initial)?;
    let reload_enabled = config.general.auto_reload && !no_reload;
    let result = (|| -> Result<()> {
        for (index, (((_, file), entry), staged)) in
            plan.dirs.iter().zip(&plan.entries).zip(staged).enumerate()
        {
            if !plan.outputs.is_empty() {
                plan.inputs.check()?;
            }
            state_directory.check_path(&paths.state)?;
            state.check(&paths.state)?;
            plan.dirs[index]
                .0
                .check_path(entry.path.parent().context("missing output parent")?)?;
            staged
                .commit_checked(file, entry.before.as_ref())
                .with_context(|| format!("replacing {}", entry.path.display()))?;
            after_replace(index + 1)?;
        }
        state_directory.check_path(&paths.state)?;
        state.check(&paths.state)?;
        if !plan.outputs.is_empty() {
            plan.inputs.check()?;
        }
        check_committed_outputs(plan)?;
        if reload_enabled {
            for output in &plan.outputs {
                reload::run(&output.reload, output.timeout_ms)
                    .with_context(|| format!("adapter {}", output.name))?;
            }
        }
        check_committed_outputs(plan)?;
        if !plan.outputs.is_empty() {
            plan.inputs.check()?;
        }
        let next = State {
            version: 1,
            current: Some(after),
            head: Some(tx_id),
            pending: None,
        };
        next.save_if(&paths.state, &state)?;
        Ok(())
    })();
    if let Err(error) = result {
        return rollback::recover_failed_checked(
            paths,
            config,
            &transaction,
            reload_enabled,
            &state,
            error,
        );
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

    fn race_fixture() -> (tempfile::TempDir, Paths, Config) {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::discover(Some(root.path().join("config")), None).unwrap();
        Dir::open(&paths.config.join("generated"), true).unwrap();
        Dir::open(&paths.config.join("templates"), true).unwrap();
        Dir::open(&paths.config.join("adapters"), true).unwrap();
        Dir::open(&paths.state, true).unwrap();
        std::fs::write(paths.config.join("config.toml"), "[adapters.a]\nenabled=true\ntemplate='templates/input'\noutput='generated/a'\n[adapters.b]\nenabled=true\ntemplate='templates/input'\noutput='generated/b'\n").unwrap();
        std::fs::write(paths.config.join("templates/input"), "{{ background }}\n").unwrap();
        let config = Config::load(&paths).unwrap();
        (root, paths, config)
    }

    #[test]
    fn intervening_input_and_directory_edits_fail_before_outputs_or_backups() {
        for change in [
            "config",
            "adapter-add",
            "template",
            "source",
            "directory",
            "symlink",
        ] {
            let (_root, paths, mut config) = race_fixture();
            let source = paths.config.join("source");
            std::fs::write(&source, "old source").unwrap();
            config.adapters.get_mut("a").unwrap().config_path =
                Some(source.to_str().unwrap().into());
            let palette = crate::palette::presets::get("nord").unwrap();
            let state = State::load(&paths.state).unwrap();
            let planned = plan(&paths, &config, &palette, &state).unwrap();
            match change {
                "config" => {
                    std::fs::write(paths.config.join("config.toml"), "# external edit").unwrap()
                }
                "adapter-add" => {
                    std::fs::write(paths.config.join("adapters/new.toml"), "enabled=false").unwrap()
                }
                "template" => {
                    std::fs::write(paths.config.join("templates/input"), "{{ primary }}").unwrap()
                }
                "source" => std::fs::write(&source, "external source edit").unwrap(),
                _ => {
                    std::fs::rename(paths.config.join("generated"), paths.config.join("moved"))
                        .unwrap();
                    if change == "symlink" {
                        std::os::unix::fs::symlink(
                            paths.config.join("moved"),
                            paths.config.join("generated"),
                        )
                        .unwrap();
                    } else {
                        Dir::open(&paths.config.join("generated"), true).unwrap();
                    }
                }
            }
            let error = commit(
                &paths,
                &config,
                ("nord", palette),
                true,
                &state,
                &planned,
                |_| Ok(()),
            )
            .unwrap_err();
            assert!(
                !paths.config.join("generated/a").exists(),
                "{change}: {error:#}"
            );
            assert!(!paths.config.join("moved/a").exists(), "{change}");
            assert!(!paths.state.join("transactions").exists(), "{change}");
        }
    }

    #[test]
    fn external_edits_during_replacement_are_preserved_and_recovery_is_explicit() {
        for change in [
            "config",
            "source",
            "state",
            "output",
            "symlink",
            "committed-output",
        ] {
            let (_root, paths, mut config) = race_fixture();
            let source = paths.config.join("source");
            std::fs::write(&source, "old").unwrap();
            config.adapters.get_mut("a").unwrap().config_path =
                Some(source.to_str().unwrap().into());
            let error = apply_inner(
                &paths,
                &config,
                "nord",
                crate::palette::presets::get("nord").unwrap(),
                true,
                |index| {
                    if index == 1 {
                        match change {
                            "config" => {
                                std::fs::write(paths.config.join("config.toml"), "# external edit")?
                            }
                            "source" => std::fs::write(&source, "external source")?,
                            "state" => {
                                std::fs::write(paths.state.join("state.toml"), "external state")?
                            }
                            "output" => {
                                std::fs::write(paths.config.join("generated/b"), "external output")?
                            }
                            "committed-output" => std::fs::write(
                                paths.config.join("generated/a"),
                                "external committed output",
                            )?,
                            _ => std::os::unix::fs::symlink(
                                &source,
                                paths.config.join("generated/b"),
                            )?,
                        }
                    }
                    Ok(())
                },
            )
            .unwrap_err();
            if change == "committed-output" {
                assert_eq!(
                    std::fs::read_to_string(paths.config.join("generated/a")).unwrap(),
                    "external committed output"
                );
            } else {
                assert!(
                    !paths.config.join("generated/a").exists(),
                    "{change}: {error:#}"
                );
            }
            match change {
                "state" => {
                    assert_eq!(
                        std::fs::read_to_string(paths.state.join("state.toml")).unwrap(),
                        "external state"
                    );
                    assert!(format!("{error:#}").contains("state recovery failed"));
                }
                "output" | "symlink" | "committed-output" => {
                    assert!(State::load(&paths.state).unwrap().pending.is_some());
                    assert!(format!("{error:#}").contains("incomplete rollback"));
                }
                _ => assert!(State::load(&paths.state).unwrap().pending.is_none()),
            }
        }
    }

    #[test]
    fn stale_config_and_manifest_cannot_authorize_an_apply() {
        let (_root, paths, config) = race_fixture();
        std::fs::write(
            paths.config.join("config.toml"),
            "[general]\nauto_reload=false",
        )
        .unwrap();
        assert!(
            apply(
                &paths,
                &config,
                "nord",
                crate::palette::presets::get("nord").unwrap(),
                true
            )
            .is_err()
        );
        let config = Config::load(&paths).unwrap();
        let original = State::load(&paths.state).unwrap();
        let mut changed = original.clone();
        changed.pending = Some(Pending {
            transaction: "tx-1-1".into(),
            restoring: false,
        });
        changed.save(&paths.state).unwrap();
        assert!(original.save_if(&paths.state, &original).is_err());
        assert_eq!(State::load(&paths.state).unwrap(), changed);
        assert!(
            plan(
                &paths,
                &config,
                &crate::palette::presets::get("nord").unwrap(),
                &original
            )
            .is_err()
        );
    }
}
