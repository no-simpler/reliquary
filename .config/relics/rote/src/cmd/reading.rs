//! The tables: the schedule, the measurement, the records, the machines.

use anyhow::Result;
use jiff::civil::Date;
use relic_core::ui::Format;

use super::{Context, block, flagship, read_chains};
use crate::cli::{LogArgs, MeasurementArgs, ScheduleArgs};
use crate::corpus::record::{Drilled, Event, Line};
use crate::corpus::{Corpus, Dossier, Lineage};
use crate::exit::CLEAN;
use crate::ladder::{self, Ladder, Standing};
use crate::render::{Table, json, seconds};
use crate::stats::{Retention, Stats};
use crate::verifier::file::Verifiers;

/// One row of the schedule.
struct ScheduleRow<'a> {
    lineage: &'a Lineage,
    dossier: &'a Dossier,
    ordinal: usize,
}

impl ScheduleRow<'_> {
    fn label(&self) -> String {
        self.lineage.label(self.ordinal)
    }

    fn current(&self) -> bool {
        self.dossier.is_current()
    }
}

/// `rote status`.
///
/// # Errors
///
/// When the corpus cannot be read.
pub fn status(ctx: &Context, args: &ScheduleArgs) -> Result<u8> {
    let chains = read_chains(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(chains.records(), &ladder);
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    let mut rows: Vec<ScheduleRow<'_>> = Vec::new();
    for lineage in corpus.lineages() {
        if lineage.retired && !args.all {
            continue;
        }
        for (index, dossier) in lineage.engrams.iter().enumerate() {
            if !dossier.is_current() {
                continue;
            }
            rows.push(ScheduleRow {
                lineage,
                dossier,
                ordinal: index.saturating_add(1),
            });
        }
    }
    rows.sort_by(|a, b| {
        b.lineage
            .critical
            .cmp(&a.lineage.critical)
            .then_with(|| a.lineage.slug.cmp(&b.lineage.slug))
            .then_with(|| a.ordinal.cmp(&b.ordinal))
    });

    if ctx.format == Format::Json {
        let listed: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "lineage": row.lineage.slug.as_str(),
                    "ordinal": row.ordinal,
                    "engram": row.dossier.engram.to_string(),
                    "label": row.label(),
                    "critical": row.lineage.critical,
                    "retired": row.lineage.retired,
                    "current": row.current(),
                    "rung": row.dossier.rung.get(),
                    "interval_days": ladder.interval(row.dossier.rung),
                    "last_review": row.dossier.last_review.map(|d| d.to_string()),
                    "last_exposed": row.dossier.last_exposed.to_string(),
                    "standing": standing_word(row.dossier.standing(today, &ladder)),
                    "due": row.dossier.due(&ladder).to_string(),
                    "here": here(row, &verifiers),
                })
            })
            .collect();
        println!(
            "{}",
            json::document(&serde_json::json!({
                "today": today.to_string(),
                "machine": ctx.machine().ok().map(|m| m.to_string()),
                "host": ctx.env.host,
                "engrams": listed,
            }))?
        );
        return Ok(CLEAN);
    }

    let mut table = Table::new(&["ENGRAM", "RUNG", "EVERY", "LAST", "NEXT", "HERE"]);
    for row in &rows {
        table.push(vec![
            row.label(),
            format!("{}/{}", row.dossier.rung.get(), ladder.cap().get()),
            format!("{}d", ladder.interval(row.dossier.rung)),
            since(row.dossier.last_exposed, today),
            next_word(row, today, &ladder),
            here(row, &verifiers).to_owned(),
        ]);
    }

    let notes = status_notes(ctx, &rows, &verifiers);
    let heading = match ctx.machine() {
        Ok(machine) => format!(
            "drills · {today} · {}",
            crate::machine::label(&machine, &ctx.env.host)
        ),
        // No identity is its own finding, and doctor is where it is made. A
        // reading still reads.
        Err(_) => format!(
            "drills · {today} · {}",
            crate::machine::short_host(&ctx.env.host)
        ),
    };
    println!("{}", block(ctx, &heading, &table, &notes));
    Ok(CLEAN)
}

