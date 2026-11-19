//! Bounded source sampling, before clustering. PNG never needs a full frame.
use crate::{Context, Result, ensure};
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Cursor, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const MIB: u64 = 1024 * 1024;
const MAX_INPUT: u64 = 64 * MIB;
// DCI 8K, including UHD 8K, Retina 6K and 5K. This bounds decode work,
// independently of output allocation. Retain support for narrow panoramas.
const MAX_PIXELS: u64 = 8192 * 4320;
const MAX_DIMENSION: u32 = 16384;
const LEGACY_PIXELS: u64 = 16_777_216;
const MAX_DECODED: u64 = 64 * MIB;

fn dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        width > 0
            && height > 0
            && width <= MAX_DIMENSION
            && height <= MAX_DIMENSION
            && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "image dimension/pixel count limit exceeded (maximum DCI 8K area, 16384 per axis)"
    );
    Ok(())
}

// The same actual-pixel, equal-area strata used by the existing extractor.
fn positions(width: u32, height: u32) -> Vec<(u32, u32)> {
    let (columns, rows) = (width.min(128), height.min(128));
    let mut result = Vec::with_capacity((columns * rows) as usize);
    for y in 0..rows {
        for x in 0..columns {
            let left = x * width / columns;
            let top = y * height / rows;
            let px = left + (x * 37 + y * 17 + 11) % ((x + 1) * width / columns - left);
            let py = top + (x * 13 + y * 28 + 7) % ((y + 1) * height / rows - top);
            result.push((px, py));
        }
    }
    result
}

pub(super) fn read(path: &Path) -> Result<Vec<[u8; 4]>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
        .with_context(|| format!("opening image {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_INPUT,
        "image must be a regular file no larger than 64 MiB"
    );
    let mut input = Vec::new();
    input
        .try_reserve_exact(metadata.len() as usize)
        .context("allocating bounded image input")?;
    file.take(MAX_INPUT + 1).read_to_end(&mut input)?;
    ensure!(
        input.len() as u64 <= MAX_INPUT,
        "compressed image exceeds 64 MiB"
    );
    match image::guess_format(&input).context("only PNG and JPEG images are supported")? {
        ImageFormat::Png => png_samples(&input).context("decoding PNG within limits"),
        ImageFormat::Jpeg => jpeg_samples(&input).context("decoding JPEG within limits"),
        _ => crate::bail!("only PNG and JPEG images are supported"),
    }
}

fn png_samples(input: &[u8]) -> Result<Vec<[u8; 4]>> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(input),
        png::Limits {
            bytes: 16 * MIB as usize,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND);
    let header = decoder.read_header_info()?;
    let (width, height, interlaced) = (header.width, header.height, header.interlaced);
    dimensions(width, height)?;
    let mut reader = decoder.read_info()?;
    // Static wallpapers only: do not silently sample a partial APNG subframe.
    ensure!(
        reader.info().animation_control.is_none(),
        "animated PNG wallpapers are unsupported"
    );
    let (color, depth) = reader.output_color_type();
    let channels = color.samples();
    let sample_bytes = match depth {
        png::BitDepth::Eight => 1,
        png::BitDepth::Sixteen => 2,
        _ => crate::bail!("unsupported expanded PNG bit depth"),
    };
    let positions = positions(width, height);
    let mut sampled = vec![[0; 4]; positions.len()];
    let mut rows = BTreeMap::<u32, Vec<(usize, u32)>>::new();
    for (i, &(x, y)) in positions.iter().enumerate() {
        rows.entry(y).or_default().push((i, x));
    }
    let plain = [(0, 0, 1, 1)];
    let adam7 = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let mut count = 0;
    for &(x0, y0, dx, dy) in if interlaced { &adam7[..] } else { &plain[..] } {
        if x0 >= width {
            continue;
        }
        for y in (y0..height).step_by(dy as usize) {
            let row = reader.next_row()?.context("truncated PNG row data")?;
            let data = row.data();
            ensure!(
                data.len() == (width - x0).div_ceil(dx) as usize * channels * sample_bytes,
                "invalid PNG scanline length"
            );
            if let Some(points) = rows.get(&y) {
                for &(i, x) in points {
                    if x < x0 || (x - x0) % dx != 0 {
                        continue;
                    }
                    let offset = ((x - x0) / dx) as usize * channels * sample_bytes;
                    let mut rgba = [0, 0, 0, 255];
                    let mut values = [0; 4];
                    for (c, value) in values.iter_mut().enumerate().take(channels) {
                        let at = offset + c * sample_bytes;
                        *value = if sample_bytes == 1 {
                            data[at]
                        } else {
                            // Match image's u16 -> u8 conversion, including rounding.
                            ((u32::from(u16::from_be_bytes([data[at], data[at + 1]])) + 128) / 257)
                                as u8
                        };
                    }
                    match color {
                        png::ColorType::Grayscale => rgba[..3].fill(values[0]),
                        png::ColorType::GrayscaleAlpha => {
                            rgba[..3].fill(values[0]);
                            rgba[3] = values[1];
                        }
                        png::ColorType::Rgb => rgba[..3].copy_from_slice(&values[..3]),
                        png::ColorType::Rgba => rgba = values,
                        _ => crate::bail!("unsupported expanded PNG color type"),
                    }
                    sampled[i] = rgba;
                    count += 1;
                }
            }
        }
    }
    ensure!(
        reader.next_row()?.is_none() && count == sampled.len(),
        "invalid PNG row count"
    );
    reader.finish()?; // Validate trailing IDAT, CRCs and IEND; no early-success on selected rows.
    Ok(sampled)
}

