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
    let mut errors = Vec::new();
    for entry in transaction.entries.iter().rev() {
        let result = (|| -> Result<()> {
            paths.check_output(&entry.path)?;
            let (dir, file) = Dir::parent(&entry.path)?;
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
                dir.stage(&before.bytes, before.mode)?.commit(&file)?;
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
    let mut errors = Vec::new();
    for entry in &transaction.entries {
        if let Some(adapter) = config.adapters.get(&entry.adapter).filter(|a| a.enabled) {
            // Never execute commands stored in backups or palette data.
            let palette = transaction
                .before
                .as_ref()
                .or(transaction.after.as_ref())
                .context("missing palette in backup")?;
            let output = crate::adapters::render(&entry.adapter, adapter, paths, &palette.palette)?;
            if output.path == entry.path {
                if let Err(error) = reload::run(&adapter.reload, output.timeout_ms) {
                    errors.push(format!("{}: {error:#}", entry.adapter));
                }
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
    restore(paths, transaction).with_context(|| {
        format!("operation failed: {error:#}; automatic recovery failed; run chroma rollback")
    })?;
    State {version: 1, current: transaction.before.clone(), head: transaction.parent.clone(), pending: None}.save(&paths.state)
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
    // Mark rollback before restoring anything; crashes can always be retried.
    state.pending = Some(Pending {
        transaction: id,
        restoring: true,
    });
    state.save(&paths.state)?;
    restore(paths, &transaction)?;
    State {
        version: 1,
        current: transaction.before.clone(),
        head: transaction.parent.clone(),
        pending: None,
    }
    .save(&paths.state)?;
    if config.general.auto_reload && !no_reload {
        reload_restored(paths, config, &transaction)?;
    }
    Ok(())
}
