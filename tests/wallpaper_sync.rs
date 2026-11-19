use chroma::{
    config::Paths,
    engine::{State, backup},
    palette::generator::{self, Mode},
    safe_fs::Dir,
};
use image::{Rgb, RgbImage};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::{TempDir, tempdir};

struct Fixture {
    root: TempDir,
}

#[test]
fn island_hook_reloads_complete_standalone_stylesheet_and_recovers_failures() {
    let f = Fixture::new(true);
    let source = f.root.path().join("style.css");
    fs::write(&source, include_str!("fixtures/desktop/island/style.css")).unwrap();
    let calls = f.root.path().join("reloads");
    let reload = f.script(
        "island-reload",
        &format!("printf 'reload\\n' >> '{}'", calls.display()),
    );
    let adapter = f.config().join("adapters/desktop-island.toml");
    fs::create_dir_all(adapter.parent().unwrap()).unwrap();
    let settings = format!(
        "enabled=true\ndesktop='island'\nisland_readable=true\nconfig_path='{}'\noutput='generated/desktop-island.css'\nreload=['{}']\n",
        source.display(),
        reload
    );
    fs::write(&adapter, &settings).unwrap();
    f.changed("a.png");
    let css = f.config().join("generated/desktop-island.css");
    let first = fs::read(&css).unwrap();
    let text = String::from_utf8(first.clone()).unwrap();
    for selector in ["flash", "dnd", "tile", "tile-on"] {
        assert!(text.contains(&format!("{selector} {{")));
    }
    assert!(!text.contains("@import") && !text.contains("var("));
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 1);
    let manifest = f.manifest();
    f.changed("a.png");
    assert_eq!(f.manifest(), manifest);
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 1);
    fs::write(&adapter, format!("{settings}validate=['/usr/bin/false']\n")).unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("b.png")])
            .status
            .success()
    );
    assert_eq!(fs::read(&css).unwrap(), first);
    assert_eq!(f.manifest(), manifest);
    fs::write(&adapter, settings.replace(&reload, "/usr/bin/false")).unwrap();
    let failed = f.run(&["wallpaper", "changed", &f.image("b.png")]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("desktop-island"));
    assert_eq!(fs::read(&css).unwrap(), first);
    assert_eq!(f.manifest(), manifest);
    fs::write(&adapter, &settings).unwrap();
    f.changed("b.png");
    assert_ne!(fs::read(&css).unwrap(), first);
    f.ok(&["rollback", "--no-reload"]);
    assert_eq!(fs::read(&css).unwrap(), first);
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        include_str!("fixtures/desktop/island/style.css")
    );
}

