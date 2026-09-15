//! Spaced-repetition drill for the passwords you must hold in your head.

use clap::Parser as _;

use rote::cli::Cli;
use rote::cmd;
use rote::exit::REFUSED;

fn main() -> std::process::ExitCode {
    // A core dump is a copy of every buffer this process ever zeroized. Off
    // before anything else runs; best effort, because a platform that refuses
    // the limit is still one worth drilling on.
    let _ = rlimit::setrlimit(rlimit::Resource::CORE, 0, 0);
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return std::process::ExitCode::from(
                u8::try_from(error.exit_code()).unwrap_or(REFUSED),
            );
        }
    };
    match cmd::run(&cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            // Never stdout: a caller reading --format json must not find prose
            // in the document.
            eprintln!("rote: {error:#}");
            std::process::ExitCode::from(REFUSED)
        }
    }
}
