//! One output shape, decided the same way by every relic.
//!
//! A tool an agent invokes should detect that and change shape: no colour, no
//! alignment padding, no box drawing, a stable field order. A tool a person
//! invokes may spend freely on tables and colour. `~/.config/reliquary/AGENTIC-TOOLING.md`
//! states the rule; this is the one implementation of it, so two relics cannot
//! answer the same question differently.

use std::io::IsTerminal;
use std::str::FromStr;

use clap::ValueEnum;

/// The shape output takes.
///
/// [`Format::Json`] is a separate, explicit opt-in rather than a richer
/// [`Format::Agent`]: it is for scripts, and it costs more tokens than a terse
/// line format does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Format {
    /// Tables, colour, alignment. What a person reads.
    Human,
    /// Terse, stable field order, no decoration. What a model reads.
    Agent,
    /// Machine-readable. What a script reads.
    Json,
}

/// Whether to emit colour, before the output shape has its say.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum ColorChoice {
    /// Honour the environment: `NO_COLOR`, `CLICOLOR_FORCE`, `TERM`, tty-ness.
    Auto,
    /// Always.
    Always,
    /// Never.
    Never,
}

/// Everything a format decision is made from, in the order a caller expects to
/// win.
///
/// Gathered from the process by [`Format::from_process`], or built by hand in a
/// test. Ambient authority is injected, never read: a resolver that reaches for
/// the environment itself can only be tested by mutating it, which no two tests
/// can safely do at once.
#[derive(Clone, Copy, Debug, Default)]
pub struct FormatInputs<'a> {
    /// An explicit flag. Beats everything.
    pub explicit: Option<Format>,
    /// The value of the relic's own `<NAME>_UI` variable, if set.
    pub env: Option<&'a str>,
    /// Whether an agent harness spawned this process.
    pub agent_harness: bool,
    /// Whether stdout is a terminal. The backstop, for agentic callers that
    /// announce nothing.
    pub stdout_is_terminal: bool,
}

/// The `<NAME>_UI` value did not name a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownFormat;

impl std::fmt::Display for UnknownFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("expected one of: human, agent, json")
    }
}

impl std::error::Error for UnknownFormat {}

impl FromStr for Format {
    type Err = UnknownFormat;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "human" => Ok(Self::Human),
            "agent" => Ok(Self::Agent),
            "json" => Ok(Self::Json),
            _ => Err(UnknownFormat),
        }
    }
}

impl Format {
    /// Resolve from inputs already gathered.
    ///
    /// An unparseable `<NAME>_UI` falls through to the next signal rather than
    /// failing: the variable is a convenience, and a typo in it must not stop a
    /// tool that a session-start hook is waiting on.
    ///
    /// ```
    /// use relic_core::ui::{Format, FormatInputs};
    ///
    /// let inputs = FormatInputs { agent_harness: true, ..FormatInputs::default() };
    /// assert_eq!(Format::resolve(&inputs), Format::Agent);
    /// ```
    #[must_use]
    pub fn resolve(inputs: &FormatInputs<'_>) -> Self {
        if let Some(format) = inputs.explicit {
            return format;
        }
        if let Some(Ok(format)) = inputs.env.map(str::parse::<Self>) {
            return format;
        }
        if inputs.agent_harness || !inputs.stdout_is_terminal {
            return Self::Agent;
        }
        Self::Human
    }

    /// Gather the inputs from this process and resolve.
    ///
    /// `var` is the relic's own `<NAME>_UI` variable. `CLAUDECODE` is exported
    /// into every subprocess Claude Code spawns, which is how an agent shelling
    /// out is recognised before the terminal check has to guess.
    #[must_use]
    pub fn from_process(explicit: Option<Self>, var: &str) -> Self {
        let env = std::env::var(var).ok();
        Self::resolve(&FormatInputs {
            explicit,
            env: env.as_deref(),
            agent_harness: std::env::var_os("CLAUDECODE").is_some(),
            stdout_is_terminal: std::io::stdout().is_terminal(),
        })
    }
}

impl ColorChoice {
    /// Whether this run emits colour.
    ///
    /// `Auto` is two conditions, and both are policy rather than capability:
    /// only [`Format::Human`] spends on decoration at all, and the environment
    /// has to want it. The environment half is `anstream`'s — the same ladder
    /// clap itself walks, so `NO_COLOR`, `CLICOLOR_FORCE`, `TERM=dumb` and
    /// tty-ness are honoured without any relic re-deriving them.
    #[must_use]
    pub fn use_color(self, format: Format) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Auto => {
                format == Format::Human
                    && anstream::AutoStream::choice(&std::io::stdout())
                        != anstream::ColorChoice::Never
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>() -> FormatInputs<'a> {
        FormatInputs::default()
    }

    #[test]
    fn an_explicit_choice_beats_every_signal() {
        let i = FormatInputs {
            explicit: Some(Format::Json),
            env: Some("human"),
            agent_harness: true,
            stdout_is_terminal: true,
        };
        assert_eq!(Format::resolve(&i), Format::Json);
    }

    #[test]
    fn the_env_var_beats_detection() {
        let i = FormatInputs {
            env: Some("human"),
            agent_harness: true,
            ..inputs()
        };
        assert_eq!(Format::resolve(&i), Format::Human);
    }

    #[test]
    fn an_unparseable_env_var_falls_through_rather_than_failing() {
        let i = FormatInputs {
            env: Some("yaml"),
            stdout_is_terminal: true,
            ..inputs()
        };
        assert_eq!(Format::resolve(&i), Format::Human);
    }

    #[test]
    fn an_agent_harness_is_recognised_before_the_terminal_check() {
        let i = FormatInputs {
            agent_harness: true,
            stdout_is_terminal: true,
            ..inputs()
        };
        assert_eq!(Format::resolve(&i), Format::Agent);
    }

    #[test]
    fn a_pipe_is_agent_shaped() {
        assert_eq!(Format::resolve(&inputs()), Format::Agent);
    }

    #[test]
    fn a_terminal_with_no_other_signal_is_human() {
        let i = FormatInputs {
            stdout_is_terminal: true,
            ..inputs()
        };
        assert_eq!(Format::resolve(&i), Format::Human);
    }

    #[test]
    fn only_human_output_spends_on_colour() {
        assert!(!ColorChoice::Auto.use_color(Format::Agent));
        assert!(!ColorChoice::Auto.use_color(Format::Json));
    }

    #[test]
    fn an_explicit_colour_choice_ignores_the_shape() {
        assert!(ColorChoice::Always.use_color(Format::Json));
        assert!(!ColorChoice::Never.use_color(Format::Human));
    }

    #[test]
    fn format_parses_from_its_own_spelling() {
        assert_eq!("agent".parse::<Format>(), Ok(Format::Agent));
        assert_eq!("Agent".parse::<Format>(), Err(UnknownFormat));
    }
}
