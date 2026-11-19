//! Render an opt-in Island candidate offline. No writes, desktop launch or IPC.
use chroma::{Context, Result, ensure, integration::island, palette::Palette, safe_fs};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "usage: island_candidate SOURCE_CSS PALETTE_TOML"
    );
    let read = |path: PathBuf| -> Result<Vec<u8>> {
        let (dir, name) = safe_fs::Dir::parent(&path)?;
        dir.check_target(&name)?;
        Ok(dir
            .read(&name)?
            .context("missing input stylesheet or palette")?
            .bytes)
    };
    let source = read(PathBuf::from(&args[0]))?;
    let palette = Palette::parse(std::str::from_utf8(&read(PathBuf::from(&args[1]))?)?)?;
    print!(
        "{}",
        island::render(std::str::from_utf8(&source)?, &palette)?
    );
    Ok(())
}
