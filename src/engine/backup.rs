use crate::{Context, Result, engine::Transaction, ensure, safe_fs};
use serde::Serialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn save<T: Serialize>(path: &Path, value: &T, replace: bool) -> Result<()> {
    let bytes = toml::to_string(value)?.into_bytes();
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "state/backup exceeds 4 MiB; reduce managed file sizes or archive old state"
    );
    safe_fs::write(path, &bytes, replace).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

pub fn create(state: &Path, transaction: &Transaction) -> Result<String> {
    safe_fs::Dir::open(&state.join("transactions"), true)?;
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
    ensure!(
        id.starts_with("tx-")
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'-' || b == b't' || b == b'x'),
        "invalid transaction ID"
    );
    let path = state.join("transactions").join(format!("{id}.toml"));
    let bytes = safe_fs::read_optional(&path)?
        .with_context(|| format!("missing backup {}", path.display()))?;
    toml::from_str(std::str::from_utf8(&bytes)?).context("invalid backup")
}