#[test]
fn unchanged_adapter_is_not_reloaded_during_apply_or_rollback() {
    let f = Fixture::new(true);
    fs::write(f.config().join("templates/static"), "unchanged\n").unwrap();
    let adapter = f.config().join("adapters/static.toml");
    fs::create_dir_all(adapter.parent().unwrap()).unwrap();
    let settings = "enabled=true\ntemplate='templates/static'\noutput='generated/static'\nreload=['/usr/bin/true']\n";
    fs::write(&adapter, settings).unwrap();
    f.changed("a.png");
    fs::write(
        &adapter,
        settings.replace("/usr/bin/true", "/usr/bin/false"),
    )
    .unwrap();
    f.changed("b.png");
    f.ok(&["rollback"]);
    f.assert_palette("a.png");
}
impl Fixture {
    fn new(enabled: bool) -> Self {
        let f = Self {
            root: tempdir().unwrap(),
        };
        f.ok(&["init"]);
        f.write_config(enabled, 0, "");
        for (name, color) in [("a.png", [40, 120, 65]), ("b.png", [190, 75, 30])] {
            RgbImage::from_pixel(48, 32, Rgb(color))
                .save(f.root.path().join(name))
                .unwrap();
        }
        f
    }
    fn config(&self) -> PathBuf {
        self.root.path().join("config")
    }
    fn paths(&self) -> Paths {
        Paths::discover(Some(self.config()), None).unwrap()
    }
    fn image(&self, name: &str) -> String {
        self.root.path().join(name).to_str().unwrap().into()
    }
    fn write_config(&self, enabled: bool, debounce: u64, extra: &str) {
        fs::write(self.config().join("config.toml"), format!("# Keep this comment\n[general]\nauto_reload=true\n[wallpaper]\nenabled={enabled} # hook switch\ndebounce_ms={debounce}\nwait_timeout_ms=5000\n[adapters.kitty]\nenabled=true\n{extra}\n[adapters.foot]\nenabled=true\n")).unwrap();
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_chroma"));
        cmd.arg("--config-dir")
            .arg(self.config())
            .args(args)
            .env("HOME", self.root.path())
            .env("XDG_CONFIG_HOME", self.root.path().join("xdg"))
            .env("XDG_STATE_HOME", self.root.path().join("state"))
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env_remove("NIRI_SOCKET");
        cmd
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        success(self.run(args))
    }
    fn changed(&self, image: &str) -> String {
        self.ok(&["wallpaper", "changed", &self.image(image)])
    }
    fn spawn(&self, image: &str) -> Child {
        self.command(&["wallpaper", "changed", &self.image(image)])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
    fn script(&self, name: &str, text: &str) -> String {
        let path = self.root.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{text}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path.to_str().unwrap().into()
    }
    fn state(&self) -> State {
        State::load(&self.paths().state).unwrap()
    }
    fn marker(&self) -> PathBuf {
        self.paths().state.join("wallpaper-sync/latest")
    }
    fn manifest(&self) -> Vec<u8> {
        fs::read(self.paths().state.join("state.toml")).unwrap()
    }
    fn outputs(&self) -> Vec<Vec<u8>> {
        ["kitty.conf", "foot.ini"]
            .map(|s| fs::read(self.config().join("generated").join(s)).unwrap())
            .into()
    }
    fn assert_palette(&self, image: &str) {
        assert_eq!(
            self.state().current.unwrap().palette,
            generator::generate(Path::new(&self.image(image)), Mode::Dark, "Wallpaper").unwrap()
        );
    }
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < end, "barrier timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn disabled_is_default_and_never_opens_image_or_creates_state() {
    let root = tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_chroma"))
        .arg("--config-dir")
        .arg(root.path().join("absent"))
        .args(["wallpaper", "changed", "/missing.png"])
        .output()
        .unwrap();
    assert!(success(out).contains("disabled"));
    assert!(!root.path().join("absent").exists());
    let f = Fixture::new(false);
    assert!(f.changed("missing.png").contains("disabled"));
    assert!(!f.paths().state.exists());
}

#[test]
fn png_and_jpeg_apply_dark_readable_deterministic_palettes() {
    let f = Fixture::new(true);
    RgbImage::from_pixel(32, 48, Rgb([40, 70, 130]))
        .save(f.root.path().join("c.jpg"))
        .unwrap();
    for image in ["a.png", "b.png", "c.jpg"] {
        f.changed(image);
        f.assert_palette(image);
        let palette = f.state().current.unwrap().palette;
        assert!(
            palette
                .colors
                .foreground
                .contrast(palette.colors.background)
                >= 4.5
        );
        assert!(palette.colors.background.luminance() < 0.05);
    }
}

#[test]
fn identical_inputs_skip_transaction_reload_and_output_replacement() {
    let f = Fixture::new(true);
    let reload = f.script("reload", "printf x >> \"$0.log\"");
    f.write_config(true, 0, &format!("reload=[{reload:?}]"));
    f.changed("a.png");
    let state = f.manifest();
    let metadata = fs::metadata(f.config().join("generated/kitty.conf"))
        .unwrap()
        .modified()
        .unwrap();
    fs::copy(f.image("a.png"), f.image("same bytes other name.png")).unwrap();
    for name in ["a.png", "same bytes other name.png", "a.png"] {
        assert!(f.changed(name).contains("already applied"));
    }
    assert_eq!(f.manifest(), state);
    assert_eq!(fs::read(format!("{reload}.log")).unwrap(), b"x");
    assert_eq!(
        fs::metadata(f.config().join("generated/kitty.conf"))
            .unwrap()
            .modified()
            .unwrap(),
        metadata
    );
    assert_eq!(
        fs::read_dir(f.paths().state.join("transactions"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn dry_run_writes_no_state_request_outputs_or_reload() {
    let f = Fixture::new(true);
    let reload = f.script("reload", "exit 77");
    f.write_config(true, 0, &format!("reload=[{reload:?}]"));
    f.ok(&["wallpaper", "changed", &f.image("a.png"), "--dry-run"]);
    assert!(!f.paths().state.exists());
    assert_eq!(
        fs::read_dir(f.config().join("generated")).unwrap().count(),
        0
    );
}

#[test]
fn invalid_missing_and_transparent_images_preserve_previous_theme() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let state = f.manifest();
    let outputs = f.outputs();
    fs::write(f.image("corrupt.png"), b"not an image").unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 0]))
        .save(f.image("transparent.png"))
        .unwrap();
    for name in ["missing.png", "corrupt.png", "transparent.png"] {
        assert!(
            !f.run(&["wallpaper", "changed", &f.image(name)])
                .status
                .success()
        );
        assert_eq!(f.manifest(), state);
        assert_eq!(f.outputs(), outputs);
    }
}

#[test]
fn rollback_then_same_wallpaper_reapplies_without_stale_deduplication() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let a = f.outputs();
    f.changed("b.png");
    let b = f.outputs();
    assert_ne!(a, b);
    f.ok(&["rollback", "--no-reload"]);
    assert_eq!(f.outputs(), a);
    f.changed("b.png");
    assert_eq!(f.outputs(), b);
    f.ok(&["rollback", "--no-reload"]);
    f.ok(&["rollback", "--no-reload"]);
    assert!(!f.config().join("generated/kitty.conf").exists());
}

