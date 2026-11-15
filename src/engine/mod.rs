pub mod apply;
pub mod backup;
pub mod rollback;

use crate::{
    Result, ensure,
    palette::Palette,
    safe_fs::{self, Snapshot},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub id: String,
    pub palette: Palette,
    pub files: BTreeMap<PathBuf, Snapshot>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub current: Option<Active>,
    pub head: Option<String>,
    pub pending: Option<Pending>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
        match safe_fs::read_optional(&dir.join("state.toml"))? {
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
                Ok(state)
            }
        }
    }

    pub fn save(&self, dir: &std::path::Path) -> Result<()> {
        backup::save(&dir.join("state.toml"), self, true)
    }
}