fn jpeg_samples(input: &[u8]) -> Result<Vec<[u8; 4]>> {
    let mut reader = ImageReader::with_format(Cursor::new(input), ImageFormat::Jpeg);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED);
    reader.limits(limits);
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    dimensions(width, height)?;
    // Keep the established codec and exact sample values for ordinary JPEGs.
    let decoded = if u64::from(width) * u64::from(height) <= LEGACY_PIXELS
        && decoder.total_bytes() <= MAX_DECODED
    {
        DynamicImage::from_decoder(decoder)?
    } else {
        drop(decoder);
        let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(input));
        decoder.set_max_decoding_buffer_size(MAX_DECODED as usize);
        decoder.read_info()?;
        let info = decoder.info().context("missing JPEG frame header")?;
        ensure!(
            info.coding_process != jpeg_decoder::CodingProcess::Lossless,
            "high-resolution lossless JPEG is unsupported"
        );
        ensure!(
            info.pixel_format == jpeg_decoder::PixelFormat::RGB24
                || info.pixel_format == jpeg_decoder::PixelFormat::L8,
            "high-resolution JPEG requires 8-bit RGB or grayscale"
        );
        // Progressive JPEG retains full-resolution DCT coefficients even at 1/8.
        // Bound padded coefficient storage AND worst-case queued row copies.
        // 3 channels * 4 bytes per padded pixel stays below 416 MiB for DCI 8K.
        let channels = info.pixel_format.pixel_bytes() as u64;
        let padded = u64::from(width.div_ceil(32) * 32) * u64::from(height.div_ceil(32) * 32);
        ensure!(
            padded * channels * 4 <= 416 * MIB,
            "JPEG coefficient memory limit exceeded"
        );
        let (w, h) = decoder.scale(1, 1)?; // Smallest supported DCT: 1/8, no full RGB frame.
        ensure!(
            u64::from(w) * u64::from(h) * channels <= MAX_DECODED,
            "JPEG reduced output limit exceeded"
        );
        let pixels = decoder.decode()?;
        if channels == 1 {
            DynamicImage::ImageLuma8(
                image::GrayImage::from_raw(u32::from(w), u32::from(h), pixels)
                    .context("invalid JPEG output size")?,
            )
        } else {
            DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(u32::from(w), u32::from(h), pixels)
                    .context("invalid JPEG output size")?,
            )
        }
    };
    Ok(positions(decoded.width(), decoded.height())
        .into_iter()
        .map(|(x, y)| decoded.get_pixel(x, y).0)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interlaced_png_matches_full_decode_exactly() {
        let input = include_bytes!("../../tests/fixtures/interlaced-rgba.png");
        let decoded = image::load_from_memory(input).unwrap();
        let expected: Vec<_> = positions(decoded.width(), decoded.height())
            .into_iter()
            .map(|(x, y)| decoded.get_pixel(x, y).0)
            .collect();
        assert_eq!(png_samples(input).unwrap(), expected);
    }

    #[test]
    fn png_samples_match_legacy_pixels_including_sixteen_bit_and_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");
        let image = image::ImageBuffer::from_fn(321, 215, |x, y| {
            image::Rgba([
                (x * 193) as u16,
                (y * 211) as u16,
                ((x + y) * 119) as u16,
                (x * 203) as u16,
            ])
        });
        image.save(&path).unwrap();
        let decoded = image::open(&path).unwrap();
        let expected: Vec<_> = positions(321, 215)
            .into_iter()
            .map(|(x, y)| decoded.get_pixel(x, y).0)
            .collect();
        assert_eq!(read(&path).unwrap(), expected);
    }
}
