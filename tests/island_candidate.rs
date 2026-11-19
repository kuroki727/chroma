use chroma::{
    integration::{
        colors::{self, Kind},
        island,
    },
    palette::{Palette, presets},
};
const SOURCE: &str = include_str!("fixtures/desktop/island/style.css");
fn boats() -> Palette {
    Palette::parse(
        r##"name="Boats Preview test"
[colors]
background="#0E0B09"
surface="#1C1816"
surface_alt="#2A2624"
foreground="#F4F1EF"
muted="#938E8B"
primary="#DE9961"
secondary="#FFBF86"
border="#75706D"
error="#C2827C"
warning="#ECD69A"
success="#85B286"
"##,
    )
    .unwrap()
}
#[test]
fn profile_covers_actual_text_surfaces_hover_danger_and_slider_pairs() {
    for palette in [boats(), presets::get("rustwood").unwrap()] {
        let p = island::profile(&palette).unwrap();
        for bg in [
            p["background"],
            p["tile"],
            p["tile-on"],
            island::hover(p["tile"]),
            island::hover(p["tile-on"]),
        ] {
            for key in ["foreground", "muted", "title"] {
                assert!(p[key].contrast(bg) >= 4.5, "{} {key} {bg}", palette.name);
            }
        }
        for bg in [p["dnd"], island::hover(p["dnd"])] {
            assert!(p["foreground"].contrast(bg) >= 4.5);
        }
        for key in ["border", "dnd", "track"] {
            assert!(p[key].contrast(p["background"]) >= 3.0);
        }
        for key in ["fill", "title", "muted"] {
            assert!(p[key].contrast(p["track"]) >= 3.0);
        }
        assert!(palette.colors.foreground.contrast(palette.colors.primary) < 4.5);
    }
}
#[test]
fn rendering_preserves_every_existing_non_color_byte_and_completes_missing_selectors() {
    let result = island::render(SOURCE, &boats()).unwrap();
    let before = colors::edits(Kind::Island, SOURCE).unwrap();
    let after = colors::edits(Kind::Island, &result).unwrap();
    let mut expected = SOURCE.to_string();
    let mut actual = result[..SOURCE.len()].to_string();
    for e in before.iter().rev() {
        expected.replace_range(e.range.clone(), "<color>");
    }
    for e in after.iter().filter(|e| e.range.end <= SOURCE.len()).rev() {
        actual.replace_range(e.range.clone(), "<color>");
    }
    assert_eq!(actual, expected);
    for selector in ["flash", "dnd", "tile", "tile-on"] {
        assert!(result.contains(&format!("{selector} {{")));
    }
    assert_eq!(island::render(&result, &boats()).unwrap(), result);
}
#[test]
fn unknown_css_transparency_variables_and_invalid_sizes_are_refused() {
    for css in [
        "@import \"x\";",
        ".bogus {color:#123456;}",
        ".tile:hover {background:#123456;}",
        ".text {color:var(--x);}",
        ".text {color:#12345688;}",
        ".text {color:#éx;}",
        ".text {font-size:NaNpx;}",
        ".text {font-size:1px;}",
        "island {border-width:-2px;}",
        "/* open",
        ".tile {background:#abc",
    ] {
        assert!(island::render(css, &boats()).is_err(), "{css}");
    }
}
#[test]
fn legacy_adapter_is_unchanged_and_profile_flag_is_explicit() {
    let old = colors::render(Kind::Island, SOURCE, &boats()).unwrap();
    assert_eq!(
        old,
        colors::recolor(Kind::Island, SOURCE, &boats()).unwrap()
    );
    assert!(!old.contains("tile-on"));
    let adapter: chroma::config::Adapter =
        toml::from_str("desktop='island'\nisland_readable=true\n").unwrap();
    assert!(adapter.island_readable);
}

#[test]
fn opt_in_adapter_uses_profile_tracks_inputs_and_refuses_missing_sources() {
    let dir = tempfile::tempdir().unwrap();
    let paths = chroma::config::Paths::discover(Some(dir.path().join("chroma")), None).unwrap();
    let source = dir.path().join("style.css");
    std::fs::write(&source, SOURCE).unwrap();
    let mut adapter = chroma::config::Adapter {
        enabled: true,
        desktop: Some(Kind::Island),
        island_readable: true,
        config_path: Some(source.to_str().unwrap().into()),
        output: Some("generated/island.css".into()),
        ..Default::default()
    };
    let output = chroma::adapters::render("island", &adapter, &paths, &boats()).unwrap();
    assert_eq!(
        output.bytes,
        island::render(SOURCE, &boats()).unwrap().as_bytes()
    );
    std::fs::write(&source, "user edit").unwrap();
    assert!(output.inputs.check().is_err());
    std::fs::remove_file(&source).unwrap();
    assert!(chroma::adapters::render("island", &adapter, &paths, &boats()).is_err());
    adapter.desktop = Some(Kind::Waybar);
    assert!(chroma::adapters::render("island", &adapter, &paths, &boats()).is_err());
}

#[test]
fn isolated_profile_transactions_and_rollback_preserve_original_configurations() {
    let dir = tempfile::tempdir().unwrap();
    let paths = chroma::config::Paths::discover(Some(dir.path().join("chroma")), None).unwrap();
    std::fs::create_dir_all(paths.config.join("generated")).unwrap();
    let source = dir.path().join("style.css");
    std::fs::write(&source, SOURCE).unwrap();
    let json = dir.path().join("config.jsonc");
    std::fs::write(&json, "{\"hover\":true}").unwrap();
    let mut config = chroma::config::Config::default();
    config.adapters.insert(
        "island".into(),
        chroma::config::Adapter {
            enabled: true,
            desktop: Some(Kind::Island),
            island_readable: true,
            config_path: Some(source.to_str().unwrap().into()),
            output: Some("generated/island.css".into()),
            ..Default::default()
        },
    );
    let wood = presets::get("rustwood").unwrap();
    chroma::engine::apply::apply(&paths, &config, "rustwood", wood.clone(), true).unwrap();
    chroma::engine::apply::apply(&paths, &config, "boats", boats(), true).unwrap();
    chroma::engine::rollback::rollback(&paths, &config, true).unwrap();
    assert_eq!(
        std::fs::read(paths.config.join("generated/island.css")).unwrap(),
        island::render(SOURCE, &wood).unwrap().as_bytes()
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    assert_eq!(std::fs::read_to_string(json).unwrap(), "{\"hover\":true}");
}
