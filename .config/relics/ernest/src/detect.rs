//! Path to profile.

use std::path::Path;

use crate::analyze::profiles::{PROFILES, Profile};

/// The profile for `path`, or `None` when the format is unsupported.
/// Exact filenames win over extensions, so a name-only format can override a
/// misleading suffix later.
pub fn profile_for(path: &Path) -> Option<&'static Profile> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && let Some(profile) = PROFILES
            .iter()
            .find(|p| p.filenames.iter().any(|f| f.eq_ignore_ascii_case(name)))
    {
        return Some(profile);
    }

    let ext = path.extension().and_then(|e| e.to_str())?;
    PROFILES
        .iter()
        .find(|p| p.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_supported_extensions() {
        assert_eq!(profile_for(Path::new("a/b.php")).unwrap().language, "php");
        assert_eq!(profile_for(Path::new("a/b.PHP")).unwrap().language, "php");
        assert_eq!(profile_for(Path::new("c.yml")).unwrap().language, "yaml");
        assert_eq!(profile_for(Path::new("c.yaml")).unwrap().language, "yaml");
    }

    #[test]
    fn declines_everything_else() {
        assert!(profile_for(Path::new("a.rs")).is_none());
        assert!(profile_for(Path::new("Makefile")).is_none());
        assert!(profile_for(Path::new("noext")).is_none());
    }
}
