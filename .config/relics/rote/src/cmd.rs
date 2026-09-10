//! What each command does.

use anyhow::{Context as _, Result, bail};
use jiff::civil::Date;
use relic_core::finding::{FixHint, Grade, Report, StationId, Summary};
use relic_core::ui::Format;

use crate::cli::{
    AddArgs, Cli, Command, Global, LogArgs, MeasurementArgs, PracticeArgs, ProbeArgs, RekeyArgs,
    RetireArgs, ScheduleArgs,
};
use crate::config::Config;
use crate::drill::{self, Mode, screen};
use crate::ladder::{self, Class, Gate, Standing};
use crate::log::{
    Added, Attempted, Digest, Event, Line, Outcome, Probed, Record, Rekeyed, Retired, SCHEMA,
    SessionId,
};
use crate::model::{SlugState, State};
use crate::render::{Table, agent, human, json};
use crate::secret::Secret;
use crate::slug::Slug;
use crate::stats::{Stats, Trend};
use crate::store::{Cache, Clock, Env, Journal, Paths, Store, Verifiers};
use crate::tui::{self, Screen as _, card, term};
use crate::verifier::Verifier;

/// Everything went as it should, or there was nothing to do.
pub const CLEAN: u8 = 0;
/// A first-attempt failure, or a soft finding.
pub const LAPSE: u8 = 1;
/// The sitting was abandoned, or something is broken.
pub const INCOMPLETE: u8 = 2;

/// What every command is handed.
pub struct Context {
    /// The environment, read once.
    pub env: Env,
    /// The config file, or its defaults.
    pub config: Config,
    /// The two homes.
    pub paths: Paths,
    /// The clock and the drill day.
    pub clock: Clock,
    /// Output shape.
    pub format: Format,
    /// Whether to color.
    pub color: bool,
    /// Whether to print only what was asked for.
    pub quiet: bool,
}

impl Context {
    /// The drill day.
    pub fn today(&self) -> Date {
        self.clock.today()
    }
}

/// Assemble the context. The only place the process environment is read.
///
/// # Errors
///
/// When the config will not parse, or a default path cannot be resolved.
pub fn open_context(global: &Global) -> Result<Context> {
    let env = Env::from_process();
    let config = Config::read(&env.config_path()?)?;
    let paths = Paths::resolve(&env, &config)?;
    let format = if global.json {
        Format::Json
    } else {
        Format::from_process(global.format, "ROTE_UI")
    };
    let clock = Clock::new(
        jiff::Timestamp::now(),
        jiff::tz::TimeZone::system(),
        config.rollover_hour(),
    );
    Ok(Context {
        env,
        config,
        paths,
        clock,
        format,
        color: global.color.use_color(format),
        quiet: global.quiet,
    })
}

/// Dispatch.
///
/// # Errors
///
/// When a command cannot do what it was asked.
pub fn run(cli: &Cli) -> Result<u8> {
    // These three describe the tool rather than read the drill, so they answer
    // before there is a store to open.
    match &cli.command {
        Some(Command::Completions(args)) => {
            completions(args.shell);
            return Ok(CLEAN);
        }
        Some(Command::Help(args)) => {
            help_topic(args.topic.as_deref())?;
            return Ok(CLEAN);
        }
        Some(Command::Guide(args)) => {
            println!("{}", crate::guide::render(&args.topics)?);
            return Ok(CLEAN);
        }
        None
        | Some(
            Command::Status(_)
            | Command::Stats(_)
            | Command::Log(_)
            | Command::Practice(_)
            | Command::Add(_)
            | Command::Probe(_)
            | Command::Rekey(_)
            | Command::Retire(_)
            | Command::Doctor
            | Command::Banner,
        ) => {}
    }

    let ctx = open_context(&cli.global)?;
    match &cli.command {
        None => sitting(&ctx, Mode::Daily(ctx.config.filler()), &[], cli.aided),
        Some(Command::Practice(args)) => practice(&ctx, args),
        Some(Command::Status(args)) => status(&ctx, args),
        Some(Command::Stats(args)) => stats(&ctx, args),
        Some(Command::Log(args)) => log(&ctx, args),
        Some(Command::Add(args)) => add(&ctx, args),
        Some(Command::Rekey(args)) => rekey(&ctx, args),
        Some(Command::Retire(args)) => retire(&ctx, args),
        Some(Command::Probe(args)) => probe(&ctx, args),
        Some(Command::Doctor) => run_doctor(&ctx),
        Some(Command::Banner) => banner(&ctx),
        Some(Command::Completions(_) | Command::Help(_) | Command::Guide(_)) => {
            unreachable!("handled above")
        }
    }
}