#[test]
fn rapid_events_coalesce_to_latest_registered_image_under_the_shared_lock() {
    let f = Fixture::new(true);
    let dir = Dir::open(&f.paths().state, true).unwrap();
    let lock = dir.lock().unwrap();
    let first = f.spawn("a.png");
    wait_until(|| f.marker().exists());
    let token = fs::read(f.marker()).unwrap();
    let second = f.spawn("b.png");
    wait_until(|| fs::read(f.marker()).unwrap() != token);
    drop(lock);
    assert!(success(first.wait_with_output().unwrap()).contains("superseded"));
    success(second.wait_with_output().unwrap());
    f.assert_palette("b.png");
    assert_eq!(
        fs::read_dir(f.paths().state.join("transactions"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn concurrent_identical_invocations_make_one_transaction() {
    let f = Fixture::new(true);
    let workers: Vec<_> = (0..8).map(|_| f.spawn("a.png")).collect();
    for worker in workers {
        success(worker.wait_with_output().unwrap());
    }
    f.assert_palette("a.png");
    assert_eq!(
        fs::read_dir(f.paths().state.join("transactions"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn newest_event_arriving_during_reload_applies_after_inflight_transaction() {
    let f = Fixture::new(true);
    let barrier = f.script(
        "barrier",
        "touch \"$0.entered\"\nwhile ! test -e \"$0.release\"; do sleep 0.01; done",
    );
    f.write_config(true, 0, &format!("reload=[{barrier:?}]"));
    let first = f.spawn("a.png");
    wait_until(|| Path::new(&format!("{barrier}.entered")).exists());
    let token = fs::read(f.marker()).unwrap();
    let second = f.spawn("b.png");
    wait_until(|| fs::read(f.marker()).unwrap() != token);
    fs::write(format!("{barrier}.release"), b"").unwrap();
    success(first.wait_with_output().unwrap());
    success(second.wait_with_output().unwrap());
    f.assert_palette("b.png");
}

#[test]
fn failed_validator_and_template_preserve_state_outputs_and_history() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let state = f.manifest();
    let outputs = f.outputs();
    let validator = f.script("validator", "test -f \"$1\" || exit 99\nexit 1");
    f.write_config(true, 0, &format!("validate=[{validator:?}, '{{file}}']"));
    let out = f.run(&["wallpaper", "changed", &f.image("b.png")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("validation failed"));
    assert_eq!(f.manifest(), state);
    assert_eq!(f.outputs(), outputs);
    f.write_config(true, 0, "");
    fs::write(f.config().join("templates/kitty.conf"), "{{ nonexistent }}").unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("b.png")])
            .status
            .success()
    );
    assert_eq!(f.manifest(), state);
    assert_eq!(f.outputs(), outputs);
    assert_eq!(
        fs::read_dir(f.paths().state.join("transactions"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn validator_receives_temporary_generated_file_and_cannot_rewrite_it() {
    let f = Fixture::new(true);
    let validator = f.script("validate bytes", "test -f \"$1\" || exit 99\ngrep '^background #' \"$1\" >/dev/null || exit 88\nprintf x >> \"$1\"");
    f.write_config(true, 0, &format!("validate=[{validator:?}, '{{file}}']"));
    let out = f.run(&["wallpaper", "changed", &f.image("a.png"), "--dry-run"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("validator modified"));
    assert!(!f.paths().state.exists());
}

#[test]
fn reload_failure_restores_previous_files_and_state_then_allows_retry() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let state = f.manifest();
    let outputs = f.outputs();
    let fail = f.script("fail", "exit 1");
    f.write_config(true, 0, &format!("reload=[{fail:?}]"));
    let out = f.run(&["wallpaper", "changed", &f.image("b.png")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("previous files and state restored"));
    assert_eq!(f.manifest(), state);
    assert_eq!(f.outputs(), outputs);
    assert!(f.state().pending.is_none());
    f.ok(&["wallpaper", "changed", &f.image("b.png"), "--no-reload"]);
    f.assert_palette("b.png");
}

#[test]
fn interrupted_hook_requires_existing_rollback_recovery_before_next_apply() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let previous = f.manifest();
    let outputs = f.outputs();
    let barrier = f.script(
        "kill-barrier",
        "touch \"$0.entered\"\nwhile ! test -e \"$0.release\"; do sleep 0.01; done",
    );
    f.write_config(true, 0, &format!("reload=[{barrier:?}]"));
    let mut child = f.spawn("b.png");
    wait_until(|| Path::new(&format!("{barrier}.entered")).exists());
    assert!(f.state().pending.is_some());
    child.kill().unwrap();
    child.wait().unwrap();
    fs::write(format!("{barrier}.release"), b"").unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("a.png")])
            .status
            .success()
    );
    // Recovery bypasses even broken local adapter configuration.
    fs::write(f.config().join("config.toml"), "broken TOML = [").unwrap();
    f.ok(&["rollback", "--no-reload"]);
    assert_eq!(f.manifest(), previous);
    assert_eq!(f.outputs(), outputs);
}

#[test]
fn conflicting_external_edit_retains_pending_recovery_and_is_never_overwritten() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let outputs = f.outputs();
    let target = f.config().join("generated/foot.ini");
    let fail = f.script(
        "conflict",
        &format!("printf 'user edit' > {target:?}\nexit 1"),
    );
    f.write_config(true, 0, &format!("reload=[{fail:?}]"));
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("b.png")])
            .status
            .success()
    );
    assert_eq!(fs::read(&target).unwrap(), b"user edit");
    let state = f.state();
    let pending = state.pending.unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("a.png")])
            .status
            .success()
    );
    let tx = backup::load(&f.paths().state, &pending.transaction).unwrap();
    // Simulate explicit user reconciliation to the pending transaction bytes.
    let entry = tx.entries.iter().find(|e| e.path == target).unwrap();
    fs::write(&target, &entry.after.bytes).unwrap();
    f.ok(&["rollback", "--no-reload"]);
    assert_eq!(f.outputs(), outputs);
    assert!(f.state().pending.is_none());
}

