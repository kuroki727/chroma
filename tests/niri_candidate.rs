use chroma::{
    integration::{
        colors::{self, Kind},
        niri, validate,
    },
    palette::presets,
};
use std::{fs, path::Path};

const SOURCE: &str = r##"// original colors deliberately remain before the managed include
layout {
 gaps 10
 border { width 2; active-color "#6f7f4f"; inactive-color "#2a2e22"; urgent-color "#b5683c"; }
 background-color "#12140f"
 focus-ring { off; }
}
window-rule {
 match app-id=r#"firefox$"# title="^Picture-in-Picture$"
 open-floating true
}
// BEGIN NiriOpacity
window-rule { opacity 1.00; }
// END NiriOpacity
binds { Mod+T { spawn "kitty"; }; }
"##;

#[test]
fn one_final_include_keeps_every_original_byte_and_repeated_connection_is_refused() {
    let path = Path::new("/home/kuroki/.config/chroma/generated/niri.kdl");
    let result = niri::candidate(SOURCE, path).unwrap();
    assert_eq!(
        result,
        format!("{SOURCE}\ninclude \"{}\"\n", path.display())
    );
    assert!(result.find("active-color \"#6f7f4f\"").unwrap() < result.rfind("include ").unwrap());
    assert!(result.contains("match app-id=r#\"firefox$\"# title=\"^Picture-in-Picture$\""));
    assert!(result.contains("window-rule { opacity 1.00; }"));
    assert!(niri::candidate(&result, path).is_err());
    // A trailing line comment without newline must not comment out the include.
    assert!(
        niri::candidate("// trailing comment", path)
            .unwrap()
            .contains("comment\ninclude")
    );
}

#[test]
fn unsafe_paths_and_existing_include_chains_fail_closed() {
    for path in [
        "relative.kdl",
        "/tmp/../other.kdl",
        "/tmp/quoted\".kdl",
        "/tmp/back\\slash",
        "/tmp/new\nline",
    ] {
        assert!(niri::candidate(SOURCE, Path::new(path)).is_err(), "{path}");
    }
    for source in [
        "include \"elsewhere\"",
        "r#\"include\"# \"elsewhere\"",
        "\"include\" \"elsewhere\"",
    ] {
        assert!(niri::candidate(source, Path::new("/tmp/colors.kdl")).is_err());
    }
}

#[test]
fn native_validator_accepts_complete_raw_string_compositions_and_rejects_invalid_fragment() {
    if !chroma::adapters::reload::available("niri") {
        return;
    }
    let rustwood = presets::get("rustwood").unwrap();
    let generated = colors::render(Kind::Niri, SOURCE, &rustwood).unwrap();
    validate::generated(Kind::Niri, SOURCE.as_bytes(), generated.as_bytes()).unwrap();
    for value in [
        rustwood.colors.primary,
        rustwood.colors.border,
        rustwood.colors.warning,
        rustwood.colors.background,
    ] {
        assert!(generated.contains(&value.to_string()));
    }
    assert!(
        validate::generated(
            Kind::Niri,
            SOURCE.as_bytes(),
            b"layout { border { width not-a-number; } }"
        )
        .is_err()
    );
    // Validate an actual final-include fixture against the native parser, without IPC.
    let temp = tempfile::tempdir().unwrap();
    let fragment = temp.path().join("colors.kdl");
    fs::write(&fragment, generated).unwrap();
    let candidate = temp.path().join("candidate.kdl");
    fs::write(&candidate, niri::candidate(SOURCE, &fragment).unwrap()).unwrap();
    assert!(
        std::process::Command::new("niri")
            .args(["validate", "-c"])
            .arg(candidate)
            .status()
            .unwrap()
            .success()
    );
}
