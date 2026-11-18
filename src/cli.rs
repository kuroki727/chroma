use crate::{
    Context, Result, adapters,
    config::{Config, Paths},
    engine::{State, apply, rollback},
    palette::{
        self,
        generator::{self, Mode},
    },
    safe_fs::{self, Dir},
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version, about = "A local, template-driven Linux desktop color engine")]
pub struct Cli {
    /// Overrides config AND isolates state/cache underneath it unless --state-dir is set.
    #[arg(long, global = true)]
    pub config_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub state_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Opt-in wallpaper hook and synchronization controls. Never changes wallpaper.
    Wallpaper {
        #[command(subcommand)]
        command: crate::wallpaper::Command,
    },
    /// Discover adapters and explicitly configure Chroma fragment generation.
    Adapters {
        #[command(subcommand)]
        command: AdapterCommands,
    },
    /// Preview or install a disabled desktop review bundle; never edits desktop configs.
    Integrate {
        #[arg(long, group = "integration_action")]
        plan: bool,
        #[arg(long, group = "integration_action")]
        status: bool,
        #[arg(long, group = "integration_action")]
        apply: bool,
        #[arg(long, group = "integration_action")]
        rollback: bool,
        /// Explicit approval to install the reviewed bundle. Desktop activation remains manual.
        #[arg(long, requires = "apply")]
        confirm: bool,
        /// Read configs under this root; required with --config-dir for isolation.
        #[arg(long)]
        desktop_dir: Option<PathBuf>,
        #[arg(long, default_value = "rustwood")]
        palette: String,
    },
    /// Create editable presets/templates and a disabled adapter configuration. Never overwrites.
    Init,
    /// List IDs (stable, one per line) or descriptive names.
    List {
        #[arg(long)]
        names_only: bool,
    },
    /// Show the last successfully applied palette and its colors.
    Current,
    /// Generate all enabled files, back up previous contents, then optionally reload.
    Apply {
        palette: Option<String>,
        #[arg(long)]
        no_reload: bool,
        /// Render and check destinations without creating state, files or commands.
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate a palette from PNG/JPEG. Does not apply it or change wallpaper.
    Generate {
        image: PathBuf,
        #[arg(long, value_enum, default_value_t = Mode::Dark)]
        mode: Mode,
        #[arg(long, default_value = "generated")]
        id: String,
        #[arg(long, default_value = "Generated")]
        name: String,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// Restore the previous state, or recover an interrupted operation.
    Rollback {
        #[arg(long)]
        no_reload: bool,
    },
    /// Validate all available palettes and configured adapter templates; no writes.
    Validate { palette: Option<String> },
    /// Inspect installed commands and integrations without executing them or writing files.
    Doctor,
}

#[derive(Debug, Subcommand)]
pub enum AdapterCommands {
    /// List built-in fragment generators and configured custom adapters; no writes.
    List,
    /// Inspect main application configs without following includes or changing files.
    Detect {
        /// Required with --config-dir to avoid inspecting the active desktop.
        #[arg(long)]
        desktop_dir: Option<PathBuf>,
    },
    /// Enable fragment generation in Chroma's config only. Desktop wiring stays manual.
    Enable {
        name: String,
        #[arg(long)]
        desktop_dir: Option<PathBuf>,
        /// Explicitly choose opaque Fuzzel colors when existing alpha is unknown.
        #[arg(long)]
        opaque: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Disable generation in Chroma's config; keeps generated files and native imports.
    Disable {
        name: String,
        #[arg(long)]
        dry_run: bool,
    },
}

fn desktop_root(paths: &Paths, isolated: bool, desktop: Option<PathBuf>) -> Result<PathBuf> {
    crate::ensure!(
        !isolated || desktop.is_some(),
        "with --config-dir, supply --desktop-dir explicitly to avoid inspecting the active desktop"
    );
    Ok(desktop.unwrap_or_else(|| paths.xdg_config.clone()))
}

pub fn run(cli: Cli) -> Result<()> {
    let isolated = cli.config_dir.is_some();
    let paths = Paths::discover(cli.config_dir, cli.state_dir)?;
    if let Commands::Wallpaper { command } = cli.command {
        return crate::wallpaper::run(&paths, command);
    }
    if let Commands::Adapters { command } = cli.command {
        return match command {
            AdapterCommands::List => {
                let config = Config::load(&paths)?;
                for (name, file, _) in adapters::BUILTINS {
                    let enabled = config.adapters.get(*name).is_some_and(|a| a.enabled);
                    println!(
                        "{name:<12} {}  fragment: generated/{file}; native connection requires explicit setup",
                        if enabled { "enabled" } else { "disabled" }
                    );
                }
                for (name, adapter) in &config.adapters {
                    if !adapters::BUILTINS.iter().any(|(id, _, _)| *id == name) {
                        println!(
                            "{name:<12} {}  custom/review-bundle adapter; manual setup",
                            if adapter.enabled {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        );
                    }
                }
                Ok(())
            }
            AdapterCommands::Detect { desktop_dir } => {
                let desktop = desktop_root(&paths, isolated, desktop_dir)?;
                for found in adapters::discovery::detect(&paths, &desktop)? {
                    println!(
                        "{:<12} {:<15} {}\n  {}",
                        found.name,
                        found.status.label(),
                        found.path.display(),
                        found.reason
                    );
                }
                println!(
                    "Read-only detection; application versions and full config semantics are not validated. No files changed."
                );
                Ok(())
            }
            AdapterCommands::Enable {
                name,
                desktop_dir,
                opaque,
                dry_run,
            } => {
                let desktop = desktop_root(&paths, isolated, desktop_dir)?;
                adapters::setup::configure(&paths, &name, true, &desktop, opaque, dry_run)
            }
            AdapterCommands::Disable { name, dry_run } => {
                adapters::setup::configure(&paths, &name, false, &paths.config, false, dry_run)
            }
        };
    }
    if let Commands::Integrate {
        plan: _,
        status,
        apply,
        rollback,
        confirm,
        desktop_dir,
        palette,
    } = cli.command
    {
        if rollback {
            return crate::integration::rollback(&paths);
        }
        if status {
            return crate::integration::status(&paths);
        }
        crate::ensure!(
            !isolated || desktop_dir.is_some(),
            "--config-dir integration needs an explicit --desktop-dir to avoid inspecting the active desktop"
        );
        let desktop = desktop_dir.unwrap_or_else(|| paths.xdg_config.clone());
        let palette = palette::load(&paths.config, &palette)?;
        if apply {
            crate::ensure!(
                confirm,
                "review integrate --plan first, then supply --apply --confirm to install the disabled review bundle"
            );
        }
        let plan = crate::integration::plan(&paths, &desktop, &palette)?;
        if apply {
            let count = crate::integration::apply(&paths, plan, confirm)?;
            println!(
                "Installed {count} review artifact(s). Adapters remain disabled; desktop configuration was not modified. See docs/DESKTOP_INTEGRATION.md for manual activation."
            );
        } else {
            crate::integration::print(&plan);
        }
        return Ok(());
    }
    if matches!(cli.command, Commands::Init) {
        return init(&paths);
    }
    // Emergency file recovery must remain available with broken/missing local
    // adapter configuration and templates. No commands are needed in this mode.
    if matches!(cli.command, Commands::Rollback { no_reload: true }) {
        rollback::rollback(&paths, &Config::default(), true)?;
        println!("Restored previous managed files and palette state.");
        return Ok(());
    }
    let mut config = Config::load(&paths)?;
    match cli.command {
        Commands::Wallpaper { .. } | Commands::Integrate { .. } | Commands::Adapters { .. } => {
            unreachable!()
        }
        Commands::Init => unreachable!(),
        Commands::List { names_only } => {
            for (id, palette) in palette::list(&paths.config)? {
                if names_only {
                    println!("{id}");
                } else {
                    println!("{id:<20} {}", palette.name);
                }
            }
        }
        Commands::Current => {
            let state = State::load(&paths.state)?;
            crate::ensure!(
                state.pending.is_none(),
                "interrupted operation; run chroma rollback before trusting current state"
            );
            if let Some(active) = state
                .current
                .filter(|a| a.files.keys().any(|p| paths.check_output(p).is_ok()))
            {
                println!("{} ({})", active.palette.name, active.id);
                for (name, color) in active.palette.variables() {
                    println!("{name:<16} {color}");
                }
            } else {
                println!("No palette has been applied.");
            }
        }
        Commands::Apply {
            palette,
            no_reload,
            dry_run,
        } => {
            let id = palette.unwrap_or_else(|| config.general.default_palette.clone());
            config
                .inputs
                .read(&paths.config.join("palettes").join(format!("{id}.toml")))?;
            let palette = palette::load(&paths.config, &id)?;
            if dry_run {
                println!("Selected theme: {} ({id})", palette.name);
                for (name, adapter) in &config.adapters {
                    if !adapter.enabled {
                        println!("Skipped {name}: disabled");
                    }
                }
                let state = State::load(&paths.state)?;
                let plan = apply::plan(&paths, &config, &palette, &state)?;
                apply::preview_transaction(&plan, &state, &id, &palette)?;
                for (output, entry) in plan.outputs.iter().zip(&plan.entries) {
                    let action = match &entry.before {
                        None => "create",
                        Some(before) if before == &entry.after => "unchanged",
                        _ => "modify",
                    };
                    println!(
                        "Enabled {} -> {} ({action}, {} bytes)",
                        output.name,
                        output.path.display(),
                        output.bytes.len()
                    );
                }
                for report in &plan.reports {
                    println!("  {report}");
                }
                println!("Dry run passed; no files or reload commands executed.");
            } else {
                let count = apply::apply(&paths, &config, &id, palette, no_reload)?;
                println!(
                    "Applied {id} to {count} managed file(s). Imports must be configured manually."
                );
            }
        }
        Commands::Generate {
            image,
            mode,
            id,
            name,
            output,
            force,
        } => {
            palette::valid_id(&id)?;
            let palette = generator::generate(&image, mode, &name)?;
            let output = match output {
                Some(path) => {
                    if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir()?.join(path)
                    }
                }
                None => {
                    Dir::open(&paths.config.join("palettes"), true)?;
                    paths.config.join("palettes").join(format!("{id}.toml"))
                }
            };
            safe_fs::write(&output, toml::to_string_pretty(&palette)?.as_bytes(), force)?;
            println!(
                "Saved {} (foreground contrast {:.2}:1); nothing applied.",
                output.display(),
                palette
                    .colors
                    .foreground
                    .contrast(palette.colors.background)
            );
        }
        Commands::Rollback { no_reload } => {
            rollback::rollback(&paths, &config, no_reload)?;
            println!("Restored previous managed files and palette state.");
        }
        Commands::Validate { palette } => {
            let palettes = if let Some(id) = palette {
                vec![(id.clone(), palette::load(&paths.config, &id)?)]
            } else {
                palette::list(&paths.config)?.into_iter().collect()
            };
            for (id, palette) in &palettes {
                palette.validate().with_context(|| id.clone())?;
                for (name, adapter) in &config.adapters {
                    adapters::render(name, adapter, &paths, palette)?;
                }
            }
            println!(
                "Validated {} palette(s), {} configured adapter template(s).",
                palettes.len(),
                config.adapters.len()
            );
            println!(
                "Use apply --dry-run to check enabled output destinations and overwrite authorization."
            );
        }
        Commands::Doctor => doctor(&paths, &config)?,
    }
    Ok(())
}

fn init(paths: &Paths) -> Result<()> {
    for directory in ["", "palettes", "templates", "adapters", "generated"] {
        Dir::open(&paths.config.join(directory), true)?;
    }
    let mut files = vec![(
        paths.config.join("config.toml"),
        include_str!("../examples/config.toml"),
    )];
    for (id, source) in palette::presets::SOURCES {
        files.push((
            paths.config.join("palettes").join(format!("{id}.toml")),
            source,
        ));
    }
    for (_, name, source) in adapters::BUILTINS {
        files.push((paths.config.join("templates").join(name), source));
    }
    files.push((
        paths.config.join("templates/dynamic-island.txt"),
        include_str!("../examples/dynamic-island.txt"),
    ));
    for (path, source) in files {
        if safe_fs::read_optional(&path)?.is_some() {
            println!("Kept existing {}", path.display());
        } else {
            safe_fs::write(&path, source.as_bytes(), false)?;
        }
    }
    println!(
        "Initialized {}. All new adapters are disabled; run chroma adapters list, adapters detect, then adapters enable NAME --dry-run to review setup.",
        paths.config.display()
    );
    Ok(())
}

fn doctor(paths: &Paths, config: &Config) -> Result<()> {
    println!(
        "Config: {}\nState: {}\nCache: {} (unused by this MVP)",
        paths.config.display(),
        paths.state.display(),
        paths.cache.display()
    );
    for program in [
        "niri", "waybar", "fuzzel", "kitty", "foot", "island", "swaylock",
    ] {
        println!(
            "{program:<12} {}",
            if adapters::reload::available(program) {
                "found on PATH"
            } else {
                "not installed/on PATH"
            }
        );
    }
    let palette = palette::load(&paths.config, &config.general.default_palette)?;
    if !config.adapters.values().any(|adapter| adapter.enabled) {
        println!(
            "No enabled adapters: run chroma adapters list/detect and adapters enable NAME --dry-run. Palette application currently changes no files. Run chroma integrate --plan for application-aware setup; all generated adapter examples require explicit enabling."
        );
    }
    let mut issues = Vec::new();
    if safe_fs::read_optional(&paths.config.join("config.toml"))?.is_none() {
        issues.push("config.toml is missing; run init to create disabled examples".into());
    }
    for (name, adapter) in &config.adapters {
        println!(
            "{name}: {}",
            if adapter.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        match adapters::render(name, adapter, paths, &palette) {
            Ok(output) => {
                println!("  output: {}", output.path.display());
                if adapter.enabled {
                    match Dir::parent(&output.path).and_then(|(dir, name)| {
                        dir.check_writable()?;
                        dir.check_target(&name)
                    }) {
                        Ok(()) => {}
                        Err(error) => issues.push(format!("{name}: destination: {error}")),
                    }
                }
            }
            Err(error) => issues.push(format!("{name}: {error:#}")),
        }
        if let Some(path) = &adapter.config_path {
            let path = paths.expand(path)?;
            if safe_fs::read_optional(&path)?.is_none() {
                issues.push(format!(
                    "{name}: config_path is missing: {}",
                    path.display()
                ));
            }
        }
        if let Some(program) = adapter.reload.first() {
            if !adapters::reload::available(program) {
                issues.push(format!("{name}: reload executable missing: {program}"));
            }
        }
    }
    let state = State::load(&paths.state)?;
    if state.pending.is_some() {
        issues.push("interrupted transaction; run rollback".into());
    }
    println!(
        "Reload commands: {}",
        if config.general.auto_reload {
            "enabled by local config"
        } else {
            "disabled"
        }
    );
    println!(
        "Imports/application versions are not auto-detected. Niri needs >=25.11; Fuzzel includes need >=1.10."
    );
    println!(
        "Island is a standalone application; use integrate --plan for its dedicated adapter. Foot takes effect in new processes. GTK/libadwaita and Qt have no universal palette API."
    );
    for issue in &issues {
        println!("Issue: {issue}");
    }
    crate::ensure!(issues.is_empty(), "doctor found {} issue(s)", issues.len());
    Ok(())
}
