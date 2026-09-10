//! Exit codes. One place, so a sitting cut short by a signal and one abandoned
//! at the keyboard leave on the same number.

/// Everything went as it should, or there was nothing to do.
pub const CLEAN: u8 = 0;
/// A first-attempt failure, or a soft finding.
pub const LAPSE: u8 = 1;
/// The sitting was abandoned, or something is broken.
pub const INCOMPLETE: u8 = 2;
/// rote could not run at all, which is different from finding something wrong.
pub const REFUSED: u8 = 3;
