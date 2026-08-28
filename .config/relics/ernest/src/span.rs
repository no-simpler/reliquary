//! Byte-span classification and measurement.
//!
//! An analyzer names the spans it recognises; everything it does not cover
//! falls to the profile's default class. Measurement then walks the file once,
//! attributing every non-whitespace character to the class that owns its byte.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    Prose,
    Code,
    Ignored,
}

/// A tally per class, reached by `Class` rather than by number: the enum owns
/// which fields exist, so a lookup is total and adding a class is a compiler
/// error everywhere it matters rather than an off-by-one.
#[derive(Clone, Copy, Debug, Default)]
struct ByClass {
    prose: u64,
    code: u64,
    ignored: u64,
}

impl ByClass {
    const fn get(self, class: Class) -> u64 {
        match class {
            Class::Prose => self.prose,
            Class::Code => self.code,
            Class::Ignored => self.ignored,
        }
    }

    const fn bump(&mut self, class: Class) {
        match class {
            Class::Prose => self.prose += 1,
            Class::Code => self.code += 1,
            Class::Ignored => self.ignored += 1,
        }
    }
}

/// A count in the unit `Counts` uses. Saturating rather than fallible: a `usize`
/// that does not fit a `u64` cannot exist on a target this runs on, and an error
/// arm for it would be unreachable in every caller.
#[must_use]
pub fn tally(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// A count as a signed delta operand. Saturating for the same reason.
#[must_use]
pub fn delta(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A count as a float. `f64` is exact below 2^53, which no character count in a
/// repository approaches — the one place that conversion is written.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "counts stay far below 2^53"
)]
pub fn approx(n: u64) -> f64 {
    n as f64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub class: Class,
}

impl Span {
    pub fn new(start: usize, end: usize, class: Class) -> Self {
        Span { start, end, class }
    }
}

/// Per-class totals in both units.
///
/// Characters are the canonical unit and count only non-whitespace. Lines are
/// secondary: a line is attributed whole to whichever class owns the most
/// non-whitespace characters on it, ties going to code — so a line is counted
/// at most once and blank lines are counted nowhere.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub prose_chars: u64,
    pub code_chars: u64,
    pub ignored_chars: u64,
    pub prose_lines: u64,
    pub code_lines: u64,
    pub ignored_lines: u64,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.prose_chars += other.prose_chars;
        self.code_chars += other.code_chars;
        self.ignored_chars += other.ignored_chars;
        self.prose_lines += other.prose_lines;
        self.code_lines += other.code_lines;
        self.ignored_lines += other.ignored_lines;
    }

    /// Prose share of the prose-plus-code base, as a ratio in `0.0..=1.0`.
    /// `None` when nothing countable was found, which is not the same as zero.
    pub fn density(&self, unit: Unit) -> Option<f64> {
        let (prose, code) = match unit {
            Unit::Chars => (self.prose_chars, self.code_chars),
            Unit::Lines => (self.prose_lines, self.code_lines),
        };
        let base = prose + code;
        if base == 0 {
            None
        } else {
            Some(approx(prose) / approx(base))
        }
    }

    pub fn prose(&self, unit: Unit) -> u64 {
        match unit {
            Unit::Chars => self.prose_chars,
            Unit::Lines => self.prose_lines,
        }
    }

    pub fn code(&self, unit: Unit) -> u64 {
        match unit {
            Unit::Chars => self.code_chars,
            Unit::Lines => self.code_lines,
        }
    }

    pub fn ignored(&self, unit: Unit) -> u64 {
        match unit {
            Unit::Chars => self.ignored_chars,
            Unit::Lines => self.ignored_lines,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Chars,
    Lines,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Unit::Chars => "chars",
            Unit::Lines => "lines",
        }
    }
}

/// Fill the gaps between recognised spans with `default`, yielding complete,
/// ordered, non-overlapping coverage of `len` bytes.
fn cover(spans: &[Span], len: usize, default: Class) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0usize;
    for s in spans {
        if s.start > at {
            out.push(Span::new(at, s.start, default));
        }
        if s.end > s.start {
            out.push(*s);
        }
        at = at.max(s.end);
    }
    if at < len {
        out.push(Span::new(at, len, default));
    }
    out
}

/// Attribute every non-whitespace character of `src` to a class, then roll the
/// result up into both units.
///
/// `spans` must be ordered and non-overlapping; `analyze` guarantees that.
pub fn measure(src: &str, spans: &[Span], default: Class) -> Counts {
    measure_range(src, spans, default, 0, src.len())
}

/// `measure`, restricted to `from..to`. Spans keep their absolute offsets, so a
/// section is measured against the same classification as the whole file.
pub fn measure_range(src: &str, spans: &[Span], default: Class, from: usize, to: usize) -> Counts {
    let coverage = cover(spans, src.len(), default);

    let mut totals = ByClass::default();
    let mut line = ByClass::default();
    let mut counts = Counts::default();
    let mut cursor = 0usize;

    let flush_line = |line: &mut ByClass, counts: &mut Counts| {
        // Ties go to code, the convention line-based counters use for a line
        // that carries both. A blank line reaches no class at all.
        let winner = if line.get(Class::Code) >= line.get(Class::Prose)
            && line.get(Class::Code) >= line.get(Class::Ignored)
        {
            Class::Code
        } else if line.get(Class::Prose) >= line.get(Class::Ignored) {
            Class::Prose
        } else {
            Class::Ignored
        };
        if line.get(winner) > 0 {
            match winner {
                Class::Prose => counts.prose_lines += 1,
                Class::Code => counts.code_lines += 1,
                Class::Ignored => counts.ignored_lines += 1,
            }
        }
        *line = ByClass::default();
    };

    let window = src.get(from..to).unwrap_or("");
    for (offset, ch) in window.char_indices().map(|(i, c)| (i + from, c)) {
        if ch == '\n' {
            flush_line(&mut line, &mut counts);
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }
        while coverage.get(cursor).is_some_and(|span| offset >= span.end) {
            cursor += 1;
        }
        let class = coverage
            .get(cursor)
            .filter(|s| offset >= s.start)
            .map_or(default, |s| s.class);
        totals.bump(class);
        line.bump(class);
    }
    flush_line(&mut line, &mut counts);

    counts.prose_chars = totals.prose;
    counts.code_chars = totals.code;
    counts.ignored_chars = totals.ignored;
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_is_never_counted() {
        let src = "   \n\t\n  \n";
        let c = measure(src, &[], Class::Code);
        assert_eq!(c, Counts::default());
    }

    #[test]
    fn gaps_take_the_default_class() {
        let src = "ab// c\ndef";
        let spans = [Span::new(2, 6, Class::Prose)];
        let c = measure(src, &spans, Class::Code);
        assert_eq!(c.prose_chars, 3); // "//" and "c"
        assert_eq!(c.code_chars, 5); // "ab" and "def"
    }

    #[test]
    fn a_mixed_line_splits_by_char_but_resolves_whole_by_line() {
        let src = "x=1; // note";
        let spans = [Span::new(5, 12, Class::Prose)];
        let c = measure(src, &spans, Class::Code);
        assert_eq!(c.code_chars, 4);
        assert_eq!(c.prose_chars, 6); // "//" + "note"
        assert_eq!(c.prose_lines, 1);
        assert_eq!(c.code_lines, 0);
    }

    #[test]
    fn density_is_none_without_a_countable_base() {
        let c = Counts {
            ignored_chars: 12,
            ..Default::default()
        };
        assert_eq!(c.density(Unit::Chars), None);
    }
}
