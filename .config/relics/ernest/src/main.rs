mod cli;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, Format};
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
    let show = cli.view.presentation();
    let format = cli.view.format();

    if let Some(Command::Diff { before, after }) = cli.command {
        let before = report::json::load(&before)?;
        let after = report::json::load(&after)?;
        match format {
            // A diff is not a measurement, so there is no snapshot to write. Say
            // so rather than write a report the caller did not ask for.
            Format::Json => {
                anyhow::bail!("diff has no --format json; compare the snapshots you already hold")
            }
            Format::Value => print!("{}", report::diff::quiet(&before, &after)?),
            Format::Text => print!("{}", report::diff::render(&before, &after, show)?),
        }
        return Ok(0);
    }

    let options = cli.measure;
    for path in &options.paths {
        if !walk::is_readable_root(path) {
            anyhow::bail!("no such path: {}", path.display());
        }
    }

    let unit = options.unit.into();
    let survey = walk::collect(
        &options.paths,
        options.scope.into(),
        options.lang.as_deref(),
    );
    let report = aggregate::run(&survey, unit, show.views);

    match format {
        Format::Json => println!("{}", report::json::render(&report)?),
        Format::Value => print!("{}", report::human::value(&report)),
        Format::Text => print!("{}", report::human::render(&report, show)),
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
