//! Explicit post-wallpaper hook. No daemon, palette cache, or second theme writer.
use crate::{
    Context, Result,
    config::{Config, Paths},
    engine::{self, State, apply},
    ensure,
    palette::{
        self,
        generator::{self, Mode},
    },
    safe_fs::{self, Dir, Lock, Snapshot},
};
use clap::Subcommand;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ID: &str = "wallpaper-auto";

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Enable only Chroma's hook; wallpaper manager wiring stays manual.
    Enable {
        #[arg(long)]
        dry_run: bool,
    },
    /// Disable the hook, retaining outputs and the current theme.
    Disable {
        #[arg(long)]
        dry_run: bool,
    },
    Status,
    /// Run AFTER the wallpaper manager succeeds. Dark PNG/JPEG themes only.
    Changed {
        image: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        no_reload: bool,
    },
}

pub fn run(paths: &Paths, command: Command) -> Result<()> {
    match command {
        Command::Enable { dry_run } => configure(paths, true, dry_run),
        Command::Disable { dry_run } => configure(paths, false, dry_run),
        Command::Status => {
            println!(
                "Wallpaper synchronization: {}",
                if Config::load(paths)?.wallpaper.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            Ok(())
        }
        Command::Changed {
            image,
            dry_run,
            no_reload,
        } => changed(paths, &image, dry_run, no_reload),
    }
}

fn wait_lock(dir: &Dir, deadline: Instant) -> Result<Lock> {
    loop {
        match dir.lock() {
            Ok(lock) => return Ok(lock),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    Instant::now() < deadline,
                    "timed out waiting for Chroma; rerun the hook for the current wallpaper"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn latest(queue: &Dir, token: &[u8]) -> Result<bool> {
    queue.check_target(OsStr::new("latest"))?;
    Ok(queue
        .read(OsStr::new("latest"))?
        .is_some_and(|s| s.bytes == token))
}

fn changed(paths: &Paths, image: &Path, dry_run: bool, no_reload: bool) -> Result<()> {
    let config = Config::load(paths)?;
    if !config.wallpaper.enabled {
        println!("Wallpaper synchronization disabled; nothing changed.");
        return Ok(());
    }
    if dry_run {
        let selected = generator::generate(image, Mode::Dark, "Wallpaper")?;
        let state = State::load(&paths.state)?;
        let config = automatic_config(config);
        let plan = apply::plan(paths, &config, &selected, &state)?;
        check_managed_outputs(&state, &plan)?;
        validate(&config, &plan)?;
        apply::preview_transaction(&plan, &state, ID, &selected)?;
        for output in plan.outputs {
            println!("{} -> {}", output.name, output.path.display());
        }
        println!(
            "Dry run passed; no persistent files, requests or reload commands. Validators used temporary files only."
        );
        return Ok(());
    }

    // Registration is independent of the theme lock: a new event can supersede
    // workers waiting for extraction/application. Tokens aren't a theme cache.
    let state_dir = Dir::open(&paths.state, true)?;
    state_dir.check_private()?;
    let queue_path = paths.state.join("wallpaper-sync");
    let queue = Dir::open(&queue_path, true)?;
    let deadline = Instant::now() + Duration::from_millis(config.wallpaper.wait_timeout_ms);
    let token = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    )
    .into_bytes();
    let epoch = {
        let _registration = wait_lock(&queue, deadline)?;
        let epoch = state_dir.operation_epoch()?;
        safe_fs::write(&queue_path.join("latest"), &token, true)?;
        epoch
    };
    thread::sleep(Duration::from_millis(config.wallpaper.debounce_ms));
    let _lock = wait_lock(&state_dir, deadline)?;
    state_dir.check_path(&paths.state)?;
    queue.check_path(&queue_path)?;
    if !latest(&queue, &token)? || state_dir.operation_epoch()? != epoch {
        println!("Wallpaper request superseded; nothing applied.");
        return Ok(());
    }
    // Reload configuration under the writer lock, including disablement that
    // happened while waiting. commit rechecks all captured config/template inputs.
    let config = Config::load(paths)?;
    if !config.wallpaper.enabled {
        return Ok(());
    }
    let config = automatic_config(config);
    let state = State::load(&paths.state)?;
    ensure!(
        state.pending.is_none(),
        "interrupted operation; run chroma rollback --no-reload"
    );
    let selected = generator::generate(image, Mode::Dark, "Wallpaper")?;
    let plan = apply::plan(paths, &config, &selected, &state)?;
    check_managed_outputs(&state, &plan)?;
    validate(&config, &plan)?;
    apply::preview_transaction(&plan, &state, ID, &selected)?;
    if !latest(&queue, &token)? || state_dir.operation_epoch()? != epoch {
        println!("Wallpaper request superseded; nothing applied.");
        return Ok(());
    }
    // Compare real outputs as well as the palette. A rollback, new adapter or
    // external edit must never be hidden by an image-path/hash-only cache.
    plan.inputs.check()?;
    state.check(&paths.state)?;
    for ((dir, name), entry) in plan.dirs.iter().zip(&plan.entries) {
        dir.check_path(entry.path.parent().context("missing output parent")?)?;
        dir.check_target(name)?;
        ensure!(
            dir.read(name)? == entry.before,
            "output changed during wallpaper validation: {}",
            entry.path.display()
        );
    }
    if state
        .current
        .as_ref()
        .is_some_and(|a| a.palette == selected)
        && plan
            .entries
            .iter()
            .all(|e| e.before.as_ref() == Some(&e.after))
    {
        println!("Wallpaper colors already applied; no transaction or reload.");
        return Ok(());
    }
    let count = apply::commit(
        paths,
        &config,
        (ID, selected),
        no_reload,
        &state,
        &plan,
        |_| Ok(()),
    )?;
    println!("Applied wallpaper colors to {count} managed file(s).");
    Ok(())
}

fn automatic_config(mut config: Config) -> Config {
    // Manual overwrite authorization does not authorize unattended replacement
    // of user customizations. Use the existing managed-file ownership checks.
    for adapter in config.adapters.values_mut() {
        adapter.allow_overwrite = false;
    }
    config
}

fn check_managed_outputs(state: &State, plan: &apply::Plan) -> Result<()> {
    for entry in &plan.entries {
        if let Some(expected) = state
            .current
            .as_ref()
            .and_then(|a| a.files.get(&entry.path))
        {
            ensure!(
                entry.before.as_ref() == Some(expected),
                "managed output changed or was deleted; automatic synchronization preserves user changes: {}",
                entry.path.display()
            );
        }
    }
    Ok(())
}

fn validate(config: &Config, plan: &apply::Plan) -> Result<()> {
    use crate::integration::{colors::Kind, validate};
    for output in &plan.outputs {
        let adapter = &config.adapters[&output.name];
        let kind = adapter.desktop.or(match output.name.as_str() {
            "niri" => Some(Kind::Niri),
            "fuzzel" => Some(Kind::Fuzzel),
            _ => None,
        });
        if let Some(kind) = kind {
            // Validate generated Niri as a standalone fragment, never follow
            // live imports or contact compositor IPC. Native wiring is separate.
            validate::generated(kind, b"", &output.bytes)?;
        }
        if !adapter.validate.is_empty() {
            let temp = tempfile::tempdir()?;
            let file = temp
                .path()
                .join(output.path.file_name().context("missing output filename")?);
            std::fs::write(&file, &output.bytes)?;
            let file_arg = file
                .to_str()
                .context("validator temporary path must be UTF-8")?;
            let argv: Vec<_> = adapter
                .validate
                .iter()
                .map(|s| s.replace("{file}", file_arg))
                .collect();
            crate::adapters::reload::run(&argv, adapter.validation_timeout_ms.unwrap_or(3000))
                .with_context(|| {
                    format!("adapter {}: generated file validation failed", output.name)
                })?;
            ensure!(
                safe_fs::read_optional(&file)?.as_deref() == Some(output.bytes.as_slice()),
                "adapter {}: validator modified its input",
                output.name
            );
        }
    }
    config.inputs.check()?;
    Ok(())
}

fn configure(paths: &Paths, enabled: bool, dry_run: bool) -> Result<()> {
    crate::integration::ensure_ready(paths)?;
    let config = Config::load(paths)?;
    let path = paths.config.join("config.toml");
    let (dir, file) = Dir::parent(&path).context("run chroma init first")?;
    dir.check_writable()?;
    dir.check_target(&file)?;
    let before = dir.read(&file)?.context("run chroma init first")?;
    let mut document: toml_edit::DocumentMut = std::str::from_utf8(&before.bytes)?.parse()?;
    if config.wallpaper.enabled == enabled {
        println!(
            "Wallpaper synchronization already {}.",
            if enabled { "enabled" } else { "disabled" }
        );
        return Ok(());
    }
    let table = &mut document["wallpaper"];
    if table.is_none() {
        *table = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let slot = &mut table["enabled"];
    let mut replacement = toml_edit::value(enabled);
    if let (Some(old), Some(new)) = (slot.as_value(), replacement.as_value_mut()) {
        *new.decor_mut() = old.decor().clone();
    }
    *slot = replacement;
    let bytes = document.to_string().into_bytes();
    let _: Config = toml::from_str(std::str::from_utf8(&bytes)?)?;
    let plan = apply::Plan {
        outputs: vec![],
        reports: vec![],
        inputs: config.inputs.clone(),
        dirs: vec![(dir, file)],
        entries: vec![engine::Entry {
            adapter: "chroma-setup".into(),
            path,
            after: Snapshot {
                bytes,
                mode: before.mode,
            },
            before: Some(before),
        }],
    };
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
    apply::preview_transaction(&plan, &state, &id, &selected)?;
    plan.inputs.check()?;
    if dry_run {
        println!("{}", String::from_utf8_lossy(&plan.entries[0].after.bytes));
        println!("Dry run; no changes.");
        return Ok(());
    }
    let state_dir = Dir::open(&paths.state, true)?;
    let _lock = state_dir.lock()?;
    _lock.advance_epoch()?;
    apply::commit(paths, &config, (&id, selected), true, &state, &plan, |_| {
        Ok(())
    })?;
    println!(
        "Wallpaper synchronization {}. Outputs retained; hook wiring stays manual.",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}
