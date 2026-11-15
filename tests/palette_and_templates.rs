use chroma::{
    adapters::{self, template},
    palette::{self, Palette, colors::Color},
};

#[test]
fn presets_roundtrip_and_render_every_builtin() {
    let all = palette::presets::all().unwrap();
    assert_eq!(all.len(), 6);
    for p in all.values() {
        assert_eq!(
            Palette::parse(&toml::to_string_pretty(p).unwrap()).unwrap(),
            *p
        );
        assert_eq!(p.variables().len(), 27);
        for (_, _, source) in adapters::BUILTINS {
            let alpha = adapters::FUZ_ALPHA
                .iter()
                .map(|name| (name.to_string(), 255))
                .collect();
            let output = template::render_with_alpha(source, p, &alpha).unwrap();
            assert!(!output.contains("{{"));
            assert!(!output.contains("}}"));
        }
    }
    let nord = &all["nord"];
    assert_eq!(nord.colors.background, Color(46, 52, 64));
    assert_eq!(
        all["tokyo-night"].variables()["ansi9"],
        Color(255, 137, 157)
    );
}

#[test]
fn color_validation_rejects_code_alpha_unicode_and_short_hex() {
    for text in [
        "red",
        "#abc",
        "#11223344",
        "112233",
        "#GG0000",
        "#🦀ab",
        "#00000\n",
        "{{ r }}",
        "#é0000",
    ] {
        assert!(text.parse::<Color>().is_err(), "{text}");
    }
    assert_eq!("#aAbBcC".parse::<Color>().unwrap().to_string(), "#AABBCC");
}

#[test]
fn missing_fields_unknown_fields_invalid_ansi_and_contrast_fail() {
    let source = palette::presets::SOURCES[0].1;
    for invalid in [
        source.replace("background = \"#17130F\"", ""),
        source.replace("#E8DCC8", "#17130F"),
        source.replace("name = \"Rustwood\"", "name = \"\""),
        format!("reload = [\"evil\"]\n{source}"),
        format!("{source}\n[ansi]\n\"16\" = \"#FFFFFF\""),
        format!("{source}\n[ansi]\n\"00\" = \"#FFFFFF\""),
    ] {
        assert!(Palette::parse(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn partial_ansi_overrides_keep_deterministic_fallbacks() {
    let source = format!(
        "{}\n[ansi]\n\"4\" = \"#123456\"",
        palette::presets::SOURCES[0].1
    );
    let p = Palette::parse(&source).unwrap();
    assert_eq!(p.variables()["ansi4"], Color(18, 52, 86));
    assert_eq!(p.variables()["ansi1"], p.colors.error);
    assert_ne!(p.variables()["ansi8"], p.variables()["ansi0"]);
}

#[test]
fn template_formats_and_literal_braces_are_safe() {
    let p = palette::presets::get("rustwood").unwrap();
    let source = "{{ background }} {{background.hex_raw}} {{background.rgb}} {{ background.rgb_raw }} {{ background.r }} {{background.g}} {{background.b}} {{{{escaped}}}}";
    assert_eq!(
        template::render(source, &p).unwrap(),
        "#17130F 17130F rgb(23, 19, 15) 23, 19, 15 23 19 15 {{escaped}}"
    );
    assert_eq!(template::render("你好 { x }", &p).unwrap(), "你好 { x }");
}

#[test]
fn unknown_variables_formats_and_unbalanced_braces_are_errors() {
    let p = palette::presets::get("rustwood").unwrap();
    for source in [
        "{{ typo }}",
        "{{ primary.rgba }}",
        "{{ primary.r.x }}",
        "{{ primary",
        "x }}",
        "{{ }}",
        "{{ include evil }}",
        "{{ansi16}}",
    ] {
        assert!(template::render(source, &p).is_err(), "{source}");
    }
    let error = template::render("line1\n{{ not_a_color }}", &p)
        .unwrap_err()
        .to_string();
    assert!(error.contains("line 2") && error.contains("not_a_color"));
}

#[test]
fn contrast_and_oklab_have_known_reference_values() {
    let white = Color(255, 255, 255);
    let black = Color(0, 0, 0);
    assert!((white.contrast(black) - 21.0).abs() < 1e-6);
    assert!((white.oklab()[0] - 1.0).abs() < 1e-6);
    assert!((Color(255, 0, 0).oklab()[0] - 0.627955).abs() < 1e-5);
    for c in [
        white,
        black,
        Color(255, 0, 0),
        Color(0, 255, 0),
        Color(0, 0, 255),
        Color(41, 82, 123),
    ] {
        let [l, a, b] = c.oklab();
        assert!(Color::lch(l, a.hypot(b), c.hue()).distance(c) < 0.03);
    }
}

#[test]
fn fuzzel_requires_explicit_local_alpha_values_and_preserves_them() {
    let root = tempfile::tempdir().unwrap();
    let paths = chroma::config::Paths::discover(Some(root.path().join("cfg")), None).unwrap();
    let mut adapter = chroma::config::Adapter {
        enabled: true,
        ..Default::default()
    };
    let palette = palette::presets::get("rustwood").unwrap();
    assert!(adapters::render("fuzzel", &adapter, &paths, &palette).is_err());
    for key in adapters::FUZ_ALPHA {
        adapter.alpha.insert(key.to_string(), 255);
    }
    adapter.alpha.insert("background".into(), 221);
    for id in ["rustwood", "paper"] {
        let palette = palette::presets::get(id).unwrap();
        let output = adapters::render("fuzzel", &adapter, &paths, &palette).unwrap();
        let text = String::from_utf8(output.bytes).unwrap();
        assert!(
            text.lines()
                .find(|l| l.starts_with("background="))
                .unwrap()
                .ends_with("DD")
        );
    }
}

#[test]
fn many_template_variables_keep_precise_line_numbers() {
    let p = palette::presets::get("rustwood").unwrap();
    let source = "{{ background }}\n".repeat(8192);
    assert_eq!(template::render(&source, &p).unwrap().lines().count(), 8192);
    let error = template::render(&(source + "{{ missing }}"), &p).unwrap_err();
    assert!(error.to_string().contains("line 8193"));
}
