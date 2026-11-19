use chroma::palette::generator::{Mode, extract, generate};
use std::{fs::File, io::Write, path::Path};

fn png(path: &Path, width: u32, height: u32, depth: png::BitDepth) {
    let mut encoder = png::Encoder::new(File::create(path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header().unwrap();
    let mut stream = writer.stream_writer().unwrap();
    let mut row = Vec::new();
    for x in 0..width {
        let rgba = if x < width * 3 / 4 {
            [40, 120, 65, 255]
        } else {
            [190, 75, 30, 255]
        };
        for value in rgba {
            row.push(value);
            if depth == png::BitDepth::Sixteen {
                row.push(value);
            }
        }
    }
    for _ in 0..height {
        stream.write_all(&row).unwrap();
    }
    stream.finish().unwrap();
    writer.finish().unwrap();
}

#[test]
fn four_five_six_and_eight_k_png_including_sixteen_bit_rgba() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wall.png");
    for (w, h, depth) in [
        (3840, 2160, png::BitDepth::Eight),
        (5120, 2880, png::BitDepth::Eight),
        (6016, 3384, png::BitDepth::Sixteen),
        (7680, 4320, png::BitDepth::Eight),
        (8192, 4320, png::BitDepth::Sixteen),
    ] {
        png(&path, w, h, depth);
        let colors = extract(&path).unwrap();
        assert_eq!(colors.len(), 2);
        assert!((colors[0].population - 0.75).abs() < 0.01);
        assert!(
            colors[0]
                .color
                .distance(chroma::palette::colors::Color(40, 120, 65))
                < 0.002
        );
        generate(&path, Mode::Dark, "8K")
            .unwrap()
            .validate()
            .unwrap();
    }
}

#[test]
fn six_and_eight_k_jpeg_use_reduced_dct_and_keep_source_hues() {
    let dir = tempfile::tempdir().unwrap();
    for (w, h) in [(6016, 3384), (8192, 4320)] {
        let path = dir.path().join("wall.jpg");
        let image = image::RgbImage::from_fn(w, h, |x, _| {
            image::Rgb(if x < w * 3 / 4 {
                [40, 120, 65]
            } else {
                [190, 75, 30]
            })
        });
        image.save(&path).unwrap();
        drop(image);
        let colors = extract(&path).unwrap();
        assert!((colors[0].population - 0.75).abs() < 0.02);
        assert!(
            colors[0]
                .color
                .distance(chroma::palette::colors::Color(40, 120, 65))
                < 0.02
        );
    }
}

#[test]
fn progressive_eight_k_and_malicious_jpeg_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("progressive.jpg");
    let original = include_bytes!("fixtures/8k-progressive.jpg");
    std::fs::write(&path, original).unwrap();
    let colors = extract(&path).unwrap();
    assert!((colors[0].population - 0.75).abs() < 0.02);
    assert!(
        colors[0]
            .color
            .distance(chroma::palette::colors::Color(40, 120, 65))
            < 0.02
    );
    let marker = original.windows(2).position(|p| p == [0xff, 0xc2]).unwrap();
    for (w, h) in [(8192u16, 8192u16), (0, 4320), (16385, 1), (65535, 65535)] {
        let mut bytes = original.to_vec();
        bytes[marker + 5..marker + 7].copy_from_slice(&h.to_be_bytes());
        bytes[marker + 7..marker + 9].copy_from_slice(&w.to_be_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(extract(&path).is_err());
    }
    std::fs::write(&path, &original[..original.len() / 2]).unwrap();
    assert!(extract(&path).is_err());
}

#[test]
fn malformed_rows_trailer_crc_unsupported_formats_and_truncated_jpeg_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input");
    png(&path, 256, 160, png::BitDepth::Eight);
    let valid = std::fs::read(&path).unwrap();
    let mut bad_crc = valid.clone();
    let last = bad_crc.len() - 1;
    bad_crc[last] ^= 1;
    for bytes in [
        valid[..valid.len() / 2].to_vec(),
        valid[..valid.len() - 12].to_vec(),
        bad_crc,
        b"GIF89a".to_vec(),
        b"not an image".to_vec(),
        vec![0xff, 0xd8, 0xff, 0xc0, 0, 17],
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(generate(&path, Mode::Dark, "bad").is_err());
    }
}
