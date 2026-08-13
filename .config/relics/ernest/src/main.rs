mod cli;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};
use ernest::{aggregate, report, walk};

/// Density exceeded `--max-density`. Distinct from 2, which means ernest could
/// not do its job at all.
const EXIT_OVER_THRESHOLD: i32 = 1;
const EXIT_ERROR: i32 = 2;

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("ernest: {err:#}");
            std::process::exit(EXIT_ERROR);
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    if let Some(Command::Diff { before, after }) = cli.command {
        let before = report::json::load(&before)?;
        let after = report::json::load(&after)?;
        print!("{}", report::diff::render(&before, &after)?);
        return Ok(0);
    }

    let options = cli.measure;
    for path in &options.paths {
        if !walk::is_readable_root(path) {
            anyhow::bail!("no such path: {}", path.display());
        }
    }

    let unit = options.unit.into();
    let survey = walk::collect(&options.paths, options.scope.into(), options.lang.as_deref());
    let report = aggregate::run(&survey, unit, options.views());

    if options.json {
        println!("{}", report::json::render(&report)?);
    } else {
        print!("{}", report::human::render(&report));
    }

    if let Some(limit) = options.max_density
        && let Some(density) = report.headline().density
    {
        let measured = density * 100.0;
        if measured > limit {
            eprintln!("ernest: prose density {measured:.1}% exceeds --max-density {limit:.1}%");
            return Ok(EXIT_OVER_THRESHOLD);
        }
    }

    Ok(0)
}
