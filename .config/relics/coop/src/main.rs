mod ask;
mod cache;
mod card;
mod cli;
mod cmd;
mod config;
mod doctor;
mod guide;
mod help;
mod notice;
mod paths;
mod predicate;
mod render;
mod session;
mod source;
mod span;
mod state;

use clap::{CommandFactory, Parser};

use cli::{Cli, Command};

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            // Never stdout: a caller reading --format json must not find prose
            // in the document.
            eprintln!("coop: {error:#}");
            std::process::ExitCode::from(3)
        }
    }
}

fn run(cli: &Cli) -> anyhow::Result<u8> {
    // Completions and help topics describe the tool rather than read the
    // declarations, so they must work before any exist.
    match &cli.command {
        Some(Command::Completions(args)) => {
            return cmd::completions(args, &mut Cli::command()).map(|()| 0);
        }
        Some(Command::Help(args)) => {
            return cmd::help_topic(args, &mut Cli::command()).map(|()| 0);
        }
        Some(Command::Guide(args)) => {
            return cmd::guide_topic(args).map(|()| 0);
        }
        _ => {}
    }

    let ctx = cmd::open_context(&cli.global)?;
    match &cli.command {
        None => cmd::show_card(&ctx)?,
        // The one command that must never fail a shell: a broken declaration,
        // an unreadable cache and a producer that will not run all resolve to
        // an empty prompt rather than a diagnostic in the middle of one.
        Some(Command::Tick) => {
            let _ = cmd::tick(&ctx);
        }
        Some(Command::Prompt) => cmd::prompt(&ctx)?,
        Some(Command::List) => cmd::list(&ctx)?,
        Some(Command::Show(args)) => return Ok(u8::from(!cmd::show(&ctx, args)?)),
        Some(Command::Sources) => cmd::sources(&ctx)?,
        Some(Command::Refresh(args)) => return Ok(u8::from(!cmd::refresh(&ctx, args)?)),
        Some(Command::Doctor) => return cmd::run_doctor(&ctx),
        Some(Command::Completions(_) | Command::Help(_) | Command::Guide(_)) => {
            unreachable!("handled above")
        }
    }
    Ok(0)
}
