use std::io::IsTerminal;

use clap::ValueEnum;

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Format {
    Human,
    Agent,
    Json,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

/// Explicit choice first, then the environment. `CLAUDECODE` is set in every
/// subprocess Claude Code spawns, so an agent shelling out is recognised before
/// the terminal check has to guess.
pub fn resolve_format(explicit: Option<Format>) -> Format {
    if let Some(format) = explicit {
        return format;
    }
    match std::env::var("MIDDEN_UI").as_deref() {
        Ok("human") => return Format::Human,
        Ok("agent") => return Format::Agent,
        Ok("json") => return Format::Json,
        _ => {}
    }
    if std::env::var_os("CLAUDECODE").is_some() {
        return Format::Agent;
    }
    if !std::io::stdout().is_terminal() {
        return Format::Agent;
    }
    Format::Human
}

pub fn use_color(choice: ColorChoice, format: Format) -> bool {
    match choice {
        ColorChoice::Never => false,
        ColorChoice::Always => true,
        ColorChoice::Auto => {
            format == Format::Human
                && supports_color::on_cached(supports_color::Stream::Stdout).is_some()
        }
    }
}

/// Whole days, because retention is counted in days and a note's age only has
/// to say whether it is still current.
pub fn age(at: jiff::Timestamp) -> String {
    let days = age_days(at);
    match days {
        0 => "today".to_owned(),
        d if d < 7 => format!("{d}d"),
        d if d < 90 => format!("{}w", d / 7),
        d => format!("{}mo", d / 30),
    }
}

pub fn age_days(at: jiff::Timestamp) -> i64 {
    (jiff::Timestamp::now() - at).get_seconds().max(0) / 86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_format_wins() {
        assert_eq!(resolve_format(Some(Format::Json)), Format::Json);
    }

    #[test]
    fn ages_read_in_the_right_unit() {
        let now = jiff::Timestamp::now();
        assert_eq!(age(now), "today");
        assert_eq!(age(now - jiff::SignedDuration::from_hours(24 * 3)), "3d");
        assert_eq!(age(now - jiff::SignedDuration::from_hours(24 * 14)), "2w");
        assert_eq!(age(now - jiff::SignedDuration::from_hours(24 * 120)), "4mo");
    }
}
