//! The tables: the schedule, the measurement, the records, the machines.

use anyhow::Result;
use jiff::civil::Date;
use relic_core::ui::Format;

use super::{Context, block, flagship, read_chains, write_cache};
use crate::cli::{LogArgs, MeasurementArgs, ScheduleArgs};
use crate::corpus::record::{Captured, Event, Line};
use crate::corpus::{Corpus, Dossier, Lineage};
use crate::exit::CLEAN;
use crate::ladder::{self, Ladder, Standing};
use crate::render::{Table, json, seconds};
use crate::stats::{Retention, Stats, Trend};
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
            if !dossier.is_current() && !args.history {
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
                    "stood_alone": row.dossier.first_unaided.map(|d| d.to_string()),
                    "aided_mismatch": row.dossier.aided_mismatch,
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

    let mut table = Table::new(&["ENGRAM", "RUNG", "EVERY", "STOOD", "LAST", "NEXT", "HERE"]);
    for row in &rows {
        table.push(vec![
            row.label(),
            format!("{}/{}", row.dossier.rung.get(), ladder.cap().get()),
            format!("{}d", ladder.interval(row.dossier.rung)),
            since(row.dossier.first_unaided, today),
            since(Some(row.dossier.last_exposed), today),
            next_word(row, today, &ladder),
            here(row, &verifiers).to_owned(),
        ]);
    }

    // The reminder cache is derived, so the command that replays the whole
    // corpus is also the natural place to repair it. Best effort: a reminder
    // that cannot be rewritten is not a reason to fail a reading.
    let _ = write_cache(ctx, &corpus, &verifiers, today, &ladder);

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
                "{}: dormant here — the next sitting will ask you to attach it",
                row.label()
            ));
        }
        if row.dossier.first_unaided.is_none() {
            notes.push(format!(
                "{}: has not stood alone yet — no unaided first pass on record",
                row.label()
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
    if !row.current() {
        return "superseded".to_owned();
    }
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

fn since(day: Option<Date>, today: Date) -> String {
    match day {
        None => "never".to_owned(),
        Some(day) if day == today => "today".to_owned(),
        Some(day) => format!("{}d ago", ladder::days_between(day, today)),
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
    let records = chains.records();
    let corpus = Corpus::replay(records.iter().copied(), &ladder);
    let today = ctx.today();
    let stats = Stats::gather(&records, &corpus, today, args.days, &ladder);

    if ctx.format == Format::Json {
        println!("{}", json::document(&stats_json(&stats, args.lineage))?);
        return Ok(CLEAN);
    }

    let table = stats_table(&stats, args.lineage);
    let notes = stats_notes(&stats, &ladder, args.lineage);
    let heading = format!("measurement · last {}d", stats.window_days);
    println!("{}", block(ctx, &heading, &table, &notes));
    Ok(CLEAN)
}

/// The table, in whichever shape was asked for.
fn stats_table(stats: &Stats, lineage: bool) -> Table {
    if lineage {
        let mut table = Table::new(&[
            "LINEAGE",
            "ENGRAMS",
            "DRILLS",
            "AIDED",
            "LAPSES",
            "PUNCTUALITY",
        ]);
        for row in &stats.lineages {
            table.push(vec![
                row.slug.to_string(),
                row.engrams.to_string(),
                row.drills.to_string(),
                row.aided.to_string(),
                row.lapses.to_string(),
                ratio(row.punctuality.on_time, row.punctuality.total),
            ]);
        }
        table
    } else {
        let mut table = Table::new(&[
            "ENGRAM",
            "AIDED",
            "RETENTION",
            "STREAK",
            "RECALL",
            "TYPING",
            "RECENT",
            "TREND",
        ]);
        for engram in &stats.engrams {
            table.push(vec![
                engram.label.clone(),
                engram.aided.to_string(),
                rate(engram.retention),
                engram.streak.to_string(),
                seconds(engram.ttfk_ms),
                seconds(engram.total_ms),
                crate::stats::sparkline(&engram.recent_ttfk),
                trend_text(engram.trend),
            ]);
        }
        table
    }
}

/// What sits under the table: the headline figures, the bands, the lapses.
fn stats_notes(stats: &Stats, ladder: &Ladder, lineage: bool) -> Vec<String> {
    let mut notes = Vec::new();
    if lineage {
        notes.push(
            "a rotation resets the memory, so only what survives one is rolled up here".to_owned(),
        );
    }
    if stats.retention.total == 0 && stats.practice.total == 0 && stats.aided.total == 0 {
        notes.push(format!(
            "nothing recorded in the last {}d",
            stats.window_days
        ));
    } else {
        notes.push(format!(
            "true retention {} over scheduled reviews · practice {}",
            rate(stats.retention),
            rate(stats.practice)
        ));
        if stats.aided.total > 0 {
            // A count over the window, and nothing more. Whether the vault and
            // the verifier disagree *now* is a different question with a
            // different answer — a refusal followed by a pass clears it — and
            // it belongs to doctor, which reads the state rather than the
            // history. Stating it from a historical count is how the two came
            // to contradict each other.
            let refused = stats.aided.total.saturating_sub(stats.aided.passes);
            notes.push(format!(
                "{} aided, in no figure above{}",
                stats.aided.total,
                if refused > 0 {
                    format!(" · {refused} refused")
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
    if !lineage {
        notes.push(format!(
            "streak counts unaided passes at {}d or longer, back from the last drill",
            ladder.cap_days()
        ));
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
            "lapse {} on {} at {}d against a scheduled {}d{}",
            lapse.label,
            lapse.day,
            lapse.effective,
            lapse.scheduled,
            if lapse.beyond_schedule() {
                " · beyond schedule"
            } else {
                ""
            }
        ));
    }
    notes
}

fn stats_json(stats: &Stats, lineage: bool) -> serde_json::Value {
    let mut document = serde_json::json!({
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
        "lapses": stats.lapses.iter().map(|lapse| serde_json::json!({
            "engram": lapse.label,
            "day": lapse.day.to_string(),
            "scheduled_interval_days": lapse.scheduled,
            "effective_interval_days": lapse.effective,
            "beyond_schedule": lapse.beyond_schedule(),
        })).collect::<Vec<_>>(),
    });
    let listed = if lineage {
        serde_json::json!(
            stats
                .lineages
                .iter()
                .map(|row| serde_json::json!({
                    "lineage": row.slug.as_str(),
                    "engrams": row.engrams,
                    "drills": row.drills,
                    "aided": row.aided,
                    "lapses": row.lapses,
                    "on_time": row.punctuality.on_time,
                    "reviews": row.punctuality.total,
                }))
                .collect::<Vec<_>>()
        )
    } else {
        serde_json::json!(
            stats
                .engrams
                .iter()
                .map(|row| serde_json::json!({
                    "lineage": row.slug.as_str(),
                    "engram": row.engram.to_string(),
                    "label": row.label,
                    "passes": row.retention.passes,
                    "total": row.retention.total,
                    "aided": row.aided,
                    "streak": row.streak,
                    "ttfk_ms": row.ttfk_ms,
                    "total_ms": row.total_ms,
                    "trend": trend_text(row.trend),
                }))
                .collect::<Vec<_>>()
        )
    };
    if let Some(object) = document.as_object_mut() {
        object.insert(
            if lineage { "lineages" } else { "engrams" }.to_owned(),
            listed,
        );
    }
    document
}

fn trend_text(trend: Trend) -> String {
    match trend {
        Trend::Unknown => "not yet".to_owned(),
        Trend::Steady => "steady".to_owned(),
        Trend::Rising(percent) => format!("slower by {percent}%"),
        Trend::Falling(percent) => format!("faster by {percent}%"),
    }
}

fn rate(retention: Retention) -> String {
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
                .is_none_or(|slug| record.event.slug() == slug),
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

fn engram_label(corpus: &Corpus, event: &Event) -> String {
    match event {
        Event::Enroll(e) => corpus.label(&e.engram),
        Event::Rotate(e) => corpus.label(&e.to),
        Event::Attach(e) => corpus.label(&e.engram),
        Event::Capture(e) => corpus.label(&e.engram),
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
        Event::Rotate(rotated) => format!(
            "supersedes {}{}",
            corpus.label(&rotated.from),
            if rotated.proved {
                ""
            } else {
                " · the old one was not proved"
            }
        ),
        Event::Attach(_) => "a verifier was made here · nothing was checked".to_owned(),
        Event::Retire(_) => "off the schedule".to_owned(),
        Event::Capture(capture) => capture_text(capture),
    }
}

fn capture_text(capture: &Captured) -> String {
    let outcome = match capture.outcome {
        crate::corpus::record::Outcome::Pass => "pass",
        crate::corpus::record::Outcome::Fail => "fail",
        crate::corpus::record::Outcome::Blank => "blank",
        crate::corpus::record::Outcome::Skip => "skip",
        crate::corpus::record::Outcome::Abort => "abandoned",
    };
    let what = if capture.aided {
        "aided".to_owned()
    } else {
        capture.occasion.word().to_owned()
    };
    let mut text = format!("{what} · {outcome} · try {}", capture.ordinal);
    // A lookup has no interval worth naming: whatever elapsed, the answer was on
    // the screen.
    if !capture.aided {
        let _ = std::fmt::Write::write_fmt(
            &mut text,
            format_args!(" · {}d cold", capture.effective_interval_days),
        );
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
