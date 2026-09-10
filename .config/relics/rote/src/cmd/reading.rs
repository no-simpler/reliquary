//! The three tables: the schedule, the measurement, the records.

use anyhow::Result;
use jiff::civil::Date;
use relic_core::ui::Format;

use super::{Context, block, write_cache};
use crate::cli::{LogArgs, MeasurementArgs, ScheduleArgs};
use crate::exit::CLEAN;
use crate::ladder::{self, Class, Gate, Standing};
use crate::log::{Event, Line, Outcome};
use crate::model::{SlugState, State};
use crate::render::{Table, json, seconds};
use crate::stats::{Stats, Trend};
use crate::store::{Journal, Verifiers};

pub fn status(ctx: &Context, args: &ScheduleArgs) -> Result<u8> {
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

pub fn stats(ctx: &Context, args: &MeasurementArgs) -> Result<u8> {
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
            seconds(slug.ttfk_ms),
            seconds(slug.total_ms),
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

pub fn log(ctx: &Context, args: &LogArgs) -> Result<u8> {
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
