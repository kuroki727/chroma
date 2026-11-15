use clap::Parser;

fn main() -> std::process::ExitCode {
    match chroma::cli::run(chroma::cli::Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // TOML diagnostics contain source excerpts from untrusted palettes.
            // Keep multiline context while preventing terminal escape injection.
            let mut message = String::new();
            for ch in format!("{error:#}").chars() {
                if ch.is_control() && !matches!(ch, '\n' | '\t') {
                    message.extend(ch.escape_default());
                } else {
                    message.push(ch);
                }
            }
            eprintln!("chroma: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}
