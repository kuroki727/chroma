use clap::Parser;

fn main() -> std::process::ExitCode {
    match chroma::cli::run(chroma::cli::Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("chroma: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
