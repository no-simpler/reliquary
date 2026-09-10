//! Spaced-repetition drill for the passwords you must hold in your head.

mod cli;
mod cmd;
mod config;
mod doctor;
mod drill;
mod exit;
mod guide;
mod help;
mod ladder;
mod log;
mod model;
mod render;
mod secret;
mod slug;
mod stats;
mod store;
mod tui;
mod verifier;

use clap::Parser as _;

use crate::exit::REFUSED;

fn main() -> std::process::ExitCode {
    let cli = match cli::Cli::try_parse() {
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