fn completions(shell: clap_complete::Shell) {
    use clap::CommandFactory as _;
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "rote", &mut std::io::stdout());
}

fn help_topic(topic: Option<&str>) -> Result<()> {
    use clap::CommandFactory as _;
    let mut command = Cli::command();
    let Some(name) = topic else {
        println!("{}", command.render_long_help());
        return Ok(());
    };
    if name == "topics" {
        println!("{}", crate::help::topic_names());
        return Ok(());
    }
    if let Some(body) = crate::help::topic(name) {
        println!("{body}");
        return Ok(());
    }
    if let Some(sub) = command.find_subcommand_mut(name) {
        println!("{}", sub.render_long_help());
        return Ok(());
    }
    bail!(
        "no topic or command called {name:?}. Topics: {}",
        crate::help::topic_names()
    )
}

// The sitting.

fn practice(ctx: &Context, args: &PracticeArgs) -> Result<u8> {
    sitting(ctx, Mode::Practice, &args.slugs, false)
}

fn sitting(ctx: &Context, mode: Mode, only: &[Slug], aided: bool) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    for slug in only {
        if state.get(slug).is_none() {
            bail!("there is no slug called {slug}");
        }
    }

    let mut plan = drill::plan(&state, &verifiers, today, mode, only);
    if aided {
        // Declared up front, because the lookup already happened. Every entry in
        // the sitting is a transcription and none of it is a reading.
        for item in &mut plan.items {
            item.class = Class::Aided;
        }
    }
    if plan.is_empty() {
        write_cache(ctx, &state, today)?;
        if !ctx.quiet {
            println!("{}", nothing_due(&state, today));
        }
        return Ok(CLEAN);
    }

    // Nothing here can be asked, so say so inline rather than opening a screen
    // that would only report it and vanish.
    if plan.missing().count() == plan.items.len() {
        let names: Vec<String> = plan.missing().map(|item| item.slug.to_string()).collect();
        println!(
            "no verifier on this machine for {} · rote rekey --force to re-enroll",
            names.join(", ")
        );
        return Ok(INCOMPLETE);
    }

    let session = new_session()?;
    let mut console = term::Terminal::enter(ctx.color)?;
    let taken = drill::run(
        &plan,
        ctx.config.max_attempts(),
        &mut console,
        &|item, secret| match &item.verifier {
            Some(verifier) => Ok(verifier.accepts(secret)?),
            None => Ok(false),
        },
        today,
    )?;

    for entry in &taken.taken {
        let Some(item) = plan.items.get(entry.item) else {
            continue;
        };
        store.append(&record(
            ctx,
            today,
            Event::Attempt(Attempted {
                slug: item.slug.clone(),
                session: session.clone(),
                version: item.version,
                class: entry.class,
                attempt: entry.attempt,
                outcome: entry.outcome,
                ttfk_ms: entry.ttfk_ms,
                total_ms: entry.total_ms,
                corrections: entry.corrections,
                paste_refused: entry.paste_refused,
                scheduled_interval_days: item.scheduled_interval_days,
                actual_interval_days: item.actual_interval_days,
                effective_interval_days: item.effective_interval_days,
                stretch: item.stretch(),
                step_before: item.step.get(),
                step_after: entry.step_after.get(),
            }),
        ))?;
    }

    let after = State::replay(store.journal().lines());
    let notes = closing_notes(&plan, &after);
    let closing = screen::card(
        &screen::Frame::done(today, &taken.rows, &taken, &notes),
        ctx.color,
    );
    console.paint(&closing)?;
    console.hold()?;
    drop(console);

    write_cache(ctx, &after, today)?;
    Ok(if taken.aborted {
        INCOMPLETE
    } else if taken.missed() > 0 {
        LAPSE
    } else {
        CLEAN
    })
}

/// What belongs under the closing card: a reading of the log *after* the
/// sitting, which the sitting itself cannot know.
fn closing_notes(plan: &drill::Plan, after: &State) -> Vec<String> {
    let mut notes = Vec::new();
    for item in &plan.items {
        if after
            .get(&item.slug)
            .is_some_and(|slug| slug.aided_mismatch)
        {
            notes.push(format!(
                "{}  the vault and the verifier disagree — confirm the item, then rote rekey",
                item.slug
            ));
        }
        let Some(slug) = after.get(&item.slug) else {
            continue;
        };
        match slug.gate() {
            Gate::Ready { passes } => notes.push(format!(
                "{}  cutover ready — {passes} cold passes at {}d",
                slug.slug,
                ladder::cap_days()
            )),
            Gate::Climbing { .. } | Gate::Below => {}
        }
    }
    notes
}

