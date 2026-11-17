//! Offline candidate/validation helper; never writes desktop files or contacts Niri IPC.
use chroma::{
    Context, Result, ensure,
    integration::{colors::Kind, niri, validate},
    safe_fs,
};
use std::path::{Path, PathBuf};

fn read(path: &Path) -> Result<Vec<u8>> {
    let (dir, name) = safe_fs::Dir::parent(path)?;
    dir.check_target(&name)?;
    Ok(dir.read(&name)?.context("missing input")?.bytes)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2 || args.len() == 3,
        "usage: niri_candidate compose|check SOURCE FRAGMENT; or check-source SOURCE"
    );
    let mode = args[0].to_str().context("non-UTF8 mode")?;
    let source = read(&PathBuf::from(&args[1]))?;
    if mode == "check-source" {
        ensure!(args.len() == 2, "check-source takes only SOURCE");
        // An empty isolated fragment leaves the complete original configuration unchanged.
        validate::generated(Kind::Niri, &source, b"")?;
        println!("PASS native Niri original-source validation");
    } else {
        ensure!(
            args.len() == 3 && matches!(mode, "compose" | "check"),
            "invalid operation"
        );
        let fragment = PathBuf::from(&args[2]);
        let candidate = niri::candidate(std::str::from_utf8(&source)?, &fragment)?;
        validate::generated(Kind::Niri, &source, &read(&fragment)?)?;
        if mode == "compose" {
            print!("{candidate}");
        } else {
            println!("PASS native Niri isolated complete composition");
        }
    }
    Ok(())
}
