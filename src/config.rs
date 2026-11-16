use crate::{Context, Result, bail, ensure, palette, safe_fs};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub home: PathBuf,
    pub xdg_config: PathBuf,
}

impl Paths {
    pub fn discover(config: Option<PathBuf>, state: Option<PathBuf>) -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is unset")?;
        ensure!(home.is_absolute(), "HOME must be absolute");
        let xdg = |key: &str, fallback: &str| -> PathBuf {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        let xdg_config = xdg("XDG_CONFIG_HOME", ".config");
        let overridden = config.is_some();
        let config = absolute(config.unwrap_or_else(|| xdg_config.join("chroma")))?;
        let state = absolute(state.unwrap_or_else(|| {
            if overridden {
                config.join("state")
            } else {
                xdg("XDG_STATE_HOME", ".local/state").join("chroma")
            }
        }))?;
        let cache = if overridden {
            config.join("cache")
        } else {
            xdg("XDG_CACHE_HOME", ".cache").join("chroma")
        };
        ensure!(
            config != state && !config.starts_with(&state),
            "state directory must not contain the configuration directory"
        );
        Ok(Self {
            config,
            state,
            cache,
            home,
            xdg_config,
        })
    }

    pub fn expand(&self, value: &str) -> Result<PathBuf> {
        let mut value = value.to_string();
        for (token, path) in [
            ("{config_dir}", &self.config),
            ("{state_dir}", &self.state),
            ("{home}", &self.home),
            ("{xdg_config_home}", &self.xdg_config),
        ] {
            if value.contains(token) {
                value = value.replace(token, path.to_str().context("path token requires UTF-8")?);
            }
        }
        if let Some(rest) = value.strip_prefix("~/") {
            return absolute(self.home.join(rest));
        }
        ensure!(
            !value.contains(['{', '}']) && !value.is_empty(),
            "invalid path token or empty path: {value}"
        );
        let path = PathBuf::from(value);
        absolute(if path.is_absolute() {
            path
        } else {
            self.config.join(path)
        })
    }

    pub fn check_output(&self, path: &Path) -> Result<()> {
        ensure!(
            path.is_absolute()
                && path.file_name().is_some()
                && !path
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
            "output must be an absolute file path without traversal: {}",
            path.display()
        );
        ensure!(
            !path.starts_with(&self.state),
            "output cannot overwrite Chroma state"
        );
        ensure!(
            !path.starts_with(&self.config) || path.starts_with(self.config.join("generated")),
            "outputs inside Chroma configuration must be under generated/"
        );
        Ok(())
    }
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "path must not contain '.' or '..': {}",
        path.display()
    );
    Ok(path)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub default_palette: String,
    pub auto_reload: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            default_palette: "rustwood".into(),
            auto_reload: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adapter {
    pub enabled: bool,
    /// Opt-in application-aware rendering from an existing read-only source.
    pub desktop: Option<crate::integration::colors::Kind>,
    /// An existing application's config, for diagnostics only. Never edited.
    pub config_path: Option<String>,
    pub template: Option<String>,
    pub output: Option<String>,
    /// Explicit local authorization to replace a pre-existing unmanaged file.
    pub allow_overwrite: bool,
    /// argv, never parsed by a shell.
    pub reload: Vec<String>,
    pub reload_timeout_ms: Option<u64>,
    /// Fixed application alpha bytes, owned by local config, never by palettes.
    pub alpha: BTreeMap<String, u8>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub adapters: BTreeMap<String, Adapter>,
    /// Read provenance, not part of the configuration format.
    #[serde(skip)]
    pub inputs: safe_fs::Inputs,
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.config.join("config.toml");
        let mut inputs = safe_fs::Inputs::default();
        let mut config: Self = match inputs.read(&path)? {
            Some(bytes) => toml::from_str(std::str::from_utf8(&bytes)?)
                .with_context(|| path.display().to_string())?,
            None => Self::default(),
        };
        let directory = paths.config.join("adapters");
        for filename in inputs.adapters(&directory)? {
            let path = directory.join(filename);
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("non-UTF8 adapter filename")?;
            let bytes = inputs.read(&path)?.context("adapter disappeared")?;
            let adapter = toml::from_str(std::str::from_utf8(&bytes)?)
                .with_context(|| path.display().to_string())?;
            ensure!(
                config.adapters.insert(name.to_string(), adapter).is_none(),
                "adapter {name} is defined twice"
            );
        }
        inputs.check()?;
        config.inputs = inputs;
        palette::valid_id(&config.general.default_palette)?;
        for (name, adapter) in &config.adapters {
            palette::valid_id(name)?;
            if !adapter.reload.is_empty() {
                ensure!(
                    !adapter.reload[0].is_empty()
                        && adapter.reload.iter().all(|a| !a.contains('\0')),
                    "adapter {name}: invalid reload argv"
                );
                let command = Path::new(&adapter.reload[0]);
                ensure!(
                    command.is_absolute() || command.components().count() == 1,
                    "adapter {name}: reload executable must be absolute or a PATH name"
                );
                if matches!(
                    command.file_name().and_then(|s| s.to_str()),
                    Some("sh" | "bash" | "zsh" | "fish" | "dash")
                ) {
                    bail!(
                        "adapter {name}: shell reload commands are not supported; use an executable script explicitly"
                    );
                }
            }
            ensure!(
                (1..=300_000).contains(&adapter.reload_timeout_ms.unwrap_or(3000)),
                "adapter {name}: reload_timeout_ms must be 1–300000"
            );
        }
        Ok(config)
    }
}
