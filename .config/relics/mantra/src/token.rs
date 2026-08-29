//! Reading `+token` activations out of a prompt.
//!
//! The grammar is deliberately narrow, and every part of it is load-bearing.
//! A prompt is task text with an occasional line of markers in it, so a marker
//! has to be recognisable without ever claiming a piece of prose:
//!
//! - Only the **leading run** of a line counts. `+terse +slash fix the parser`
//!   activates two modes and leaves `fix the parser` as the request. A `+` in
//!   the middle of a line is arithmetic, a diff, or C++.
//! - A token must be followed by whitespace or the end of the line, so `+1,`
//!   and `+terse.` are prose.
//! - A name starts and ends alphanumeric; `_` and `-` may appear between.
//! - Every line is examined, because a prompt written over several lines puts
//!   its markers wherever the writer paused.
//!
//! Order is preserved and duplicates collapse across the whole prompt: sending
//! `+terse +terse` is one activation, and the repetition was the user asking for
//! exactly what this crate now does on a schedule.

/// The names activated by `prompt`, in the order they appear, each once.
pub fn activations(prompt: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in prompt.lines() {
        for name in leading_run(line) {
            if !found.iter().any(|seen| seen == name) {
                found.push(name.to_owned());
            }
        }
    }
    found
}

/// The tokens at the start of one line, stopping at the first thing that is not
/// one. A token that fails mid-run ends the run without discarding the tokens
/// before it.
fn leading_run(line: &str) -> Vec<&str> {
    let mut rest = line.trim_start_matches([' ', '\t']);
    let mut run = Vec::new();
    while let Some(after_plus) = rest.strip_prefix('+') {
        let width = name_width(after_plus);
        if width == 0 {
            break;
        }
        let (name, tail) = after_plus.split_at(width);
        if tail.is_empty() {
            run.push(name);
            break;
        }
        if !tail.starts_with([' ', '\t']) {
            // The delimiter is what separates a marker from prose, so a token
            // that lacks one is prose and so is the rest of the line.
            break;
        }
        run.push(name);
        rest = tail.trim_start_matches([' ', '\t']);
    }
    run
}

/// How much of `text` is a mode name: the longest prefix of name characters
/// that still ends alphanumeric. Zero when `text` does not open with one, which
/// is also what rejects `++`.
fn name_width(text: &str) -> usize {
    if !text.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return 0;
    }
    let mut width = 0;
    let mut last_alphanumeric = 0;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            width += c.len_utf8();
            last_alphanumeric = width;
        } else if c == '_' || c == '-' {
            width += c.len_utf8();
        } else {
            break;
        }
    }
    last_alphanumeric
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(prompt: &str) -> Vec<String> {
        activations(prompt)
    }

    #[test]
    fn a_leading_run_activates_in_order() {
        assert_eq!(names("+terse +slash fix the parser"), ["terse", "slash"]);
    }

    #[test]
    fn duplicates_collapse_across_the_whole_prompt() {
        assert_eq!(names("+terse +terse\n+terse +robust"), ["terse", "robust"]);
    }

    #[test]
    fn a_token_after_prose_is_prose() {
        assert!(names("fix +terse").is_empty());
        assert!(names("a + b").is_empty());
    }

    #[test]
    fn arithmetic_and_cpp_are_not_tokens() {
        assert!(names("++i").is_empty());
        assert!(names("+ 1").is_empty());
        assert!(names("+= 1").is_empty());
    }

    #[test]
    fn a_digit_does_open_a_name() {
        // The grammar admits it; nothing in the corpus uses it. Pinned so a
        // rewrite of `name_width` cannot quietly change the answer, and as the
        // reason a bare `+5` on its own line is not prose to this parser.
        assert_eq!(names("+5 apples"), ["5"]);
    }

    #[test]
    fn punctuation_after_a_name_ends_the_run() {
        assert!(names("+terse.").is_empty());
        assert!(names("+terse, +slash").is_empty());
    }

    #[test]
    fn a_trailing_separator_is_not_part_of_a_name() {
        // The longest name ending alphanumeric is `a`, and what follows it is
        // not a delimiter, so the token is prose.
        assert!(names("+a- rest").is_empty());
        assert_eq!(names("+a-b rest"), ["a-b"]);
    }

    #[test]
    fn a_failed_token_keeps_the_ones_before_it() {
        assert_eq!(names("+terse +bad. +slash"), ["terse"]);
    }

    #[test]
    fn indentation_does_not_hide_a_run() {
        assert_eq!(names("   \t+terse"), ["terse"]);
    }

    #[test]
    fn a_later_line_carries_its_own_run() {
        assert_eq!(names("do the thing\n\n+terse +robust"), ["terse", "robust"]);
    }

    #[test]
    fn underscores_and_hyphens_live_inside_a_name() {
        assert_eq!(names("+a_b-c2 x"), ["a_b-c2"]);
    }

    proptest::proptest! {
        /// Whatever the prompt, a name that comes back is one the resolver can
        /// turn into a file name: no separators, no dots, no spaces.
        #[test]
        fn every_name_is_a_bare_file_stem(prompt in "(?s).{0,200}") {
            for name in activations(&prompt) {
                proptest::prop_assert!(!name.is_empty());
                proptest::prop_assert!(
                    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                    "{name:?}"
                );
                proptest::prop_assert!(name.starts_with(|c: char| c.is_ascii_alphanumeric()));
                proptest::prop_assert!(name.ends_with(|c: char| c.is_ascii_alphanumeric()));
            }
        }
    }
}