fn nothing_due(state: &State, today: Date) -> String {
    let next = state
        .scheduled()
        .into_iter()
        .filter_map(|slug| match slug.standing(today) {
            Standing::Waiting { until } | Standing::Held { until } => Some(until),
            Standing::Due | Standing::Probe => None,
        })
        .min();
    match next {
        Some(day) => format!(
            "nothing due · next in {}",
            relic_core::fmt::plural(
                usize::try_from(ladder::days_between(today, day)).unwrap_or(0),
                "day",
                "days"
            )
        ),
        None => "nothing to drill · rote add <slug> to start one".to_owned(),
    }
}

fn new_session() -> Result<SessionId> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).context("drawing a session identifier")?;
    Ok(SessionId::from_bytes(bytes))
}

fn record(ctx: &Context, today: Date, event: Event) -> Record {
    Record {
        v: SCHEMA,
        at: ctx
            .clock
            .now()
            .round(jiff::Unit::Second)
            .unwrap_or(ctx.clock.now()),
        day: today,
        host: ctx.env.host.clone(),
        prev: Digest::GENESIS,
        event,
    }
}

fn write_cache(ctx: &Context, state: &State, today: Date) -> Result<()> {
    let due = state
        .scheduled()
        .into_iter()
        .map(|slug| match slug.standing(today) {
            Standing::Due | Standing::Probe => today,
            Standing::Waiting { until } | Standing::Held { until } => until,
        })
        .collect();
    Cache { v: SCHEMA, due }.save(&ctx.paths.cache())
}

// Enrollment and rotation.

