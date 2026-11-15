use chroma::{
    config::{Adapter, Config, Paths},
    engine::{apply, rollback},
    integration::{
        self,
        colors::{self, Kind},
    },
    palette::presets,
};
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;

const ISLAND: &str = include_str!("fixtures/desktop/island/style.css");
const WAYBAR: &str = include_str!("fixtures/desktop/waybar/style.css");
const FUZZEL: &str = "[main]\nfont=JetBrainsMono Nerd Font:size=12\nterminal=kitty -e\nwidth=40\nlines=10\n[colors]\nbackground=12140fee\ntext=c9c8b8ff\nprompt=6f7f4fdd\nplaceholder=6b6f60ff\ninput=c9c8b8ff\nmatch=b5683ccc\nselection=1a1d16ff\nselection-text=c9c8b8ff\nselection-match=b5683cff\nborder=6f7f4f88\n[border]\nwidth=1\nradius=0\n";
const NIRI: &str = "layout {\n gaps 10\n border {\n width 2\n active-color \"#6f7f4f\"\n inactive-color \"#2a2e22\"\n urgent-color \"#b5683c\"\n }\n background-color \"#12140f\"\n}\nanimations { slowdown 1.0; }\n";
const SWAYLOCK: &str = "screenshots\neffect-blur=10x5\neffect-vignette=0.4:0.8\nclock\nfont=JetBrainsMono Nerd Font\nindicator-radius=110\ninside-color=12140f99\ninside-clear-color=12140f88\ninside-ver-color=12140f77\ninside-wrong-color=12140f66\nring-color=2a2e22\nring-clear-color=b5683c\nring-ver-color=6f7f4f\nring-wrong-color=a34a3c\ntext-color=c9c8b8\nkey-hl-color=6f7f4f\nbs-hl-color=b5683c\nline-color=00000000\n";

fn kitty() -> String {
    let mut source = String::from(
        "font_family JetBrains Mono\nfont_size 13\nbackground #12140f\nforeground #c9c8b8\ncursor #6f7f4f\nselection_background #2a2e22\nselection_foreground #c9c8b8\n",
    );
    for i in 0..16 {
        source.push_str(&format!("color{i} #123456\n"));
    }
    source
}

