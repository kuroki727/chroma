use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
};

#[test]
fn selector_handles_option_like_palette_ids_without_opening_a_real_menu() {
    let root = tempfile::tempdir().unwrap();
    let cfg = root.path().join("cfg");
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let executable = env!("CARGO_BIN_EXE_chroma");
    assert!(
        Command::new(executable)
            .arg("--config-dir")
            .arg(&cfg)
            .arg("init")
            .status()
            .unwrap()
            .success()
    );
    fs::write(cfg.join("config.toml"), "[adapters.kitty]\nenabled=true\n").unwrap();
    fs::copy(
        cfg.join("palettes/rustwood.toml"),
        cfg.join("palettes/--help.toml"),
    )
    .unwrap();
    symlink(executable, bin.join("chroma")).unwrap();
    fs::write(bin.join("fuzzel"), "#!/bin/sh\nprintf '%s\\n' '--help'\n").unwrap();
    fs::set_permissions(bin.join("fuzzel"), fs::Permissions::from_mode(0o700)).unwrap();
    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&old_path))).unwrap();
    let output = Command::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/chroma-fuzzel"
    ))
    .arg("--config-dir")
    .arg(&cfg)
    .env("PATH", path)
    .env_remove("DISPLAY")
    .env_remove("WAYLAND_DISPLAY")
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cfg.join("generated/kitty.conf").exists());
    assert_eq!(
        chroma::engine::State::load(&cfg.join("state"))
            .unwrap()
            .current
            .unwrap()
            .id,
        "--help"
    );
}