#[test]
fn edited_outputs_are_preserved_even_with_manual_overwrite_authorization() {
    let f = Fixture::new(true);
    f.changed("a.png");
    f.write_config(true, 0, "allow_overwrite=true");
    let state = f.manifest();
    let target = f.config().join("generated/kitty.conf");
    fs::write(&target, b"# my customization").unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("b.png")])
            .status
            .success()
    );
    assert_eq!(fs::read(&target).unwrap(), b"# my customization");
    assert_eq!(f.manifest(), state);
}

#[test]
fn controls_preserve_comments_modes_outputs_and_support_dry_run_and_rollback() {
    let f = Fixture::new(false);
    let original = fs::read(f.config().join("config.toml")).unwrap();
    f.ok(&["wallpaper", "enable", "--dry-run"]);
    assert_eq!(fs::read(f.config().join("config.toml")).unwrap(), original);
    assert!(!f.paths().state.exists());
    f.ok(&["wallpaper", "enable"]);
    assert!(f.ok(&["wallpaper", "status"]).contains("enabled"));
    assert!(
        fs::read_to_string(f.config().join("config.toml"))
            .unwrap()
            .contains("enabled=true # hook switch")
    );
    f.changed("a.png");
    let outputs = f.outputs();
    f.ok(&["wallpaper", "disable"]);
    let state = f.manifest();
    f.changed("b.png");
    assert_eq!(f.manifest(), state);
    assert_eq!(f.outputs(), outputs);
    f.ok(&["rollback", "--no-reload"]);
    assert!(f.ok(&["wallpaper", "status"]).contains("enabled"));
    assert!(
        fs::read_to_string(f.config().join("config.toml"))
            .unwrap()
            .starts_with("# Keep this comment")
    );
}

