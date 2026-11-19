use crate::{
    Context, Result,
    adapters::reload,
    config::{Config, Paths},
    engine::{Pending, State, Transaction, backup},
    ensure,
    safe_fs::Dir,
};

/// Recovery is retryable: entries already restored are skipped. External edits
/// are preserved and reported instead of being overwritten.
pub fn restore(paths: &Paths, transaction: &Transaction) -> Result<()> {
    // Reject a malformed later entry before touching an earlier valid one.
    transaction.validate()?;
    for entry in &transaction.entries {
        crate::engine::check_entry(paths, entry)?;
    }
    let mut errors = Vec::new();
    for entry in transaction.entries.iter().rev() {
        let result = (|| -> Result<()> {
            let (dir, file) = match Dir::parent(&entry.path) {
                Ok(pair) => pair,
                // A creation is already undone if its parent is gone too. Do
                // not recreate directories, or accept symlink/permission errors.
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && entry.before.is_none() =>
                {
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            dir.check_writable()?;
            dir.check_target(&file)?;
            let actual = dir.read(&file)?;
            if actual == entry.before {
                return Ok(());
            }
            ensure!(
                actual.as_ref() == Some(&entry.after),
                "file changed outside Chroma; preserve it or restore manually before retrying"
            );
            if let Some(before) = &entry.before {
                dir.stage(&before.bytes, before.mode)?
                    .commit_checked(&file, Some(&entry.after))?;
            } else {
                dir.remove(&file)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(format!("{}: {error:#}", entry.path.display()));
        }
    }
    ensure!(
        errors.is_empty(),
        "incomplete rollback (backup retained):\n{}",
        errors.join("\n")
    );
    Ok(())
}

fn reload_restored(paths: &Paths, config: &Config, transaction: &Transaction) -> Result<()> {
    if transaction
        .entries
        .iter()
        .all(|e| e.adapter == "chroma-setup" && paths.check_output(&e.path).is_err())
    {
        return Ok(());
    }
    config
        .inputs
        .check()
        .context("files restored, but reload configuration changed")?;
    let mut errors = Vec::new();
    for entry in &transaction.entries {
        if entry.before.as_ref() == Some(&entry.after) {
            continue;
        }
        if let Some(adapter) = config.adapters.get(&entry.adapter).filter(|a| a.enabled) {
            // Never execute commands stored in backups or palette data.
            let result = (|| -> Result<()> {
                if crate::adapters::output_path(&entry.adapter, adapter, paths)? == entry.path {
                    reload::run(&adapter.reload, adapter.reload_timeout_ms.unwrap_or(3000))?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                errors.push(format!("{}: {error:#}", entry.adapter));
            }
        }
    }
    ensure!(
        errors.is_empty(),
        "files restored, but application reload failed:\n{}",
        errors.join("\n")
    );
    Ok(())
}

pub fn recover_failed<T>(
    paths: &Paths,
    config: &Config,
    transaction: &Transaction,
    reload_enabled: bool,
    error: anyhow::Error,
) -> Result<T> {
    let expected = State::load(&paths.state)?;
    recover_failed_checked(paths, config, transaction, reload_enabled, &expected, error)
}

pub(crate) fn recover_failed_checked<T>(
    paths: &Paths,
    config: &Config,
    transaction: &Transaction,
    reload_enabled: bool,
    expected: &State,
    error: anyhow::Error,
) -> Result<T> {
    restore(paths, transaction).with_context(|| {
        format!("operation failed: {error:#}; automatic recovery failed; run chroma rollback")
    })?;
    State {version: 1, current: transaction.before.clone(), head: transaction.parent.clone(), pending: None}.save_if(&paths.state, expected)
        .with_context(|| format!("operation failed: {error:#}; files restored but state recovery failed; run chroma rollback"))?;
    let reload_result = if reload_enabled {
        reload_restored(paths, config, transaction)
    } else {
        Ok(())
    };
    match reload_result {
        Ok(()) => Err(error).context("operation failed; previous files and state restored"),
        Err(reload_error) => Err(error).context(format!(
            "previous files and state restored; {reload_error:#}"
        )),
    }
}

pub fn rollback(paths: &Paths, config: &Config, no_reload: bool) -> Result<()> {
    let dir = Dir::open(&paths.state, false).context("no state directory; nothing to roll back")?;
    let _lock = dir.lock().context("another Chroma operation is running")?;
    let mut state = State::load(&paths.state)?;
    let id = state
        .pending
        .as_ref()
        .map(|p| p.transaction.clone())
        .or_else(|| state.head.clone())
        .context("nothing to roll back")?;
    let transaction = backup::load(&paths.state, &id)?;
    // The manifest must describe one of this transaction's two endpoints.
    // A pending apply still points at `before`; a normal rollback starts at `after`.
    ensure!(
        (state.current == transaction.before
            && state.head == transaction.parent
            && state.pending.is_some())
            || (state.current == transaction.after && state.head.as_deref() == Some(&id)),
        "recovery state does not match backup; no files changed"
    );
    // An older admitted wallpaper request must not undo explicit recovery.
    // This never reads the hook queue or local adapter configuration.
    _lock.advance_epoch()?;
    // Mark rollback before restoring anything; crashes can always be retried.
    let previous = state.clone();
    state.pending = Some(Pending {
        transaction: id,
        restoring: true,
    });
    dir.check_path(&paths.state)?;
    state.save_if(&paths.state, &previous)?;
    restore(paths, &transaction)?;
    dir.check_path(&paths.state)?;
    State {
        version: 1,
        current: transaction.before.clone(),
        head: transaction.parent.clone(),
        pending: None,
    }
    .save_if(&paths.state, &state)?;
    if config.general.auto_reload && !no_reload {
        reload_restored(paths, config, &transaction)?;
    }
    Ok(())
}
