use std::{
    fs,
    process::{Command, Output},
};
use tempfile::tempdir;

fn cli(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chroma"))
        .arg("--config-dir")
        .arg(root)
        .args(args)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .output()
        .unwrap()
}

#[test]
fn config_override_is_hermetic_and_defaults_are_read_only() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    for args in [vec!["list"], vec!["current"], vec!["validate"]] {
        let out = cli(&cfg, &args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!cfg.exists());
    }
    let list = cli(&cfg, &["list", "--names-only"]);
    assert_eq!(String::from_utf8(list.stdout).unwrap().lines().count(), 6);
    assert!(!cli(&cfg, &["apply", "missing"]).status.success());
    assert!(!cli(&cfg, &["apply", "../../bad"]).status.success());
    assert!(!cli(&cfg, &["rollback"]).status.success());
    assert_eq!(cli(&cfg, &["not-a-command"]).status.code(), Some(2));
    assert!(cli(&cfg, &["--help"]).status.success());
}

#[test]
fn init_is_idempotent_defaults_are_disabled_and_diagnostics_work() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    let text = fs::read(cfg.join("config.toml")).unwrap();
    assert!(cli(&cfg, &["init"]).status.success());
    assert_eq!(fs::read(cfg.join("config.toml")).unwrap(), text);
    assert!(cli(&cfg, &["validate"]).status.success());
    assert!(cli(&cfg, &["doctor"]).status.success());
    assert!(!cli(&cfg, &["apply", "rustwood"]).status.success());
    assert_eq!(fs::read_dir(cfg.join("generated")).unwrap().count(), 0);
}

#[test]
fn cli_apply_dry_run_current_and_rollback_need_no_desktop() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    fs::write(
        cfg.join("config.toml"),
        "[adapters.kitty]\nenabled = true\n",
    )
    .unwrap();
    assert!(cli(&cfg, &["apply", "nord", "--dry-run"]).status.success());
    assert!(!cfg.join("state").exists());
    assert!(!cfg.join("generated/kitty.conf").exists());
    let out = cli(&cfg, &["apply", "nord", "--no-reload"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(cfg.join("state/state.toml").exists());
    assert!(cfg.join("generated/kitty.conf").exists());
    let out = cli(&cfg, &["current"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("Nord (nord)")
    );
    assert!(cli(&cfg, &["rollback", "--no-reload"]).status.success());
    assert!(!cfg.join("generated/kitty.conf").exists());
}

#[test]
fn generated_palette_saves_and_overwrite_requires_force() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    let image = root.path().join("wall.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([160, 90, 40]))
        .save(&image)
        .unwrap();
    let args = ["generate", image.to_str().unwrap(), "--mode", "light"];
    let output = cli(&cfg, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cfg.join("palettes/generated.toml").exists());
    assert!(!cfg.join("state").exists());
    assert!(!cli(&cfg, &args).status.success());
    assert!(
        cli(&cfg, &["generate", image.to_str().unwrap(), "--force"])
            .status
            .success()
    );
    assert!(cli(&cfg, &["validate", "generated"]).status.success());
}

#[test]
fn malformed_config_and_unknown_keys_do_not_silently_enable_features() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    for text in [
        "not toml",
        "[general]\nauto_relod = true",
        "[adapters.foo]\nenabled = true\nreloadd = [\"foo\"]",
        "[adapters.foo]\nreload = [\"sh\",\"-c\",\"true\"]",
        "[adapters.foo]\nreload_timeout_ms = 0",
    ] {
        fs::write(cfg.join("config.toml"), text).unwrap();
        assert!(!cli(&cfg, &["validate"]).status.success(), "{text}");
    }
}

#[test]
fn standalone_adapter_files_and_duplicate_names_are_checked() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    fs::write(cfg.join("adapters/custom.toml"),"enabled = true\ntemplate = 'templates/dynamic-island.txt'\noutput = 'generated/custom.theme'\n").unwrap();
    assert!(cli(&cfg, &["validate"]).status.success());
    assert!(cli(&cfg, &["apply", "paper"]).status.success());
    fs::write(cfg.join("adapters/kitty.toml"), "enabled = false\n").unwrap();
    assert!(!cli(&cfg, &["validate"]).status.success());
}

