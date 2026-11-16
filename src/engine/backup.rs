use crate::{Context, Result, engine::Transaction, ensure, safe_fs};
use serde::Serialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn save<T: Serialize>(path: &Path, value: &T, replace: bool) -> Result<()> {
    let bytes = encode(value)?;
    safe_fs::write(path, &bytes, replace).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = toml::to_string(value)?.into_bytes();
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "state/backup exceeds 4 MiB; reduce managed file sizes or archive old state"
    );
    Ok(bytes)
}

pub fn create(state: &Path, transaction: &Transaction) -> Result<String> {
    transaction.validate()?;
    safe_fs::Dir::open(&state.join("transactions"), true)?.check_private()?;
    let id = format!(
        "tx-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    save(
        &state.join("transactions").join(format!("{id}.toml")),
        transaction,
        false,
    )?;
    Ok(id)
}

pub fn load(state: &Path, id: &str) -> Result<Transaction> {
    validate_id(id)?;
    let path = state.join("transactions").join(format!("{id}.toml"));
    let bytes = safe_fs::read_private_optional(&path)?
        .with_context(|| format!("missing backup {}", path.display()))?;
    let tx: Transaction = toml::from_str(std::str::from_utf8(&bytes)?).context("invalid backup")?;
    tx.validate()
        .context("invalid backup structure; no files changed")?;
    ensure!(
        tx.parent.as_deref() != Some(id),
        "backup cannot be its own parent"
    );
    Ok(tx)
}

pub fn validate_id(id: &str) -> Result<()> {
    let parts: Vec<_> = id.split('-').collect();
    ensure!(
        parts.len() == 3
            && parts[0] == "tx"
            && parts[1..]
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "invalid transaction ID"
    );
    Ok(())
}
