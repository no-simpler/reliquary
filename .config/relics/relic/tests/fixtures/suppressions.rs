// Fixture text for the lint ratchet's own tests.
//
// It lives here because `fixtures/` is excluded from the count, which is the
// rule's whole point: a fixture is test data, and lints written into one are
// what it is for. Inline in the source, this crate — the one crate that talks
// about suppressions — would carry eighteen of them and its baseline would stop
// meaning "suppressions".
//
// Five openers below, one of them in a comment. A count that reasoned about
// comments would be a parser, and this file is the assertion that it is not.

#[allow(dead_code)]
#![allow(clippy::all)]
#[expect(unused, reason = "x")]
#![expect(unused)]
// #[allow( in a comment still opens one, and that is honest.

// Neither of these is a suppression.
#[derive(Debug)]
#[must_use]
