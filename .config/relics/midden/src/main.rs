mod cli;
mod cmd;
mod field;
mod guide;
mod help;
mod id;
mod note;
mod render;
mod store;
mod ui;

use clap::{CommandFactory, Parser};

use cli::{Cli, Command, ListArgs};

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            eprintln!("midden: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<bool> {
    // Completions and help topics describe the tool rather than read the
    // corpus, so they must work before one exists.
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
        _ => {}
    }

    let ctx = cmd::open_context(&cli.global)?;
    match &cli.command {
        None => cmd::list(&ctx, &ListArgs::default())?,
        Some(Command::List(args)) => cmd::list(&ctx, args)?,
        Some(Command::Note(args)) => cmd::note(&ctx, args)?,
        Some(Command::Show(args)) => cmd::show(&ctx, args)?,
        Some(Command::Path(args)) => cmd::path(&ctx, args)?,
        Some(Command::Set(args)) => cmd::set(&ctx, args)?,
        Some(Command::Resolve(args)) => cmd::resolve(&ctx, args)?,
        Some(Command::Archive(args)) => cmd::archive(&ctx, args)?,
        Some(Command::Digest(args)) => cmd::digest(&ctx, args)?,
        Some(Command::Gc(args)) => cmd::gc(&ctx, args)?,
        Some(Command::Doctor) => return cmd::doctor(&ctx),
        Some(Command::Completions(_) | Command::Help(_) | Command::Guide(_)) => {
            unreachable!("handled above")
        }
    }
    Ok(true)
}