fn status_notes(ctx: &Context, rows: &[ScheduleRow<'_>], verifiers: &Verifiers) -> Vec<String> {
    let mut notes = Vec::new();
    if let Ok(machine) = ctx.machine()
        && let Ok(state) = flagship(ctx, &machine)
        && !state.writes_allowed()
    {
        notes.push("this machine is not the flagship, so rote will not write here".to_owned());
    }
    for row in rows.iter().filter(|row| row.current()) {
        if row.lineage.retired {
            continue;
        }
        if matches!(here_state(row, verifiers), Held::Dormant) {
            notes.push(format!(
                "{}: dormant here — rote attach {}",
                row.label(),
                row.lineage.slug
            ));
        }
    }
    notes
}

enum Held {
    Attached,
    Dormant,
    Unreadable,
}

fn here_state(row: &ScheduleRow<'_>, verifiers: &Verifiers) -> Held {
    match verifiers.get(&row.dossier.engram) {
        Ok(Some(_)) => Held::Attached,
        Ok(None) => Held::Dormant,
        Err(_) => Held::Unreadable,
    }
}

/// Whether this machine holds a verifier for an engram. Named for where it is
/// true: attachment is machine-local, and the corpus does not know it.
fn here(row: &ScheduleRow<'_>, verifiers: &Verifiers) -> &'static str {
    match here_state(row, verifiers) {
        Held::Attached => "attached",
        Held::Dormant => "dormant",
        Held::Unreadable => "unreadable",
    }
}

fn standing_word(standing: Standing) -> &'static str {
    match standing {
        Standing::Due => "due",
        Standing::Waiting { .. } => "waiting",
    }
}

fn next_word(row: &ScheduleRow<'_>, today: Date, ladder: &Ladder) -> String {
    if row.lineage.retired {
        return "retired".to_owned();
    }
    match row.dossier.standing(today, ladder) {
        Standing::Due => "due".to_owned(),
        Standing::Waiting { until } => {
            format!("in {}d", ladder::days_between(today, until))
        }
    }
}

fn since(day: Date, today: Date) -> String {
    if day == today {
        "today".to_owned()
    } else {
        format!("{}d ago", ladder::days_between(day, today))
    }
}

/// `rote stats`.
///
/// # Errors
///
/// When the corpus cannot be read.
pub fn stats(ctx: &Context, args: &MeasurementArgs) -> Result<u8> {
    let chains = read_chains(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(chains.records(), &ladder);
    let today = ctx.today();
    let stats = Stats::gather(&corpus, today, args.days, &ladder);

    if ctx.format == Format::Json {
        println!("{}", json::document(&stats_json(&stats))?);
        return Ok(CLEAN);
    }

    let table = stats_table(&stats);
    let notes = stats_notes(&stats, &ladder);
    let heading = format!("measurement · last {}d", stats.window_days);
    println!("{}", block(ctx, &heading, &table, &notes));
    Ok(CLEAN)
}

/// The table: one row per engram, never pooled across a rotation.
fn stats_table(stats: &Stats) -> Table {
    let mut table = Table::new(&["ENGRAM", "DRILLS", "RETENTION", "STREAK", "RECALL"]);
    for engram in &stats.engrams {
        table.push(vec![
            engram.label.clone(),
            engram.drills.to_string(),
            rate(engram.retention),
            engram.streak.to_string(),
            seconds(engram.ttfk_ms),
        ]);
    }
    table
}

/// What sits under the table: the headline figures, the bands, the fails.
fn stats_notes(stats: &Stats, ladder: &Ladder) -> Vec<String> {
    let mut notes = Vec::new();
    if stats.drills == 0 {
        notes.push(format!(
            "nothing recorded in the last {}d",
            stats.window_days
        ));
    } else {
        notes.push(format!(
            "retention {} · cold passes over every drill",
            rate(stats.retention)
        ));
        if let Some(days) = stats.median_lateness_days {
            notes.push(format!("reviews ran a median {days}d past due"));
        }
    }
    notes.push(format!(
        "streak counts cold passes at {}d or longer, back from the last drill",
        ladder.cap_days()
    ));
    let bands: Vec<String> = stats
        .buckets
        .iter()
        .filter(|bucket| bucket.retention.total > 0)
        .map(|bucket| format!("{} {}", bucket.label, rate(bucket.retention)))
        .collect();
    if !bands.is_empty() {
        notes.push(format!("by interval · {}", bands.join("  ")));
    }
    for fail in stats.fails.iter().take(5) {
        let mut text = format!(
            "fail {} on {} at {}d against a scheduled {}d",
            fail.label, fail.day, fail.effective, fail.scheduled
        );
        for (holds, word) in [
            (fail.recovered, "recovered"),
            (fail.aided, "aided"),
            (fail.beyond_schedule(), "beyond schedule"),
        ] {
            if holds {
                text.push_str(" · ");
                text.push_str(word);
            }
        }
        notes.push(text);
    }
    notes
}

fn stats_json(stats: &Stats) -> serde_json::Value {
    serde_json::json!({
        "window_days": stats.window_days,
        "drills": stats.drills,
        "retention": {"passes": stats.retention.passes, "total": stats.retention.total},
        "median_lateness_days": stats.median_lateness_days,
        "buckets": stats.buckets.iter().map(|bucket| serde_json::json!({
            "interval": bucket.label,
            "passes": bucket.retention.passes,
            "total": bucket.retention.total,
        })).collect::<Vec<_>>(),
        "fails": stats.fails.iter().map(|fail| serde_json::json!({
            "engram": fail.label,
            "day": fail.day.to_string(),
            "scheduled_days": fail.scheduled,
            "effective_days": fail.effective,
            "beyond_schedule": fail.beyond_schedule(),
            "recovered": fail.recovered,
            "aided": fail.aided,
        })).collect::<Vec<_>>(),
        "engrams": stats.engrams.iter().map(|row| serde_json::json!({
            "lineage": row.slug.as_str(),
            "engram": row.engram.to_string(),
            "label": row.label,
            "drills": row.drills,
            "passes": row.retention.passes,
            "total": row.retention.total,
            "streak": row.streak,
            "ttfk_ms": row.ttfk_ms,
        })).collect::<Vec<_>>(),
    })
}

fn rate(retention: Retention) -> String {
    match retention.percent() {
        Some(percent) => format!("{percent:.0}% of {}", retention.total),
        None => "—".to_owned(),
    }
}

/// `rote log`.
///
/// # Errors
///
/// When the corpus cannot be read.
pub fn log(ctx: &Context, args: &LogArgs) -> Result<u8> {
    let chains = read_chains(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(chains.records(), &ladder);
    let placed = chains.placed();
    let wanted: Vec<&crate::corpus::chain::Placed> = placed
        .into_iter()
        .filter(|found| match found.line.record() {
            Some(record) => args
                .lineage
                .as_ref()
                .is_none_or(|slug| about(&corpus, &record.event, slug)),
            None => args.lineage.is_none(),
        })
        .collect();
    let shown = wanted.len().saturating_sub(args.count);

    if ctx.format == Format::Json {
        let rows: Vec<serde_json::Value> = wanted
            .iter()
            .skip(shown)
            .map(|found| match &found.line {
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

    let total = chains.len();
    let mut table = Table::new(&["DAY", "KIND", "ENGRAM", "WHAT"]);
    for found in wanted.iter().skip(shown) {
        match &found.line {
            Line::Parsed(record) => table.push(vec![
                record.day.to_string(),
                record.event.kind().to_owned(),
                engram_label(&corpus, &record.event),
                event_text(&record.event, &corpus),
            ]),
            Line::Malformed { why, .. } => table.push(vec![
                "?".to_owned(),
                "malformed".to_owned(),
                found.machine.to_string(),
                why.clone(),
            ]),
        }
    }
    let heading = format!("records · {} of {total}", table.rows.len());
    println!("{}", block(ctx, &heading, &table, &[]));
    Ok(CLEAN)
}

/// Whether an event is about this lineage: by the engram it names, resolved
/// through the corpus, or by the slug where it names no engram.
fn about(corpus: &Corpus, event: &Event, slug: &crate::slug::Slug) -> bool {
    match (event.engram(), event) {
        (Some(engram), _) => corpus
            .owner(&engram)
            .is_some_and(|lineage| lineage.slug == *slug),
        (None, Event::Retire(retired)) => retired.slug == *slug,
        (None, _) => false,
    }
}

fn engram_label(corpus: &Corpus, event: &Event) -> String {
    match event {
        Event::Enroll(e) => corpus.label(&e.engram),
        Event::Rotate(e) => corpus.label(&e.to),
        Event::Attach(e) => corpus.label(&e.engram),
        Event::Drill(e) => corpus.label(&e.engram),
        Event::Retire(e) => e.slug.to_string(),
    }
}

fn event_text(event: &Event, corpus: &Corpus) -> String {
    match event {
        Event::Enroll(enrolled) => {
            if enrolled.critical {
                "opened · critical".to_owned()
            } else {
                "opened".to_owned()
            }
        }
        Event::Rotate(rotated) => match superseded(corpus, &rotated.to) {
            Some(previous) => format!("supersedes {previous}"),
            None => "the ladder starts over".to_owned(),
        },
        Event::Attach(_) => "a verifier was made here · nothing was checked".to_owned(),
        Event::Retire(_) => "off the schedule".to_owned(),
        Event::Drill(drilled) => drill_text(drilled),
    }
}

/// The label of the engram a rotation stepped down: the one before it in its
/// lineage, which replay knows and the record does not carry.
fn superseded(corpus: &Corpus, to: &crate::corpus::record::EngramId) -> Option<String> {
    let lineage = corpus.owner(to)?;
    let previous = lineage.ordinal(to)?.checked_sub(1)?;
    (previous >= 1).then(|| lineage.label(previous))
}

fn drill_text(drilled: &Drilled) -> String {
    let mut text = drilled.outcome.word().to_owned();
    if drilled.follow_ups > 0 {
        text.push_str(" · ");
        text.push_str(&relic_core::fmt::plural(
            usize::from(drilled.follow_ups),
            "follow-up",
            "follow-ups",
        ));
    }
    for (holds, word) in [(drilled.recovered, "recovered"), (drilled.aided, "aided")] {
        if holds {
            text.push_str(" · ");
            text.push_str(word);
        }
    }
    text
}

/// `rote machines`.
///
/// # Errors
///
/// When the corpus cannot be read.
pub fn machines(ctx: &Context) -> Result<u8> {
    let chains = read_chains(ctx)?;
    let mine = ctx.machine().ok();
    let placed = chains.placed();

    let mut rows = Vec::new();
    for machine in chains.machines() {
        let theirs: Vec<&crate::corpus::record::Record> = placed
            .iter()
            .filter(|found| found.machine == *machine)
            .filter_map(|found| found.line.record())
            .collect();
        let host = theirs
            .last()
            .map_or_else(String::new, |record| record.host.clone());
        let first = theirs.first().map(|record| record.day);
        let last = theirs.last().map(|record| record.day);
        rows.push((machine.clone(), host, theirs.len(), first, last));
    }

    let state = match &mine {
        Some(machine) => flagship(ctx, machine).ok(),
        None => None,
    };
    let writes = state
        .as_ref()
        .is_some_and(crate::machine::Flagship::writes_allowed);

    if ctx.format == Format::Json {
        println!(
            "{}",
            json::document(&serde_json::json!({
                "this": mine.as_ref().map(ToString::to_string),
                "host": ctx.env.host,
                "flagship": writes,
                "marker": ctx.paths.marker.to_string(),
                "machines": rows.iter().map(|(machine, host, count, first, last)| serde_json::json!({
                    "machine": machine.to_string(),
                    "host": host,
                    "records": count,
                    "first": first.map(|d| d.to_string()),
                    "last": last.map(|d| d.to_string()),
                    "this": Some(machine) == mine.as_ref(),
                })).collect::<Vec<_>>(),
            }))?
        );
        return Ok(CLEAN);
    }

    let mut table = Table::new(&["MACHINE", "HOST", "RECORDS", "FIRST", "LAST", "THIS"]);
    for (machine, host, count, first, last) in &rows {
        table.push(vec![
            machine.to_string(),
            crate::machine::short_host(host).to_owned(),
            count.to_string(),
            first.map_or_else(|| "—".to_owned(), |d| d.to_string()),
            last.map_or_else(|| "—".to_owned(), |d| d.to_string()),
            if Some(machine) == mine.as_ref() {
                "yes"
            } else {
                ""
            }
            .to_owned(),
        ]);
    }

    let mut notes = Vec::new();
    match &mine {
        Some(machine) => notes.push(format!(
            "this machine is {}",
            crate::machine::label(machine, &ctx.env.host)
        )),
        None => notes.push("this machine has no stable identity, so rote cannot write".to_owned()),
    }
    if let (Some(machine), Some(state)) = (&mine, &state)
        && !state.writes_allowed()
    {
        notes.push(state.refusal(&ctx.paths.marker, machine, &ctx.env.host));
    }
    println!("{}", block(ctx, "machines", &table, &notes));
    Ok(CLEAN)
}