#[test]
fn user_palette_overrides_embedded_preset() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    let source = fs::read_to_string(cfg.join("palettes/rustwood.toml")).unwrap();
    fs::write(
        cfg.join("palettes/rustwood.toml"),
        source.replace("Rustwood", "My Rustwood"),
    )
    .unwrap();
    let out = cli(&cfg, &["list"]);
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("My Rustwood")
    );
    fs::write(cfg.join("palettes/rustwood.toml"), "broken").unwrap();
    assert!(!cli(&cfg, &["validate"]).status.success());
}

#[test]
fn no_reload_and_dry_run_never_execute_configured_commands() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    let marker = root.path().join("executed");
    let config = format!(
        "[general]\nauto_reload = true\n[adapters.kitty]\nenabled = true\nreload = ['/usr/bin/touch', '{}']\n",
        marker.display()
    );
    fs::write(cfg.join("config.toml"), config).unwrap();
    assert!(cli(&cfg, &["apply", "nord", "--dry-run"]).status.success());
    assert!(!marker.exists());
    assert!(
        cli(&cfg, &["apply", "nord", "--no-reload"])
            .status
            .success()
    );
    assert!(!marker.exists());
    assert!(cli(&cfg, &["apply", "paper"]).status.success());
    assert!(marker.exists());
}

#[test]
fn doctor_reports_missing_configs_without_creating_them() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    let output = cli(&cfg, &["doctor"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("config.toml is missing")
    );
    assert!(!cfg.exists());
}

#[test]
fn emergency_rollback_works_with_broken_config_and_missing_templates() {
    let root = tempdir().unwrap();
    let cfg = root.path().join("isolated");
    assert!(cli(&cfg, &["init"]).status.success());
    fs::write(
        cfg.join("config.toml"),
        "[adapters.kitty]\nenabled = true\n",
    )
    .unwrap();
    assert!(cli(&cfg, &["apply", "nord"]).status.success());
    fs::write(cfg.join("config.toml"), "broken").unwrap();
    fs::remove_dir_all(cfg.join("templates")).unwrap();
    assert!(cli(&cfg, &["rollback", "--no-reload"]).status.success());
    assert!(!cfg.join("generated/kitty.conf").exists());
}

#[test]
fn explicit_xdg_paths_and_state_override_are_honored_without_desktop_access() {
    let root = tempdir().unwrap();
    let config = root.path().join("xdg-config");
    let state = root.path().join("xdg-state");
    let cache = root.path().join("xdg-cache");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_chroma"))
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_STATE_HOME", &state)
            .env("XDG_CACHE_HOME", &cache)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["init"]).status.success());
    let cfg = config.join("chroma");
    fs::write(cfg.join("config.toml"), "[adapters.kitty]\nenabled=true\n").unwrap();
    assert!(run(&["apply", "nord", "--no-reload"]).status.success());
    assert!(state.join("chroma/state.toml").exists());
    assert!(!cfg.join("state").exists());
    assert!(!cache.exists());
    assert!(run(&["rollback", "--no-reload"]).status.success());
    let override_state = root.path().join("override-state");
    assert!(
        run(&[
            "--config-dir",
            cfg.to_str().unwrap(),
            "--state-dir",
            override_state.to_str().unwrap(),
            "apply",
            "paper",
            "--no-reload"
        ])
        .status
        .success()
    );
    assert!(override_state.join("state.toml").exists());
}

#[test]
fn wallpaper_fifo_is_rejected_without_hanging() {
    let root = tempdir().unwrap();
    let fifo = root.path().join("image.png");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chroma"))
        .arg("--config-dir")
        .arg(root.path().join("cfg"))
        .arg("generate")
        .arg(&fifo)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(1));
            break;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("generator blocked on a FIFO");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!root.path().join("cfg").exists());
}

#[test]
fn untrusted_palette_errors_do_not_emit_terminal_control_sequences() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join("palettes")).unwrap();
    fs::write(
        root.path().join("palettes/invalid.toml"),
        b"name = \"\x1b]52;c;dGVzdA==\x07\"\n[colors]\n",
    )
    .unwrap();
    let output = cli(root.path(), &["validate", "invalid"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.iter().any(|b| *b == 0x1b || *b == 0x07));
}