fn fixture(root: &Path) -> (Paths, std::path::PathBuf) {
    let desktop = root.join("desktop");
    let paths = Paths::discover(Some(root.join("chroma")), None).unwrap();
    for (kind, source) in [
        (Kind::Island, ISLAND.into()),
        (Kind::Waybar, WAYBAR.into()),
        (Kind::Fuzzel, FUZZEL.into()),
        (Kind::Niri, NIRI.into()),
        (Kind::Kitty, kitty()),
        (Kind::Swaylock, SWAYLOCK.into()),
    ] {
        let path = desktop.join(kind.relative());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    (paths, desktop)
}

// Compare the complete byte skeleton, including comments, selectors, whitespace,
// every non-color declaration and the ordering of all declarations.
fn without_colors(kind: Kind, source: &str) -> String {
    let edits = colors::edits(kind, source).unwrap();
    let mut masked = source.to_string();
    for e in edits.iter().rev() {
        masked.replace_range(e.range.clone(), "<color>");
    }
    masked
}

#[test]
fn island_maps_every_color_and_preserves_every_non_color_byte() {
    let palette = presets::get("nord").unwrap();
    let rendered = colors::recolor(Kind::Island, ISLAND, &palette).unwrap();
    assert_eq!(colors::edits(Kind::Island, ISLAND).unwrap().len(), 11);
    assert!(rendered.contains(&format!("background: {};", palette.colors.background)));
    assert!(rendered.contains(&format!("border-color: {};", palette.colors.border)));
    assert!(rendered.contains(&format!(".title {{ color: {};", palette.colors.primary)));
    assert_eq!(
        without_colors(Kind::Island, ISLAND),
        without_colors(Kind::Island, &rendered)
    );
    assert_eq!(
        colors::recolor(Kind::Island, &rendered, &palette).unwrap(),
        rendered
    );
}

#[test]
fn css_alpha_short_hex_comments_and_unicode_are_preserved() {
    let palette = presets::get("rustwood").unwrap();
    let source = "/* Цвет #123 */\nisland { background: #abc42Faa; border-color: #abc; border-width: 1.2px; }\n.text { color: rgba(1, 2, 3, 0.275); font-size: 14px; }";
    let rendered = colors::recolor(Kind::Island, source, &palette).unwrap();
    assert!(rendered.contains(&format!("{}aa", palette.colors.background)));
    assert!(rendered.contains(", 0.275)"));
    assert_eq!(
        without_colors(Kind::Island, source),
        without_colors(Kind::Island, &rendered)
    );
}

#[test]
fn malformed_or_ambiguous_css_fails_closed() {
    for source in [
        "island { color: #123",
        "island { background #123; }",
        "island { background: #12; }",
        "/* unterminated",
        "@import \"colors.css\"; island { background: #123; }",
        ".title, .date { color: #123; }",
        ".text { color: rgba(1,2,3,NaN); }",
        ".text { color: rgba(1,2,3,1.5); }",
        ".text { color: red; }",
        "island { background: #123; } garbage",
    ] {
        assert!(
            colors::recolor(Kind::Island, source, &presets::get("nord").unwrap()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn waybar_uses_gtk_named_colors_and_keeps_appearance_properties() {
    let rendered = colors::waybar_migration(WAYBAR, "/tmp/chroma/waybar.css").unwrap();
    assert!(rendered.starts_with("@import \"/tmp/chroma/waybar.css\";"));
    for text in [
        "JetBrainsMono Nerd Font",
        "Symbols Nerd Font",
        "font-size: 13px",
        "padding: 0 5px",
        "border-bottom: 2px solid @chroma_primary",
        "background: transparent",
        "color: @chroma_warning",
        "color: @chroma_error",
    ] {
        assert!(rendered.contains(text), "{text}");
    }
    let edits = colors::edits(Kind::Waybar, WAYBAR).unwrap();
    let mut expected = WAYBAR.into();
    for e in edits.iter().rev() {
        expected = {
            let mut s: String = expected;
            s.replace_range(e.range.clone(), &format!("@chroma_{}", e.role));
            s
        };
    }
    assert_eq!(rendered.split_once('\n').unwrap().1, expected);
    let alpha = colors::waybar_migration(
        "window#waybar { background: rgba(1, 2, 3, 0.42); color: #abcdef80; }",
        "/tmp/p.css",
    )
    .unwrap();
    assert!(alpha.contains("alpha(@chroma_background, 0.42)"));
    assert!(alpha.contains("alpha(@chroma_foreground, 0.501960784)"));
    let diff = colors::waybar_diff(WAYBAR, &rendered, "style.css").unwrap();
    assert!(diff.contains("+    border-bottom: 2px solid @chroma_primary;"));
    assert!(!diff.contains("font-family"));
    // The source adapter must still work after its main CSS has been migrated.
    assert!(colors::render(Kind::Waybar, &rendered, &presets::get("nord").unwrap()).is_ok());
}

#[test]
fn completely_missing_desktop_is_a_read_only_empty_plan() {
    let root = tempdir().unwrap();
    let paths = Paths::discover(Some(root.path().join("chroma")), None).unwrap();
    let plan = integration::plan(
        &paths,
        &root.path().join("missing-desktop"),
        &presets::get("nord").unwrap(),
    )
    .unwrap();
    assert!(plan.changes.is_empty());
    assert_eq!(
        plan.reports
            .iter()
            .filter(|r| r.contains(": missing "))
            .count(),
        6
    );
    assert!(!paths.config.exists());
    assert!(!paths.state.exists());
}

#[test]
fn fuzzel_all_ten_alphas_and_geometry_survive() {
    let palette = presets::get("nord").unwrap();
    let rendered = colors::recolor(Kind::Fuzzel, FUZZEL, &palette).unwrap();
    assert_eq!(
        without_colors(Kind::Fuzzel, FUZZEL),
        without_colors(Kind::Fuzzel, &rendered)
    );
    let before = colors::edits(Kind::Fuzzel, FUZZEL).unwrap();
    let after = colors::edits(Kind::Fuzzel, &rendered).unwrap();
    for (a, b) in before.iter().zip(after) {
        assert_eq!(
            &FUZZEL[a.range.end - 2..a.range.end],
            &rendered[b.range.end - 2..b.range.end]
        );
    }
    assert!(
        colors::recolor(
            Kind::Fuzzel,
            &format!("{FUZZEL}\n[colors]\ntext=abcdef00"),
            &palette
        )
        .is_err()
    );
    integration::validate::generated(Kind::Fuzzel, FUZZEL.as_bytes(), rendered.as_bytes()).unwrap();
}

#[test]
fn kitty_maps_complete_ansi_palette_and_uses_color_only_include() {
    let source = kitty();
    let palette = presets::get("nord").unwrap();
    let rendered = colors::render(Kind::Kitty, &source, &palette).unwrap();
    for i in 0..16 {
        assert!(rendered.contains(&format!(
            "color{i} {}\n",
            palette.variables()[&format!("ansi{i}")]
        )));
    }
    assert!(!rendered.contains("font"));
    assert_eq!(
        without_colors(Kind::Kitty, &source),
        without_colors(
            Kind::Kitty,
            &colors::recolor(Kind::Kitty, &source, &palette).unwrap()
        )
    );
}

#[test]
fn niri_native_include_is_valid_and_only_changes_required_colors() {
    let rendered = colors::render(Kind::Niri, NIRI, &presets::get("nord").unwrap()).unwrap();
    assert!(rendered.contains("urgent-color"));
    for forbidden in [
        "width",
        "gaps",
        "focus-ring",
        "animations",
        "window-rule",
        "binds",
    ] {
        assert!(!rendered.contains(forbidden));
    }
    if chroma::adapters::reload::available("niri") {
        integration::validate::generated(Kind::Niri, NIRI.as_bytes(), rendered.as_bytes()).unwrap();
    }
}

#[test]
fn swaylock_states_and_transparency_preserve_blur_clock_and_dimensions() {
    let palette = presets::get("nord").unwrap();
    let rendered = colors::recolor(Kind::Swaylock, SWAYLOCK, &palette).unwrap();
    assert_eq!(
        without_colors(Kind::Swaylock, SWAYLOCK),
        without_colors(Kind::Swaylock, &rendered)
    );
    assert!(rendered.contains(&format!(
        "ring-wrong-color={}",
        &palette.colors.error.to_string()[1..]
    )));
    assert!(rendered.contains(&format!(
        "inside-color={}99",
        &palette.colors.background.to_string()[1..]
    )));
    assert!(rendered.contains("line-color=00000000"));
}

#[test]
fn planning_is_read_only_missing_configs_are_reported_and_scripts_untouched() {
    let root = tempdir().unwrap();
    let (paths, desktop) = fixture(root.path());
    let marker = desktop.join("island/config.jsonc");
    fs::write(&marker, "{\"animation\":42}").unwrap();
    fs::write(
        desktop.join("swaylock/config"),
        format!("exec swayidle -w \\\n  before-sleep 'swaylock -f'\n{SWAYLOCK}"),
    )
    .unwrap();
    fs::remove_file(desktop.join("kitty/kitty.conf")).unwrap();
    let plan = integration::plan(&paths, &desktop, &presets::get("nord").unwrap()).unwrap();
    assert!(!paths.config.exists());
    assert!(!paths.state.exists());
    assert!(plan.reports.iter().any(|s| s.contains("kitty: missing")));
    assert!(
        plan.reports
            .iter()
            .any(|s| s.contains("swaylock: unsupported/manual"))
    );
    assert!(!plan.changes.iter().any(|c| c.path.starts_with(&desktop)));
    assert_eq!(fs::read_to_string(marker).unwrap(), "{\"animation\":42}");
}

#[test]
fn confirmation_concurrent_inputs_and_existing_artifacts_are_checked() {
    let root = tempdir().unwrap();
    let (paths, desktop) = fixture(root.path());
    let palette = presets::get("nord").unwrap();
    assert!(
        integration::apply(
            &paths,
            integration::plan(&paths, &desktop, &palette).unwrap(),
            false
        )
        .is_err()
    );
    assert!(!paths.state.exists());
    let plan = integration::plan(&paths, &desktop, &palette).unwrap();
    fs::write(desktop.join("island/style.css"), "changed").unwrap();
    assert!(
        integration::apply(&paths, plan, true)
            .unwrap_err()
            .to_string()
            .contains("source changed")
    );
    let paths2 = Paths::discover(Some(root.path().join("existing")), None).unwrap();
    fs::create_dir_all(paths2.config.join("generated")).unwrap();
    fs::write(integration::output(&paths2, Kind::Niri), "user file").unwrap();
    assert!(integration::plan(&paths2, &desktop, &palette).is_err());
}

#[test]
fn bundle_is_disabled_idempotent_and_rollback_is_separate_from_palettes() {
    let root = tempdir().unwrap();
    let (paths, desktop) = fixture(root.path());
    let palette = presets::get("nord").unwrap();
    // Niri installation requires its noninteractive validator; skip only the Niri
    // fixture on build hosts without it, never simulate success.
    if !chroma::adapters::reload::available("niri") {
        fs::remove_file(desktop.join("niri/config.kdl")).unwrap();
    }
    let plan = integration::plan(&paths, &desktop, &palette).unwrap();
    let files: Vec<_> = plan.changes.iter().map(|c| c.path.clone()).collect();
    assert!(integration::apply(&paths, plan, true).unwrap() > 0);
    let mut config = Config::load(&paths).unwrap();
    assert!(config.adapters.values().all(|a| !a.enabled));
    assert!(
        integration::plan(&paths, &desktop, &palette)
            .unwrap()
            .changes
            .is_empty()
    );
    assert!(apply::apply(&paths, &config, "nord", palette, true).is_err());
    for adapter in config.adapters.values_mut() {
        adapter.enabled = true;
    }
    apply::apply(
        &paths,
        &config,
        "rustwood",
        presets::get("rustwood").unwrap(),
        true,
    )
    .unwrap();
    rollback::rollback(&paths, &config, true).unwrap();
    integration::rollback(&paths).unwrap();
    assert!(files.iter().all(|p| !p.exists()));
    assert!(!paths.state.join("integration.toml").exists());
    assert_eq!(
        fs::read_to_string(desktop.join("island/style.css")).unwrap(),
        ISLAND
    );
}

#[test]
fn rollback_preserves_later_edits_and_can_be_retried() {
    let root = tempdir().unwrap();
    let (paths, desktop) = fixture(root.path());
    fs::remove_file(desktop.join("niri/config.kdl")).unwrap();
    let plan = integration::plan(&paths, &desktop, &presets::get("nord").unwrap()).unwrap();
    integration::apply(&paths, plan, true).unwrap();
    let edited = integration::output(&paths, Kind::Island);
    let original = fs::read(&edited).unwrap();
    fs::write(&edited, "user changes").unwrap();
    assert!(integration::rollback(&paths).is_err());
    assert_eq!(fs::read(&edited).unwrap(), b"user changes");
    fs::write(&edited, original).unwrap();
    integration::rollback(&paths).unwrap();
    assert!(!edited.exists());
}

#[test]
fn cli_plan_status_and_no_adapter_diagnostics_never_write_desktop() {
    let root = tempdir().unwrap();
    let (paths, desktop) = fixture(root.path());
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_chroma"))
            .arg("--config-dir")
            .arg(&paths.config)
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&[
        "integrate",
        "--plan",
        "--desktop-dir",
        desktop.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("@chroma_primary"));
    assert!(run(&["integrate", "--status"]).status.success());
    assert!(
        !run(&[
            "integrate",
            "--apply",
            "--desktop-dir",
            desktop.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(!paths.config.exists());
    assert!(!paths.state.exists());
    let out = run(&["apply", "nord", "--dry-run"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("no enabled adapters"));
}

#[test]
fn desktop_adapter_cannot_overwrite_its_source_or_write_outside_generated() {
    let root = tempdir().unwrap();
    let (paths, desktop) = fixture(root.path());
    let adapter = Adapter {
        desktop: Some(Kind::Island),
        config_path: Some(desktop.join("island/style.css").to_str().unwrap().into()),
        output: Some(desktop.join("island/style.css").to_str().unwrap().into()),
        ..Default::default()
    };
    assert!(
        chroma::adapters::render(
            "desktop-island",
            &adapter,
            &paths,
            &presets::get("nord").unwrap()
        )
        .is_err()
    );
}
