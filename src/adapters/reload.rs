use crate::{Context, Result, bail, ensure};
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Config is the sole source of argv. Palette data never reaches this function.
pub fn run(argv: &[String], timeout_ms: u64) -> Result<()> {
    if argv.is_empty() {
        return Ok(());
    }
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot execute reload {:?}", argv[0]))?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                ensure!(
                    status.success(),
                    "reload {:?} exited with {status}",
                    argv[0]
                );
                return Ok(());
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                if let Err(error) = result {
                    return Err(error).context("waiting for reload");
                }
                bail!("reload {:?} timed out after {timeout_ms} ms", argv[0]);
            }
        }
    }
}

pub fn available(program: &str) -> bool {
    use std::{env, os::unix::fs::PermissionsExt, path::Path};
    let executable = |path: &Path| {
        path.is_file()
            && path
                .metadata()
                .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    };
    if Path::new(program).is_absolute() {
        return executable(Path::new(program));
    }
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|p| executable(&p.join(program))))
}
