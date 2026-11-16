pub mod apply;
pub mod backup;
pub mod rollback;

use crate::{
    Result, ensure,
    palette::Palette,
    safe_fs::{self, Snapshot},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub id: String,
    pub palette: Palette,
    pub files: BTreeMap<PathBuf, Snapshot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub current: Option<Active>,
    pub head: Option<String>,
    pub pending: Option<Pending>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub transaction: String,
    pub restoring: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub parent: Option<String>,
    pub before: Option<Active>,
    pub after: Option<Active>,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub adapter: String,
    pub path: PathBuf,
    pub before: Option<Snapshot>,
    pub after: Snapshot,
}

impl State {
    pub fn load(dir: &std::path::Path) -> Result<Self> {
        match safe_fs::read_private_optional(&dir.join("state.toml"))? {
            None => Ok(Self {
                version: 1,
                ..Self::default()
            }),
            Some(bytes) => {
                let state: Self = toml::from_str(std::str::from_utf8(&bytes)?)?;
                ensure!(
                    state.version == 1,
                    "unsupported state version {}",
                    state.version
                );
                state.validate()?;
                Ok(state)
            }
        }
    }

    pub fn save(&self, dir: &std::path::Path) -> Result<()> {
        self.validate()?;
        backup::save(&dir.join("state.toml"), self, true)
    }

    /// Compare the manifest loaded under the lock before replacing it. Never
    /// turn an intervening external state edit into a silent lost update.
    pub(crate) fn save_if(&self, dir: &std::path::Path, expected: &Self) -> Result<()> {
        self.validate()?;
        let (directory, name) = safe_fs::Dir::parent(&dir.join("state.toml"))?;
        directory.check_private()?;
        directory.check_target(&name)?;
        let before = directory.read(&name)?;
        ensure!(
            before.as_ref().is_none_or(|s| s.mode & 0o077 == 0),
            "state file must be private"
        );
        let actual = match &before {
            Some(snapshot) => toml::from_str::<Self>(std::str::from_utf8(&snapshot.bytes)?)?,
            None => Self {
                version: 1,
                ..Self::default()
            },
        };
        ensure!(
            &actual == expected,
            "transaction state changed outside Chroma; preserve it and recover using retained backups"
        );
        let bytes = backup::encode(self)?;
        directory
            .stage(&bytes, before.as_ref().map_or(0o600, |s| s.mode))?
            .commit_checked(&name, before.as_ref())?;
        Ok(())
    }

    pub(crate) fn check(&self, dir: &std::path::Path) -> Result<()> {
        ensure!(
            Self::load(dir)? == *self,
            "transaction state changed outside Chroma; backups retained"
        );
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported state version");
        ensure!(
            self.head.is_some() == self.current.is_some(),
            "state head/current mismatch"
        );
        if let Some(current) = &self.current {
            current.validate()?;
        }
        if let Some(id) = &self.head {
            backup::validate_id(id)?;
        }
        if let Some(pending) = &self.pending {
            backup::validate_id(&pending.transaction)?;
        }
        Ok(())
    }
}

fn validate_path(path: &std::path::Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && path.file_name().is_some()
            && !path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "invalid managed file path: {}",
        path.display()
    );
    Ok(())
}

impl Active {
    fn validate(&self) -> Result<()> {
        crate::palette::valid_id(&self.id)?;
        self.palette.validate()?;
        for (path, snapshot) in &self.files {
            validate_path(path)?;
            snapshot.validate()?;
        }
        Ok(())
    }
}

impl Transaction {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.entries.is_empty(), "backup has no entries");
        ensure!(
            self.parent.is_some() == self.before.is_some(),
            "backup parent/before mismatch"
        );
        if let Some(id) = &self.parent {
            backup::validate_id(id)?;
        }
        if let Some(before) = &self.before {
            before.validate()?;
        }
        let after = self
            .after
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup lacks after state"))?;
        after.validate()?;
        let mut expected = self
            .before
            .as_ref()
            .map(|a| a.files.clone())
            .unwrap_or_default();
        let mut paths = BTreeSet::new();
        for entry in &self.entries {
            crate::palette::valid_id(&entry.adapter)?;
            validate_path(&entry.path)?;
            ensure!(paths.insert(&entry.path), "duplicate backup destination");
            if let Some(before) = &entry.before {
                before.validate()?;
            }
            entry.after.validate()?;
            ensure!(
                entry.after.mode == entry.before.as_ref().map_or(0o600, |s| s.mode),
                "backup permission mismatch"
            );
            expected.insert(entry.path.clone(), entry.after.clone());
        }
        ensure!(
            after.files == expected,
            "backup after-state does not match its entries"
        );
        Ok(())
    }
}

/// The setup writer may change exactly Chroma's own TOML configuration, never
/// application configs. Ordinary adapter outputs retain the v0.1 restrictions.
pub(crate) fn check_entry(paths: &crate::config::Paths, entry: &Entry) -> Result<()> {
    let standalone = entry.path.parent() == Some(paths.config.join("adapters").as_path())
        && entry.path.extension().is_some_and(|s| s == "toml");
    if entry.adapter == "chroma-setup"
        && (entry.path == paths.config.join("config.toml") || standalone)
    {
        validate_path(&entry.path)?;
        ensure!(
            !entry.path.starts_with(&paths.state),
            "setup cannot overwrite state"
        );
        Ok(())
    } else {
        // Preserve ordinary v0.1 custom adapters named chroma-setup. The name
        // alone grants no exception for protected configuration/state paths.
        paths.check_output(&entry.path)
    }
}