fn add(ctx: &Context, args: &AddArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    if state.get(&args.slug).is_some() {
        bail!(
            "{} is already enrolled. Use rote rekey to replace its verifier",
            args.slug
        );
    }
    let today = ctx.today();
    let heading = format!("enroll {}", args.slug);
    let mut dialog = dialog(args.stdin, ctx.color)?;
    let secret = match dialog.as_mut() {
        None => piped(1)?.into_iter().next().unwrap_or_else(Secret::new),
        Some(console) => {
            let Some(first) = typed(console, today, &heading, "type it twice")? else {
                return abandoned(console, today, &heading);
            };
            let Some(again) = typed(console, today, &heading, "again")? else {
                return abandoned(console, today, &heading);
            };
            if !first.same_as(&again) {
                return refused(console, today, &heading, "the two entries differ");
            }
            first
        }
    };
    if secret.is_empty() {
        return match dialog.as_mut() {
            Some(console) => refused(console, today, &heading, "an empty secret is not a secret"),
            None => bail!("an empty secret is not a secret"),
        };
    }
    let verifier = make_verifier(&secret)?;
    drop(secret);

    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    verifiers.set(&args.slug, &verifier);
    verifiers.save(&ctx.paths.verifiers())?;

    store.append(&record(
        ctx,
        today,
        Event::Add(Added {
            slug: args.slug.clone(),
            version: 1,
            critical: args.critical,
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    let said = format!("{} enrolled · first review tomorrow", args.slug);
    match dialog.as_mut() {
        Some(console) => tui::outcome(console, today, &heading, &said, card::GREEN)?,
        None => {
            if !ctx.quiet {
                println!("{said}");
            }
        }
    }
    Ok(CLEAN)
}

fn rekey(ctx: &Context, args: &RekeyArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let Some(slug) = state.get(&args.slug) else {
        bail!("there is no slug called {}", args.slug);
    };
    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();
    let heading = format!("rekey {}", args.slug);
    let mut dialog = dialog(args.stdin, ctx.color)?;

    let mut piped_secrets = if args.stdin {
        piped(if args.force { 1 } else { 2 })?.into_iter()
    } else {
        Vec::new().into_iter()
    };

    if !args.force {
        let proved = prove(
            args,
            &verifiers,
            piped_secrets.next(),
            &mut dialog,
            today,
            &heading,
        )?;
        if let Some(code) = proved {
            return Ok(code);
        }
    }

    let secret = if let Some(secret) = piped_secrets.next() {
        secret
    } else {
        let Some(console) = dialog.as_mut() else {
            bail!("--stdin wants a new secret on its own line");
        };
        let Some(first) = typed(console, today, &heading, "the new secret, typed twice")? else {
            return abandoned(console, today, &heading);
        };
        let Some(again) = typed(console, today, &heading, "again")? else {
            return abandoned(console, today, &heading);
        };
        if !first.same_as(&again) {
            return refused(console, today, &heading, "the two entries differ");
        }
        first
    };
    if secret.is_empty() {
        return match dialog.as_mut() {
            Some(console) => refused(console, today, &heading, "an empty secret is not a secret"),
            None => bail!("an empty secret is not a secret"),
        };
    }
    let verifier = make_verifier(&secret)?;
    drop(secret);

    let version = slug.version.saturating_add(1);
    verifiers.set(&args.slug, &verifier);
    verifiers.save(&ctx.paths.verifiers())?;

    store.append(&record(
        ctx,
        today,
        Event::Rekey(Rekeyed {
            slug: args.slug.clone(),
            version,
            proved: !args.force,
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    let said = format!(
        "{} rekeyed to version {version} · the ladder starts over",
        args.slug
    );
    match dialog.as_mut() {
        Some(console) => tui::outcome(console, today, &heading, &said, card::GREEN)?,
        None => {
            if !ctx.quiet {
                println!("{said}");
            }
        }
    }
    Ok(CLEAN)
}

fn retire(ctx: &Context, args: &RetireArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let Some(slug) = state.get(&args.slug) else {
        bail!("there is no slug called {}", args.slug);
    };
    if slug.retired {
        bail!("{} is already retired", args.slug);
    }
    // The verifier goes with it: an oracle for a secret nobody drills any more
    // is cost with no benefit.
    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    verifiers.remove(&args.slug);
    verifiers.save(&ctx.paths.verifiers())?;

    let today = ctx.today();
    store.append(&record(
        ctx,
        today,
        Event::Retire(Retired {
            slug: args.slug.clone(),
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    if !ctx.quiet {
        println!(
            "{} retired · its history stays, its verifier does not",
            args.slug
        );
    }
    Ok(CLEAN)
}

fn probe(ctx: &Context, args: &ProbeArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    if state.get(&args.slug).is_none() {
        bail!("there is no slug called {}", args.slug);
    }
    let today = ctx.today();
    let until = if args.clear {
        None
    } else {
        let Some(days) = args.days else {
            bail!("say how far out the horizon sits, with --in <DAYS>, or drop it with --clear");
        };
        Some(
            today
                .checked_add(jiff::Span::new().days(i64::from(days)))
                .context("that horizon does not land on a date")?,
        )
    };
    store.append(&record(
        ctx,
        today,
        Event::Probe(Probed {
            slug: args.slug.clone(),
            until,
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    if !ctx.quiet {
        match until {
            Some(day) => println!("{} held out of the reminder until {day}", args.slug),
            None => println!("{} back on the schedule", args.slug),
        }
    }
    Ok(CLEAN)
}

/// Prove the current secret before a rotation replaces it.
///
/// Without it a verifier could be replaced by one somebody else knows, and the
/// reading would still say memorised. `Some(code)` means the rotation stopped
/// here and that is the status to leave on.
fn prove(
    args: &RekeyArgs,
    verifiers: &Verifiers,
    piped: Option<Secret>,
    dialog: &mut Option<term::Terminal>,
    today: Date,
    heading: &str,
) -> Result<Option<u8>> {
    // A rotation proves the current secret before accepting a new one, so a
    // verifier cannot be replaced by one somebody else knows: without it, "I
    // have memorised it" could be made true by editing a file.
    let Some(current) = verifiers.get(&args.slug)? else {
        bail!(
            "there is no verifier for {} on this machine, so there is nothing to prove against. Use --force to re-enroll",
            args.slug
        );
    };
    let offered = if let Some(secret) = piped {
        secret
    } else {
        let Some(console) = dialog.as_mut() else {
            bail!("--stdin wants the current secret first, then the new one");
        };
        let Some(secret) = typed(
            console,
            today,
            heading,
            "prove the current secret before it is replaced",
        )?
        else {
            return abandoned(console, today, heading).map(Some);
        };
        secret
    };
    if current.accepts(&offered)? {
        return Ok(None);
    }
    let said = "that is not the current secret, so nothing was replaced";
    match dialog.as_mut() {
        Some(console) => refused(console, today, heading, said).map(Some),
        None => bail!("{said}"),
    }
}

/// The screen a secret is typed on, or nothing when this is a pipe.
///
/// The only place a dialog is opened. Raw mode and the alternate screen are
/// process-wide, so a command that opens a second one puts the terminal back
/// under the first — and holding that to one call site is what keeps a later
/// edit from reintroducing it. `Terminal::enter` refuses a second all the same.
fn dialog(stdin: bool, color: bool) -> Result<Option<term::Terminal>> {
    if stdin {
        return Ok(None);
    }
    term::Terminal::enter(color).map(Some)
}

/// Ask for one secret. `None` means the dialog was abandoned rather than
/// answered.
fn typed(
    console: &mut term::Terminal,
    today: Date,
    heading: &str,
    intention: &str,
) -> Result<Option<Secret>> {
    let prompt = tui::Ask { heading, intention };
    match tui::ask(console, today, &prompt)? {
        tui::Typed::Submitted(entry) => Ok(Some(entry.secret)),
        // Nothing here has a vault to consult: the secret being enrolled is the
        // one the reader brought with them.
        tui::Typed::Skipped | tui::Typed::Aborted | tui::Typed::Lookup => Ok(None),
    }
}

/// Close a dialog that was walked away from.
fn abandoned(console: &mut term::Terminal, today: Date, heading: &str) -> Result<u8> {
    tui::outcome(console, today, heading, "nothing was changed", card::DIM)?;
    Ok(INCOMPLETE)
}

/// Close a dialog on an outcome that is a refusal.
///
/// Said on the card and held, rather than raised: the screen is erased on the
/// way out, so an error printed after it is an error printed to a person who has
/// already been told nothing.
fn refused(console: &mut term::Terminal, today: Date, heading: &str, text: &str) -> Result<u8> {
    tui::outcome(console, today, heading, text, card::RED)?;
    Ok(INCOMPLETE)
}

/// Read secrets from a pipe, one per line.
///
/// Refused when any standard stream is a terminal, which is what stops this
/// becoming a habit that leaves secrets in shell history.
fn piped(count: usize) -> Result<Vec<Secret>> {
    use std::io::Read as _;
    if term::is_interactive() {
        bail!("--stdin is for a pipe. At a terminal, type the secret instead");
    }
    let mut buffer = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .read_to_end(&mut buffer)
        .context("reading stdin")?;
    let mut secrets = Vec::new();
    for line in buffer.split(|byte| *byte == b'\n').take(count) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        secrets.push(
            Secret::from_bytes(line).context("that is longer than a secret this tool will take")?,
        );
    }
    if secrets.len() < count {
        bail!("expected {count} lines on stdin, one secret per line");
    }
    Ok(secrets)
}

fn make_verifier(secret: &Secret) -> Result<Verifier> {
    let mut salt = [0u8; crate::verifier::SALT_LEN];
    getrandom::fill(&mut salt).context("drawing a salt")?;
    Ok(Verifier::create(secret, &salt)?)
}

// Reading.

fn status(ctx: &Context, args: &ScheduleArgs) -> Result<u8> {
    let journal = Journal::load(&ctx.paths.log())?;
    let state = State::replay(journal.lines());
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    let slugs: Vec<&SlugState> = if args.all {
        state.all().collect()
    } else {
        state.scheduled()
    };

    if ctx.format == Format::Json {
        let rows: Vec<serde_json::Value> = slugs
            .iter()
            .map(|slug| {
                serde_json::json!({
                    "slug": slug.slug.as_str(),
                    "critical": slug.critical,
                    "retired": slug.retired,
                    "version": slug.version,
                    "step": slug.step.get(),
                    "interval_days": slug.step.interval(),
                    "last_review": slug.last_review.map(|d| d.to_string()),
                    "last_attempt": slug.last_attempt.map(|d| d.to_string()),
                    "standing": standing_word(slug.standing(today)),
                    "due": due_day(slug, today).map(|d| d.to_string()),
                    "cap_passes": slug.cap_passes,
                    "stood_alone": slug.first_unaided.map(|d| d.to_string()),
                    "aided_mismatch": slug.aided_mismatch,
                    "gate": gate_word(slug.gate()),
                    "verifier": verifier_word(slug, &verifiers),
                })
            })
            .collect();
        println!(
            "{}",
            json::document(&serde_json::json!({"today": today.to_string(), "slugs": rows}))?
        );
        return Ok(CLEAN);
    }

    let mut table = Table::new(&[
        "SLUG", "STEP", "EVERY", "STOOD", "LAST", "NEXT", "VERIFIER", "GATE",
    ]);
    for slug in &slugs {
        table.push(vec![
            slug.slug.to_string(),
            format!("{}/{}", slug.step.get(), ladder::LADDER.len() - 1),
            format!("{}d", slug.step.interval()),
            since(slug.first_unaided, today),
            since(slug.last_attempt, today),
            next_word(slug, today),
            verifier_word(slug, &verifiers).to_owned(),
            gate_text(slug),
        ]);
    }
    // The reminder cache is derived, so the command that replays the whole log
    // is also the natural place to repair it. Best effort: a reminder that
    // cannot be rewritten is not a reason to fail a reading.
    let _ = write_cache(ctx, &state, today);

    let notes = status_notes(&slugs, today);
    let heading = format!("drills · {today}");
    println!("{}", block(ctx, &heading, &table, &notes));
    Ok(CLEAN)
}

fn status_notes(slugs: &[&SlugState], today: Date) -> Vec<String> {
    let mut notes = Vec::new();
    for slug in slugs {
        if let Gate::Climbing { passes } = slug.gate()
            && slug.warm()
        {
            notes.push(format!(
                "{}: {passes} of {} cold passes at the cap. Practicing it inside the interval is why the next one will not count",
                slug.slug,
                ladder::GATE_PASSES
            ));
        }
        if let Standing::Held { until } = slug.standing(today) {
            notes.push(format!(
                "{}: held for a stretch probe until {until}",
                slug.slug
            ));
        }
        if slug.first_unaided.is_none() && !slug.retired {
            notes.push(format!(
                "{}: has not stood alone yet — no unaided first-attempt pass on record",
                slug.slug
            ));
        }
        if slug.aided_mismatch {
            notes.push(format!(
                "{}: an aided entry was refused — the vault and the verifier disagree",
                slug.slug
            ));
        }
    }
    notes
}

fn due_day(slug: &SlugState, today: Date) -> Option<Date> {
    match slug.standing(today) {
        Standing::Due | Standing::Probe => Some(today),
        Standing::Waiting { until } | Standing::Held { until } => Some(until),
    }
}

fn standing_word(standing: Standing) -> &'static str {
    match standing {
        Standing::Due => "due",
        Standing::Probe => "probe",
        Standing::Held { .. } => "held",
        Standing::Waiting { .. } => "waiting",
    }
}

fn gate_word(gate: Gate) -> &'static str {
    match gate {
        Gate::Ready { .. } => "ready",
        Gate::Climbing { .. } => "climbing",
        Gate::Below => "below",
    }
}

fn gate_text(slug: &SlugState) -> String {
    match slug.gate() {
        Gate::Ready { passes } => format!("ready · {passes} at cap"),
        Gate::Climbing { passes } => format!("{passes}/{} at cap", ladder::GATE_PASSES),
        Gate::Below => "—".to_owned(),
    }
}

fn verifier_word(slug: &SlugState, verifiers: &Verifiers) -> &'static str {
    match verifiers.get(&slug.slug) {
        Ok(Some(verifier)) => {
            if verifier.meets_floor().unwrap_or(false) {
                "ok"
            } else {
                "weak"
            }
        }
        Ok(None) => "missing",
        Err(_) => "unreadable",
    }
}

fn next_word(slug: &SlugState, today: Date) -> String {
    // A retired slug is only ever listed by --all, and its schedule is a
    // leftover: nothing will ask for it again.
    if slug.retired {
        return "retired".to_owned();
    }
    match slug.standing(today) {
        Standing::Due => "due".to_owned(),
        Standing::Probe => "probe due".to_owned(),
        Standing::Held { until } => format!("held to {until}"),
        Standing::Waiting { until } => {
            format!("in {}d", ladder::days_between(today, until))
        }
    }
}

fn since(day: Option<Date>, today: Date) -> String {
    match day {
        None => "never".to_owned(),
        Some(day) if day == today => "today".to_owned(),
        Some(day) => format!("{}d ago", ladder::days_between(day, today)),
    }
}

fn stats(ctx: &Context, args: &MeasurementArgs) -> Result<u8> {
    let journal = Journal::load(&ctx.paths.log())?;
    let today = ctx.today();
    let stats = Stats::gather(journal.lines(), today, args.days);

    if ctx.format == Format::Json {
        println!("{}", json::document(&stats_json(&stats))?);
        return Ok(CLEAN);
    }

    let mut table = Table::new(&[
        "SLUG",
        "AIDED",
        "RETENTION",
        "TTFK",
        "TYPING",
        "RECENT",
        "TREND",
    ]);
    for slug in &stats.slugs {
        table.push(vec![
            slug.slug.to_string(),
            slug.aided.to_string(),
            rate(slug.retention),
            millis(slug.ttfk_ms),
            millis(slug.total_ms),
            crate::stats::sparkline(&slug.recent_ttfk),
            trend_text(slug.trend),
        ]);
    }

    let mut notes = Vec::new();
    if stats.retention.total == 0 && stats.practice.total == 0 && stats.aided.total == 0 {
        notes.push(format!("no entries in the last {}d", stats.window_days));
    } else {
        notes.push(format!(
            "true retention {} over scheduled reviews · practice {}",
            rate(stats.retention),
            rate(stats.practice)
        ));
        if stats.aided.total > 0 {
            let refused = stats.aided.total.saturating_sub(stats.aided.passes);
            notes.push(format!(
                "{} aided, in no figure above{}",
                stats.aided.total,
                if refused > 0 {
                    format!(" · {refused} refused, so the vault and the verifier disagree")
                } else {
                    String::new()
                }
            ));
        }
        if stats.punctuality.total > 0 {
            notes.push(format!(
                "punctuality {} taken within a day of falling due{}",
                ratio(stats.punctuality.on_time, stats.punctuality.total),
                match stats.punctuality.median_lateness {
                    Some(days) => format!(" · median {days}d late"),
                    None => String::new(),
                }
            ));
        }
    }
    let bands: Vec<String> = stats
        .buckets
        .iter()
        .filter(|bucket| bucket.retention.total > 0)
        .map(|bucket| format!("{} {}", bucket.label, rate(bucket.retention)))
        .collect();
    if !bands.is_empty() {
        notes.push(format!("by interval · {}", bands.join("  ")));
    }
    for lapse in stats.lapses.iter().take(5) {
        notes.push(format!(
            "lapse {} on {} at {} against a scheduled {}d{}",
            lapse.slug,
            lapse.day,
            match lapse.effective {
                Some(days) => format!("{days}d"),
                None => "an unknown interval".to_owned(),
            },
            lapse.scheduled,
            if lapse.beyond_schedule() {
                " · beyond schedule"
            } else {
                ""
            }
        ));
    }
    let heading = format!("measurement · last {}d", stats.window_days);
    println!("{}", block(ctx, &heading, &table, &notes));
    Ok(CLEAN)
}

fn stats_json(stats: &Stats) -> serde_json::Value {
    serde_json::json!({
        "window_days": stats.window_days,
        "retention": {"passes": stats.retention.passes, "total": stats.retention.total},
        "practice": {"passes": stats.practice.passes, "total": stats.practice.total},
        "aided": {"passes": stats.aided.passes, "total": stats.aided.total},
        "punctuality": {
            "on_time": stats.punctuality.on_time,
            "total": stats.punctuality.total,
            "median_lateness_days": stats.punctuality.median_lateness,
        },
        "buckets": stats.buckets.iter().map(|bucket| serde_json::json!({
            "interval": bucket.label,
            "passes": bucket.retention.passes,
            "total": bucket.retention.total,
        })).collect::<Vec<_>>(),
        "slugs": stats.slugs.iter().map(|slug| serde_json::json!({
            "slug": slug.slug.as_str(),
            "passes": slug.retention.passes,
            "total": slug.retention.total,
            "aided": slug.aided,
            "ttfk_ms": slug.ttfk_ms,
            "total_ms": slug.total_ms,
            "trend": trend_text(slug.trend),
        })).collect::<Vec<_>>(),
        "lapses": stats.lapses.iter().map(|lapse| serde_json::json!({
            "slug": lapse.slug.as_str(),
            "day": lapse.day.to_string(),
            "scheduled_interval_days": lapse.scheduled,
            "effective_interval_days": lapse.effective,
            "beyond_schedule": lapse.beyond_schedule(),
        })).collect::<Vec<_>>(),
    })
}

fn trend_text(trend: Trend) -> String {
    match trend {
        Trend::Unknown => "not yet".to_owned(),
        Trend::Steady => "steady".to_owned(),
        Trend::Rising(percent) => format!("slower by {percent}%"),
        Trend::Falling(percent) => format!("faster by {percent}%"),
    }
}

fn rate(retention: crate::stats::Retention) -> String {
    match retention.percent() {
        Some(percent) => format!("{percent:.0}% of {}", retention.total),
        None => "—".to_owned(),
    }
}

fn ratio(part: u32, whole: u32) -> String {
    if whole == 0 {
        "—".to_owned()
    } else {
        format!("{part}/{whole}")
    }
}

fn millis(value: Option<u64>) -> String {
    match value {
        Some(ms) => format!(
            "{:.1}s",
            f64::from(u32::try_from(ms).unwrap_or(u32::MAX)) / 1000.0
        ),
        None => "—".to_owned(),
    }
}

fn log(ctx: &Context, args: &LogArgs) -> Result<u8> {
    let journal = Journal::load(&ctx.paths.log())?;
    let wanted: Vec<&Line> = journal
        .lines()
        .filter(|line| match line.record() {
            Some(record) => args
                .slug
                .as_ref()
                .is_none_or(|slug| record.event.slug() == slug),
            None => args.slug.is_none(),
        })
        .collect();
    let shown = wanted.len().saturating_sub(args.count);

    if ctx.format == Format::Json {
        let rows: Vec<serde_json::Value> = wanted
            .into_iter()
            .skip(shown)
            .map(|line| match line {
                Line::Parsed(record) => {
                    serde_json::to_value(record.as_ref()).unwrap_or(serde_json::Value::Null)
                }
                Line::Malformed { raw, why } => {
                    serde_json::json!({"malformed": raw, "why": why})
                }
            })
            .collect();
        println!("{}", json::document(&serde_json::json!({"records": rows}))?);
        return Ok(CLEAN);
    }

    let mut table = Table::new(&["DAY", "KIND", "SLUG", "WHAT"]);
    let total = journal.records();
    for line in wanted.into_iter().skip(shown) {
        match line {
            Line::Parsed(record) => table.push(vec![
                record.day.to_string(),
                record.event.kind().to_owned(),
                record.event.slug().to_string(),
                event_text(&record.event),
            ]),
            Line::Malformed { why, .. } => table.push(vec![
                "?".to_owned(),
                "malformed".to_owned(),
                "?".to_owned(),
                why.clone(),
            ]),
        }
    }
    let heading = format!("records · {} of {total}", table.rows.len());
    println!("{}", block(ctx, &heading, &table, &[]));
    Ok(CLEAN)
}

fn event_text(event: &Event) -> String {
    match event {
        Event::Add(added) => {
            let mut text = format!("version {}", added.version);
            if added.critical {
                text.push_str(" · critical");
            }
            text
        }
        Event::Rekey(rekeyed) => format!(
            "version {}{}",
            rekeyed.version,
            if rekeyed.proved {
                ""
            } else {
                " · re-enrolled without proving the old one"
            }
        ),
        Event::Retire(_) => "off the schedule".to_owned(),
        Event::Probe(probed) => match probed.until {
            Some(until) => format!("held until {until}"),
            None => "horizon dropped".to_owned(),
        },
        Event::Attempt(entry) => {
            let class = match entry.class {
                Class::Review => "review",
                Class::Practice => "practice",
                Class::Probe => "probe",
                Class::Aided => "aided",
            };
            let outcome = match entry.outcome {
                Outcome::Pass => "pass",
                Outcome::Fail => "fail",
                Outcome::Blank => "blank",
                Outcome::Skip => "skip",
                Outcome::Abort => "abandoned",
            };
            let mut text = format!("{class} · {outcome} · try {}", entry.attempt);
            // A lookup has no interval worth naming: whatever elapsed, the
            // answer was on the screen.
            if let Some(days) = entry
                .effective_interval_days
                .filter(|_| entry.class.unaided())
            {
                let _ = std::fmt::Write::write_fmt(&mut text, format_args!(" · {days}d cold"));
            }
            if entry.stretch {
                text.push_str(" · stretch");
            }
            text
        }
    }
}

fn block(ctx: &Context, heading: &str, table: &Table, notes: &[String]) -> String {
    match ctx.format {
        Format::Human => human::block(heading, table, notes, ctx.color),
        Format::Agent | Format::Json => agent::block(heading, table, notes),
    }
}

// Health, and the reminder.

fn run_doctor(ctx: &Context) -> Result<u8> {
    let today = ctx.today();
    let report = match Journal::load(&ctx.paths.log()) {
        Ok(journal) => {
            let state = State::replay(journal.lines());
            let verifiers = Verifiers::load(&ctx.paths.verifiers()).unwrap_or_default();
            crate::doctor::report(&journal, &state, &verifiers, today, &ctx.env.host)
        }
        Err(error) => crate::doctor::unreadable(&format!("{error:#}")),
    };
    if ctx.format == Format::Json {
        println!("{}", json::document(&report)?);
    } else {
        println!("{}", crate::doctor::render(&report, ctx.color));
    }
    Ok(match report.grade() {
        Grade::Ok => CLEAN,
        Grade::Soft => LAPSE,
        Grade::Broken => INCOMPLETE,
    })
}

fn banner(ctx: &Context) -> Result<u8> {
    // Decoration, and decoration fails silent: a reminder that cannot read its
    // own cache prints nothing rather than a diagnostic in every new shell.
    let due = Cache::load(&ctx.paths.cache()).map_or(0, |cache| cache.due_by(ctx.today()));

    // The nag is not the health check. This fires the day a drill comes due;
    // doctor waits out a grace, because a drill taken a day late is a drill
    // taken. coop reads this one, and assay's registry station reads that one.
    if ctx.format == Format::Json {
        let station = StationId::from_static("rote");
        let findings = if due == 0 {
            Vec::new()
        } else {
            vec![
                station
                    .soft(Summary::lossy(&format!(
                        "{} due",
                        relic_core::fmt::plural(due, "drill", "drills")
                    )))
                    .fixed_by(FixHint::lossy("rote")),
            ]
        };
        println!("{}", json::document(&Report::ran(station, findings))?);
        return Ok(CLEAN);
    }

    if due == 0 {
        return Ok(CLEAN);
    }
    let text = format!(
        "rote: {} due.",
        relic_core::fmt::plural(due, "drill", "drills")
    );
    if ctx.color {
        println!("\x1b[1m==>\x1b[0m \x1b[1;33m{text}\x1b[0m");
    } else {
        println!("==> {text}");
    }
    Ok(CLEAN)
}