#[test]
fn lock_timeout_and_unsafe_queue_fail_without_theme_mutation() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let state = f.manifest();
    let outputs = f.outputs();
    let config = fs::read_to_string(f.config().join("config.toml"))
        .unwrap()
        .replace("wait_timeout_ms=5000", "wait_timeout_ms=10");
    fs::write(f.config().join("config.toml"), config).unwrap();
    let dir = Dir::open(&f.paths().state, false).unwrap();
    let lock = dir.lock().unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("b.png")])
            .status
            .success()
    );
    drop(lock);
    fs::remove_file(f.marker()).unwrap();
    symlink(f.root.path().join("victim"), f.marker()).unwrap();
    assert!(
        !f.run(&["wallpaper", "changed", &f.image("b.png")])
            .status
            .success()
    );
    assert!(!f.root.path().join("victim").exists());
    assert_eq!(f.manifest(), state);
    assert_eq!(f.outputs(), outputs);
}

#[test]
fn awww_wrapper_hooks_only_after_success_and_quotes_image_paths() {
    let root = tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for (name, script) in [
        (
            "awww",
            "#!/bin/sh\nprintf '%s\\n' manager \"$@\" >> \"$HOOK_LOG\"\nexit \"$MANAGER_STATUS\"\n",
        ),
        (
            "chroma",
            "#!/bin/sh\nprintf '%s\\n' hook \"$@\" >> \"$HOOK_LOG\"\nexit \"$HOOK_STATUS\"\n",
        ),
    ] {
        let path = bin.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let log = root.path().join("log");
    let run = |manager: &str, hook: &str| {
        Command::new("/bin/sh")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/chroma-wall"))
            .args(["image with spaces;literal.png", "--transition-type", "fade"])
            .current_dir(root.path())
            .env("PATH", &bin)
            .env("HOOK_LOG", &log)
            .env("MANAGER_STATUS", manager)
            .env("HOOK_STATUS", hook)
            .output()
            .unwrap()
    };
    assert_eq!(run("7", "0").status.code(), Some(7));
    assert!(!fs::read_to_string(&log).unwrap().contains("hook"));
    fs::write(&log, b"").unwrap();
    success(run("0", "0"));
    let image = root.path().join("image with spaces;literal.png");
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        format!(
            "manager\nimg\n{}\n--transition-type\nfade\nhook\nwallpaper\nchanged\n{}\n",
            image.display(),
            image.display()
        )
    );
    assert_eq!(run("0", "9").status.code(), Some(9));
}

