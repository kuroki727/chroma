use chroma::{
    adapters::discovery::{self, Status},
    config::Paths,
    engine::{State, backup},
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct Desktop {
    root: TempDir,
}
impl Desktop {
    fn new() -> Self {
        Self {
            root: tempdir().unwrap(),
        }
    }
    fn config(&self) -> std::path::PathBuf {
        self.root.path().join("xdg/chroma")
    }
    fn desktop(&self) -> std::path::PathBuf {
        self.root.path().join("xdg")
    }
    fn paths(&self) -> Paths {
        Paths::discover(Some(self.config()), None).unwrap()
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_chroma"))
            .args(["--config-dir", self.config().to_str().unwrap()])
            .args(args)
            .env("HOME", self.root.path())
            .env("XDG_CONFIG_HOME", self.desktop())
            .env("XDG_STATE_HOME", self.root.path().join("state"))
            .env("XDG_CACHE_HOME", self.root.path().join("cache"))
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn write(&self, relative: &str, text: &str) {
        let path = self.desktop().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn enable(&self, name: &str, extra: &[&str]) -> String {
        let desktop = self.desktop();
        let mut args = vec![
            "adapters",
            "enable",
            name,
            "--desktop-dir",
            desktop.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        self.ok(&args)
    }
    fn ready(&self) {
        self.ok(&["init"]);
        self.write("waybar/style.css", &format!("@import \"{}/generated/waybar.css\";\nwindow#waybar {{ background: @chroma_background; color: @chroma_foreground; font-size: 17px; }}\n", self.config().display()));
        self.write(
            "fuzzel/fuzzel.ini",
            &format!(
                "[main]\ninclude={}/generated/fuzzel.ini\nfont=My Font:size=17\nwidth=63\n",
                self.config().display()
            ),
        );
    }
}

fn snapshot(root: &Path) -> BTreeMap<std::path::PathBuf, (Vec<u8>, u32, std::time::SystemTime)> {
    fn visit(
        root: &Path,
        out: &mut BTreeMap<std::path::PathBuf, (Vec<u8>, u32, std::time::SystemTime)>,
    ) {
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let bytes = if meta.is_dir() {
                vec![]
            } else {
                fs::read(&path).unwrap()
            };
            out.insert(
                path.clone(),
                (bytes, meta.permissions().mode(), meta.modified().unwrap()),
            );
            if meta.is_dir() {
                visit(&path, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, &mut out);
    out
}

#[test]
fn fresh_list_and_missing_discovery_do_not_initialize_anything() {
    let d = Desktop::new();
    let before = snapshot(d.root.path());
    assert!(d.ok(&["adapters", "list"]).contains("fuzzel"));
    let out = d.ok(&[
        "adapters",
        "detect",
        "--desktop-dir",
        d.desktop().to_str().unwrap(),
    ]);
    assert_eq!(out.matches("missing ").count(), 6);
    assert_eq!(snapshot(d.root.path()), before);
}

#[test]
fn isolation_requires_explicit_desktop_root() {
    let d = Desktop::new();
    for args in [
        vec!["adapters", "detect"],
        vec!["adapters", "enable", "waybar"],
    ] {
        assert!(!d.run(&args).status.success());
    }
    assert!(!d.config().exists());
}

#[test]
fn detection_classifies_ready_setup_missing_and_unsupported_independently() {
    let d = Desktop::new();
    d.ready();
    d.write(
        "niri/config.kdl",
        "// Manual KDL\nbinds { Mod+Return { spawn \"foot\"; } }\n",
    );
    d.write("foot/foot.ini", "font=Unchanged:size=12\n");
    let report = discovery::detect(&d.paths(), &d.desktop()).unwrap();
    assert_eq!(report[0].status, Status::Ready);
    assert_eq!(report[1].status, Status::Ready);
    assert_eq!(report[2].status, Status::SetupRequired);
    assert_eq!(report[4].status, Status::Missing);
    d.write("fuzzel/fuzzel.ini", "[main\nfont=x");
    assert_eq!(
        discovery::detect(&d.paths(), &d.desktop()).unwrap()[1].status,
        Status::Unsupported
    );
}

#[test]
fn existing_literal_colors_are_setup_required_and_remain_untouched() {
    let d = Desktop::new();
    d.ok(&["init"]);
    let css = "/* user */\nwindow#waybar { background: #123456; color: #fefefe; font-family: Custom; padding: 8px; }\n";
    d.write("waybar/style.css", css);
    assert!(d.enable("waybar", &[]).contains("setup-required"));
    let out = d.ok(&["apply", "nord", "--dry-run"]);
    assert!(out.contains("literal colors are preserved"));
    d.ok(&["apply", "nord", "--no-reload"]);
    assert_eq!(
        fs::read_to_string(d.desktop().join("waybar/style.css")).unwrap(),
        css
    );
}

#[test]
fn setup_preview_and_apply_preview_preserve_complete_filesystem() {
    let d = Desktop::new();
    d.ready();
    let before = snapshot(d.root.path());
    d.enable("waybar", &["--dry-run"]);
    d.enable("fuzzel", &["--opaque", "--dry-run"]);
    assert_eq!(snapshot(d.root.path()), before);
    d.enable("waybar", &[]);
    d.enable("fuzzel", &["--opaque"]);
    let before = snapshot(d.root.path());
    let out = d.ok(&["apply", "nord", "--dry-run"]);
    assert!(out.contains("Selected theme: Nord (nord)"));
    assert!(out.contains("Enabled waybar"));
    assert!(out.contains("create,"));
    assert!(out.contains("Skipped kitty: disabled"));
    assert_eq!(snapshot(d.root.path()), before);
    d.ok(&["apply", "nord", "--no-reload"]);
    let before = snapshot(d.root.path());
    assert!(d.ok(&["apply", "nord", "--dry-run"]).contains("unchanged,"));
    assert_eq!(snapshot(d.root.path()), before);
}

#[test]
fn multiple_adapters_propagate_palette_and_reapply_identically() {
    let d = Desktop::new();
    d.ready();
    d.enable("waybar", &[]);
    d.enable("fuzzel", &["--opaque"]);
    let original_css = fs::read(d.desktop().join("waybar/style.css")).unwrap();
    let original_ini = fs::read(d.desktop().join("fuzzel/fuzzel.ini")).unwrap();
    for id in ["nord", "paper", "rustwood"] {
        d.ok(&["apply", id, "--no-reload"]);
        let palette = chroma::palette::presets::get(id).unwrap();
        let css = fs::read_to_string(d.config().join("generated/waybar.css")).unwrap();
        let ini = fs::read_to_string(d.config().join("generated/fuzzel.ini")).unwrap();
        for (name, color) in palette.colors.named() {
            assert!(css.contains(&format!("@define-color chroma_{name} {color};")));
        }
        assert!(ini.contains(&format!(
            "background={}FF",
            &palette.colors.background.to_string()[1..]
        )));
        assert!(ini.contains(&format!(
            "border={}FF",
            &palette.colors.border.to_string()[1..]
        )));
        d.ok(&["apply", id, "--no-reload"]);
        assert_eq!(
            fs::read_to_string(d.config().join("generated/waybar.css")).unwrap(),
            css
        );
        assert_eq!(
            fs::read_to_string(d.config().join("generated/fuzzel.ini")).unwrap(),
            ini
        );
    }
    assert_eq!(
        fs::read(d.desktop().join("waybar/style.css")).unwrap(),
        original_css
    );
    assert_eq!(
        fs::read(d.desktop().join("fuzzel/fuzzel.ini")).unwrap(),
        original_ini
    );
}

#[test]
fn enable_is_idempotent_preserves_comments_permissions_and_undoes_exactly() {
    let d = Desktop::new();
    d.ready();
    let main = d.config().join("config.toml");
    fs::set_permissions(&main, fs::Permissions::from_mode(0o640)).unwrap();
    let before = fs::read(&main).unwrap();
    d.enable("waybar", &[]);
    assert!(d.ok(&["current"]).contains("No palette has been applied"));
    let after = snapshot(d.root.path());
    assert!(d.enable("waybar", &[]).contains("Already configured"));
    assert_eq!(snapshot(d.root.path()), after);
    assert_eq!(
        fs::metadata(&main).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(
        fs::read_to_string(&main)
            .unwrap()
            .contains("# Commands only run")
    );
    d.ok(&["rollback", "--no-reload"]);
    assert_eq!(fs::read(main).unwrap(), before);
}

#[test]
fn standalone_adapters_and_disable_share_rollback_history() {
    let d = Desktop::new();
    d.ready();
    let path = d.config().join("adapters/waybar.toml");
    fs::write(
        d.config().join("config.toml"),
        "[general]\nauto_reload=false\n",
    )
    .unwrap();
    fs::write(
        &path,
        "# Keep this comment\nenabled=false # retain explanation\n",
    )
    .unwrap();
    let before = fs::read(&path).unwrap();
    d.enable("waybar", &[]);
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("# retain explanation")
    );
    d.ok(&["adapters", "disable", "waybar"]);
    assert!(!chroma::config::Config::load(&d.paths()).unwrap().adapters["waybar"].enabled);
    d.ok(&["rollback", "--no-reload"]);
    assert!(chroma::config::Config::load(&d.paths()).unwrap().adapters["waybar"].enabled);
    d.ok(&["rollback", "--no-reload"]);
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn duplicate_or_conflicting_includes_and_invalid_ini_are_refused() {
    let d = Desktop::new();
    d.ready();
    let import = format!(
        "@import \"{}/generated/waybar.css\";\n",
        d.config().display()
    );
    for source in [
        format!("{import}{import}"),
        "window { color: #fff;".into(),
        "/* broken".into(),
        "@import \"/untrusted/file\";\n".into(),
    ] {
        d.write("waybar/style.css", &source);
        assert_eq!(
            discovery::detect(&d.paths(), &d.desktop()).unwrap()[0].status,
            Status::Unsupported
        );
        assert!(
            !d.run(&[
                "adapters",
                "enable",
                "waybar",
                "--desktop-dir",
                d.desktop().to_str().unwrap()
            ])
            .status
            .success()
        );
    }
    for source in [
        "[main]\ninclude=/outside",
        "[main]\ninclude=x\ninclude=x",
        "[colors]\ntext=ffffff\ntext=000000",
        "[colors]\ninclude=foo",
        "[main]\ninvalid",
    ] {
        d.write("fuzzel/fuzzel.ini", source);
        assert_eq!(
            discovery::detect(&d.paths(), &d.desktop()).unwrap()[1].status,
            Status::Unsupported
        );
    }
}

#[test]
fn alpha_is_inferred_without_changing_existing_fuzzel_colors_or_settings() {
    let d = Desktop::new();
    d.ok(&["init"]);
    let mut source = String::from("[main]\nfont=My Font:size=18\nterminal=foot\n[colors]\n");
    for (i, key) in chroma::adapters::FUZ_ALPHA.iter().enumerate() {
        source.push_str(&format!(
            "{}=123456{:02x}\n",
            key.replace('_', "-"),
            i + 100
        ));
    }
    d.write("fuzzel/fuzzel.ini", &source);
    d.enable("fuzzel", &[]);
    d.ok(&["apply", "nord", "--no-reload"]);
    let ini = fs::read_to_string(d.config().join("generated/fuzzel.ini")).unwrap();
    for (i, key) in chroma::adapters::FUZ_ALPHA.iter().enumerate() {
        let line = ini
            .lines()
            .find(|l| l.starts_with(&format!("{}=", key.replace('_', "-"))))
            .unwrap();
        assert!(line.ends_with(&format!("{:02X}", i + 100)), "{line}");
    }
    assert_eq!(
        fs::read_to_string(d.desktop().join("fuzzel/fuzzel.ini")).unwrap(),
        source
    );
}

#[test]
fn unknown_alpha_requires_explicit_choice_and_never_writes_on_error() {
    let d = Desktop::new();
    d.ready();
    let before = snapshot(d.root.path());
    let out = d.run(&[
        "adapters",
        "enable",
        "fuzzel",
        "--desktop-dir",
        d.desktop().to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--opaque"));
    assert_eq!(snapshot(d.root.path()), before);
}

#[test]
fn unsafe_roots_sources_targets_and_setup_symlinks_are_rejected() {
    let d = Desktop::new();
    d.ready();
    let outside = d.root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, d.root.path().join("linked-root")).unwrap();
    assert!(discovery::detect(&d.paths(), &d.root.path().join("linked-root")).is_err());
    assert!(discovery::detect(&d.paths(), &d.desktop().join("../outside")).is_err());
    fs::remove_file(d.desktop().join("waybar/style.css")).unwrap();
    fs::write(outside.join("style.css"), "external").unwrap();
    symlink(
        outside.join("style.css"),
        d.desktop().join("waybar/style.css"),
    )
    .unwrap();
    assert_eq!(
        discovery::detect(&d.paths(), &d.desktop()).unwrap()[0].status,
        Status::Unsupported
    );
    let main = d.config().join("config.toml");
    let original = fs::read(&main).unwrap();
    fs::remove_file(&main).unwrap();
    fs::write(outside.join("config.toml"), &original).unwrap();
    symlink(outside.join("config.toml"), &main).unwrap();
    assert!(!d.run(&["adapters", "disable", "waybar"]).status.success());
    assert_eq!(fs::read(outside.join("config.toml")).unwrap(), original);
}

#[test]
fn pending_setup_recovers_even_with_malformed_current_config() {
    let d = Desktop::new();
    d.ready();
    let before = fs::read(d.config().join("config.toml")).unwrap();
    d.enable("waybar", &[]);
    let paths = d.paths();
    let mut state = State::load(&paths.state).unwrap();
    let id = state.head.clone().unwrap();
    let tx = backup::load(&paths.state, &id).unwrap();
    state.current = tx.before.clone();
    state.head = tx.parent.clone();
    state.pending = Some(chroma::engine::Pending {
        transaction: id,
        restoring: false,
    });
    state.save(&paths.state).unwrap();
    // Simulates interruption after the setup replacement but before commit.
    d.ok(&["rollback", "--no-reload"]);
    assert_eq!(fs::read(d.config().join("config.toml")).unwrap(), before);
    assert!(State::load(&paths.state).unwrap().pending.is_none());
}

#[test]
fn rollback_preserves_later_config_edits_and_reports_incomplete_recovery() {
    let d = Desktop::new();
    d.ready();
    d.enable("waybar", &[]);
    let main = d.config().join("config.toml");
    fs::write(&main, "broken config from outside Chroma").unwrap();
    let out = d.run(&["rollback", "--no-reload"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("incomplete rollback"));
    assert_eq!(
        fs::read_to_string(main).unwrap(),
        "broken config from outside Chroma"
    );
    assert!(State::load(&d.paths().state).unwrap().pending.is_some());
}

#[test]
fn unmanaged_output_conflict_blocks_all_adapters_before_any_replacement() {
    let d = Desktop::new();
    d.ready();
    d.enable("waybar", &[]);
    d.enable("fuzzel", &["--opaque"]);
    fs::write(d.config().join("generated/waybar.css"), "user colors").unwrap();
    for mode in ["--dry-run", "--no-reload"] {
        let out = d.run(&["apply", "nord", mode]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("existing unmanaged"));
        assert!(!d.config().join("generated/fuzzel.ini").exists());
        assert_eq!(
            fs::read_to_string(d.config().join("generated/waybar.css")).unwrap(),
            "user colors"
        );
    }
}

#[test]
fn setup_never_executes_reload_and_palette_then_setup_rollback_stays_ordered() {
    let d = Desktop::new();
    d.ready();
    let marker = d.root.path().join("reload");
    fs::write(d.config().join("config.toml"),format!("[general]\nauto_reload=true\n[adapters.waybar]\nenabled=false\nreload=['/usr/bin/touch','{}']\n",marker.display())).unwrap();
    d.enable("waybar", &[]);
    assert!(!marker.exists());
    d.ok(&["apply", "nord", "--dry-run"]);
    assert!(!marker.exists());
    d.ok(&["apply", "nord", "--no-reload"]);
    assert!(!marker.exists());
    d.ok(&["adapters", "disable", "waybar"]);
    assert!(!marker.exists());
    d.ok(&["rollback", "--no-reload"]);
    assert!(chroma::config::Config::load(&d.paths()).unwrap().adapters["waybar"].enabled);
    d.ok(&["rollback", "--no-reload"]);
    assert!(!d.config().join("generated/waybar.css").exists());
    assert!(d.ok(&["current"]).contains("No palette has been applied"));
    d.ok(&["rollback", "--no-reload"]);
    assert!(!chroma::config::Config::load(&d.paths()).unwrap().adapters["waybar"].enabled);
}

#[test]
fn apply_dry_run_checks_source_and_target_symlinks_using_real_plan() {
    let d = Desktop::new();
    d.ready();
    d.enable("waybar", &[]);
    let outside = d.root.path().join("untouched");
    fs::write(&outside, "outside").unwrap();
    let output = d.config().join("generated/waybar.css");
    symlink(&outside, &output).unwrap();
    for flag in ["--dry-run", "--no-reload"] {
        assert!(!d.run(&["apply", "nord", flag]).status.success());
    }
    fs::remove_file(output).unwrap();
    let source = d.desktop().join("waybar/style.css");
    fs::remove_file(&source).unwrap();
    symlink(&outside, &source).unwrap();
    for flag in ["--dry-run", "--no-reload"] {
        assert!(!d.run(&["apply", "nord", flag]).status.success());
    }
    assert_eq!(fs::read_to_string(outside).unwrap(), "outside");
}

#[test]
fn complete_setup_and_palette_history_reports_only_actual_applies() {
    let d = Desktop::new();
    d.ready();
    assert!(d.ok(&["current"]).contains("No palette has been applied"));
    d.enable("waybar", &[]);
    d.enable("fuzzel", &["--opaque"]);
    d.ok(&["adapters", "disable", "fuzzel"]);
    for _ in 0..3 {
        assert!(d.ok(&["current"]).contains("No palette has been applied"));
        d.ok(&["rollback", "--no-reload"]);
    }
    assert!(d.ok(&["current"]).contains("No palette has been applied"));
    d.enable("waybar", &[]);
    d.ok(&["apply", "nord", "--no-reload"]);
    let active = State::load(&d.paths().state).unwrap().current.unwrap();
    d.enable("fuzzel", &["--opaque"]);
    d.ok(&["adapters", "disable", "waybar"]);
    for _ in 0..2 {
        let now = State::load(&d.paths().state).unwrap().current.unwrap();
        assert_eq!(now.id, active.id);
        assert_eq!(now.palette, active.palette);
        assert!(d.ok(&["current"]).contains("Nord (nord)"));
        d.ok(&["rollback", "--no-reload"]);
    }
    d.ok(&["rollback", "--no-reload"]);
    assert!(d.ok(&["current"]).contains("No palette has been applied"));
    d.ok(&["rollback", "--no-reload"]);
    assert!(State::load(&d.paths().state).unwrap().current.is_none());
}

#[test]
fn symlinked_adapter_directory_is_rejected_without_following_it() {
    let d = Desktop::new();
    d.ok(&["init"]);
    let directory = d.config().join("adapters");
    fs::remove_dir(&directory).unwrap();
    let outside = d.root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.toml"), "private malformed contents").unwrap();
    symlink(&outside, &directory).unwrap();
    let result = d.run(&["adapters", "list"]);
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("private malformed"));
}

#[test]
fn dry_run_checks_serialized_backup_limits_before_any_output_or_state_write() {
    let d = Desktop::new();
    d.ok(&["init"]);
    fs::write(d.config().join("templates/large"), vec![b'x'; 800_000]).unwrap();
    fs::write(
        d.config().join("config.toml"),
        "[adapters.large]\nenabled=true\ntemplate='templates/large'\noutput='generated/large'\n",
    )
    .unwrap();
    let before = snapshot(d.root.path());
    let preview = d.run(&["apply", "nord", "--dry-run"]);
    assert!(!preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stderr).contains("state/backup exceeds 4 MiB"));
    assert_eq!(snapshot(d.root.path()), before);
    let applied = d.run(&["apply", "nord", "--no-reload"]);
    assert!(!applied.status.success());
    assert!(String::from_utf8_lossy(&applied.stderr).contains("state/backup exceeds 4 MiB"));
    assert!(!d.config().join("generated/large").exists());
    assert!(!d.paths().state.join("transactions").exists());
}
