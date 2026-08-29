mod cli;
mod cmd;
mod guide;
mod help;
mod hook;
mod inject;
mod mode;
mod render;
mod resolve;
mod schedule;
mod state;
mod token;
mod transcript;
mod trigger;

use clap::{CommandFactory, Parser};

use cli::{Cli, Command};

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            // Never stdout: on a prompt-submission hook that is the model's
            // context. `hook` never reaches here, and this keeps the invariant
            // true for every other command too.
            eprintln!("mantra: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> anyhow::Result<bool> {
    // These describe the tool rather than read the machine, so they must work
    // before there is a mode corpus or a state directory.
    match &cli.command {
        Some(Command::Completions(args)) => {
            return cmd::completions(args, &mut Cli::command()).map(|()| true);
        }
        Some(Command::Help(args)) => {
            return cmd::help_topic(args, &mut Cli::command()).map(|()| true);
        }
        Some(Command::Guide(args)) => {
            return cmd::guide_topic(args).map(|()| true);
        }
        // The one command that must never fail, and never read a flag: its
        // whole input is a payload on stdin.
        Some(Command::Hook) => return cmd::hook().map(|()| true),
        _ => {}
    }

    let ctx = cmd::open(&cli.global)?;
    match &cli.command {
        None | Some(Command::List) => cmd::list(&ctx)?,
        Some(Command::Explain(args)) => cmd::explain(&ctx, args)?,
        Some(Command::DryRun(args)) => cmd::dry_run(&ctx, args)?,
        Some(Command::Gc(args)) => cmd::gc(&ctx, args)?,
        Some(Command::Doctor) => return cmd::doctor(&ctx),
        Some(Command::Completions(_) | Command::Help(_) | Command::Guide(_) | Command::Hook) => {
            unreachable!("handled above")
        }
    }
    Ok(true)
}
