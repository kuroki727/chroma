use chroma::{
    integration::colors::{self, Kind},
    palette::{Palette, colors::Color, presets},
};

const SOURCE: &str = include_str!("fixtures/desktop/waybar/style.css");
const CANDIDATE: &str = include_str!("../docs/t470-waybar/style.css");
const IMPORT: &str = "/home/kuroki/.config/chroma/generated/waybar.css";

#[test]
fn t470_candidate_changes_only_reviewed_colors_and_one_import() {
    let standard = colors::waybar_migration(SOURCE, IMPORT).unwrap();
    let expected = standard.replace(
        "color: @chroma_error;",
        "color: mix(@chroma_error, @chroma_foreground, 0.15);",
    );
    assert_eq!(CANDIDATE, expected);
    assert_eq!(CANDIDATE.matches("@import").count(), 1);
    assert_eq!(colors::edits(Kind::Waybar, SOURCE).unwrap().len(), 9);
    // Transparent backgrounds and every non-color byte are retained by the equality above.
    assert_eq!(CANDIDATE.matches("background: transparent;").count(), 2);
    assert!(!CANDIDATE.contains(":hover"));
}

fn mix(a: Color, b: Color) -> Color {
    Color(
        (f64::from(a.0) * 0.85 + f64::from(b.0) * 0.15).round() as u8,
        (f64::from(a.1) * 0.85 + f64::from(b.1) * 0.15).round() as u8,
        (f64::from(a.2) * 0.85 + f64::from(b.2) * 0.15).round() as u8,
    )
}

#[test]
fn boats_and_rustwood_waybar_text_and_indicator_contrast() {
    let rustwood = presets::get("rustwood").unwrap();
    let boats = Palette::parse(
        r##"name = "Boats Preview test colors"
[colors]
background = "#0E0B09"
surface = "#1C1816"
surface_alt = "#2A2624"
foreground = "#F4F1EF"
muted = "#938E8B"
primary = "#DE9961"
secondary = "#FFBF86"
border = "#75706D"
error = "#C2827C"
warning = "#ECD69A"
success = "#85B286"
"##,
    )
    .unwrap();
    assert!(rustwood.colors.error.contrast(rustwood.colors.background) < 4.5);
    for palette in [boats, rustwood] {
        let c = &palette.colors;
        for color in [c.foreground, c.muted, c.warning, mix(c.error, c.foreground)] {
            assert!(
                color.contrast(c.background) >= 4.5,
                "{}: {color}",
                palette.name
            );
        }
        assert!(c.primary.contrast(c.background) >= 3.0);
        let definitions = colors::render(Kind::Waybar, CANDIDATE, &palette).unwrap();
        for role in [
            "background",
            "foreground",
            "muted",
            "primary",
            "warning",
            "error",
        ] {
            assert!(definitions.contains(&format!("@define-color chroma_{role} ")));
        }
    }
}
