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

impl Class {
    const fn index(self) -> usize {
        match self {
            Class::Prose => 0,
            Class::Code => 1,
            Class::Ignored => 2,
        }
    }
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
            Some(prose as f64 / base as f64)
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
    let coverage = cover(spans, src.len(), default);

    let mut totals = [0u64; 3];
    let mut line = [0u64; 3];
    let mut counts = Counts::default();
    let mut cursor = 0usize;

    let flush_line = |line: &mut [u64; 3], counts: &mut Counts| {
        // Ties go to code, the convention line-based counters use for a line
        // that carries both. A blank line reaches no class at all.
        let winner = if line[Class::Code.index()] >= line[Class::Prose.index()]
            && line[Class::Code.index()] >= line[Class::Ignored.index()]
        {
            Class::Code
        } else if line[Class::Prose.index()] >= line[Class::Ignored.index()] {
            Class::Prose
        } else {
            Class::Ignored
        };
        if line[winner.index()] > 0 {
            match winner {
                Class::Prose => counts.prose_lines += 1,
                Class::Code => counts.code_lines += 1,
                Class::Ignored => counts.ignored_lines += 1,
            }
        }
        *line = [0u64; 3];
    };

    for (offset, ch) in src.char_indices() {
        if ch == '\n' {
            flush_line(&mut line, &mut counts);
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }
        while cursor < coverage.len() && offset >= coverage[cursor].end {
            cursor += 1;
        }
        let class = coverage
            .get(cursor)
            .filter(|s| offset >= s.start)
            .map_or(default, |s| s.class);
        totals[class.index()] += 1;
        line[class.index()] += 1;
    }
    flush_line(&mut line, &mut counts);

    counts.prose_chars = totals[Class::Prose.index()];
    counts.code_chars = totals[Class::Code.index()];
    counts.ignored_chars = totals[Class::Ignored.index()];
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