#[test]
fn legacy_config_enablement_adds_only_the_switch_and_rollback_restores_exact_bytes() {
    let f = Fixture::new(false);
    let original = b"# Legacy config without wallpaper table\n[general]\nauto_reload=false\n";
    let path = f.config().join("config.toml");
    fs::write(&path, original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    f.ok(&["wallpaper", "enable"]);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let state = f.manifest();
    f.ok(&["wallpaper", "enable"]);
    assert_eq!(f.manifest(), state);
    f.ok(&["rollback", "--no-reload"]);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn generated_niri_validation_rejects_include_chains_and_invalid_kdl_before_writes() {
    let f = Fixture::new(true);
    let mut config = fs::read_to_string(f.config().join("config.toml")).unwrap();
    config.push_str("\n[adapters.niri]\nenabled=true\n");
    fs::write(f.config().join("config.toml"), config).unwrap();
    let target = f.config().join("templates/niri.kdl");
    for source in [
        "include \"/should/not/be/read\"\n",
        "layout { definitely-invalid-property 1; }\n",
    ] {
        fs::write(&target, source).unwrap();
        assert!(
            !f.run(&["wallpaper", "changed", &f.image("a.png")])
                .status
                .success()
        );
        assert!(!f.paths().state.join("state.toml").exists());
        assert_eq!(
            fs::read_dir(f.config().join("generated")).unwrap().count(),
            0
        );
    }
}

#[test]
fn disablement_while_waiting_is_rechecked_under_the_writer_lock() {
    let f = Fixture::new(true);
    let dir = Dir::open(&f.paths().state, true).unwrap();
    let lock = dir.lock().unwrap();
    let worker = f.spawn("a.png");
    wait_until(|| f.marker().exists());
    f.write_config(false, 0, "");
    drop(lock);
    success(worker.wait_with_output().unwrap());
    assert!(!f.paths().state.join("state.toml").exists());
    assert_eq!(
        fs::read_dir(f.config().join("generated")).unwrap().count(),
        0
    );
}

#[test]
fn validator_timeout_preserves_theme_and_validation_configuration_is_checked() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let state = f.manifest();
    let outputs = f.outputs();
    let validator = f.script("slow-validator", "sleep 0.1");
    f.write_config(
        true,
        0,
        &format!("validate=[{validator:?}, '{{file}}']\nvalidation_timeout_ms=10"),
    );
    let out = f.run(&["wallpaper", "changed", &f.image("b.png")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("timed out"));
    assert_eq!(f.manifest(), state);
    assert_eq!(f.outputs(), outputs);
    for extra in [
        "validate=['sh', '-c', '{file}']",
        "validate=['test']",
        "validation_timeout_ms=0",
    ] {
        f.write_config(true, 0, extra);
        assert!(
            !f.run(&["wallpaper", "changed", &f.image("b.png")])
                .status
                .success()
        );
    }
}

struct ResumeOnDrop(rustix::process::Pid);
impl Drop for ResumeOnDrop {
    fn drop(&mut self) {
        let _ = rustix::process::kill_process(self.0, rustix::process::Signal::CONT);
    }
}

fn pause_worker(worker: &Child) -> ResumeOnDrop {
    let pid = rustix::process::Pid::from_raw(worker.id() as i32).unwrap();
    let guard = ResumeOnDrop(pid);
    rustix::process::kill_process(pid, rustix::process::Signal::STOP).unwrap();
    wait_until(|| {
        fs::read_to_string(format!("/proc/{}/status", worker.id()))
            .unwrap()
            .lines()
            .any(|line| line.starts_with("State:") && line.contains('T'))
    });
    guard
}

#[test]
fn manual_rollback_cancels_an_already_registered_waiting_hook() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let restored = f.outputs();
    f.changed("b.png");
    let dir = Dir::open(&f.paths().state, false).unwrap();
    let lock = dir.lock().unwrap();
    let before_token = fs::read(f.marker()).unwrap();
    let worker = f.spawn("b.png");
    wait_until(|| fs::read(f.marker()).unwrap() != before_token);
    let stopped = pause_worker(&worker);
    drop(lock);
    let rollback = f.run(&["rollback", "--no-reload"]);
    let state = f.manifest();
    drop(stopped);
    success(worker.wait_with_output().unwrap());
    success(rollback);
    assert_eq!(f.outputs(), restored, "waiting hook must not undo rollback");
    assert_eq!(f.manifest(), state);
    // A genuinely new event after rollback still works.
    f.changed("b.png");
    f.assert_palette("b.png");
}

#[test]
fn manual_palette_selection_cancels_an_already_registered_waiting_hook() {
    let f = Fixture::new(true);
    let dir = Dir::open(&f.paths().state, true).unwrap();
    let lock = dir.lock().unwrap();
    let worker = f.spawn("b.png");
    wait_until(|| f.marker().exists());
    let stopped = pause_worker(&worker);
    drop(lock);
    let manual = f.run(&["apply", "nord", "--no-reload"]);
    let selected = f.manifest();
    drop(stopped);
    success(worker.wait_with_output().unwrap());
    success(manual);
    assert_eq!(
        f.manifest(),
        selected,
        "waiting hook must not undo manual selection"
    );
}

#[test]
fn automatically_deleted_managed_output_is_preserved_as_a_user_change() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let before = f.manifest();
    let removed = f.config().join("generated/kitty.conf");
    fs::remove_file(&removed).unwrap();
    let output = f.run(&["wallpaper", "changed", &f.image("b.png")]);
    assert!(
        !output.status.success(),
        "automatic hook must not recreate a deleted managed output"
    );
    assert!(!removed.exists());
    assert_eq!(f.manifest(), before);
}

#[test]
fn a_validator_edit_during_a_duplicate_request_cannot_be_reported_as_already_applied() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let before = f.manifest();
    let target = f.config().join("generated/kitty.conf");
    let validator = f.script("edit-output", &format!("printf 'user change' > {target:?}"));
    f.write_config(true, 0, &format!("validate=[{validator:?}, '{{file}}']"));
    let output = f.run(&["wallpaper", "changed", &f.image("a.png")]);
    assert!(!output.status.success());
    assert_eq!(fs::read(target).unwrap(), b"user change");
    assert_eq!(f.manifest(), before);
}

#[test]
fn corrupt_hook_coordination_does_not_block_emergency_file_recovery() {
    let f = Fixture::new(true);
    f.changed("a.png");
    let before = f.outputs();
    f.changed("b.png");
    fs::remove_file(f.marker()).unwrap();
    symlink(f.root.path().join("must-not-be-written"), f.marker()).unwrap();
    fs::write(f.config().join("config.toml"), "broken config = [").unwrap();
    f.ok(&["rollback", "--no-reload"]);
    assert_eq!(f.outputs(), before);
    assert!(!f.root.path().join("must-not-be-written").exists());
}
