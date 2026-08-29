//! Where everything is, resolved once.
//!
//! Injected rather than reached for, so a test states a machine instead of
//! building one — and so the whole surface can be pointed at a scratch `HOME`
//! without any of it consulting the environment a second time.

use camino::Utf8PathBuf;

/// Every root this relic reads or writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// The home directory everything else hangs off.
    pub home: Utf8PathBuf,
    /// The public relic lane, and a cargo workspace root.
    pub public: Utf8PathBuf,
    /// The private relic lane, and a second cargo workspace root.
    pub private: Utf8PathBuf,
    /// Stage-1 one-shot utilities, which `scaffold` promotes from.
    pub bin: Utf8PathBuf,
    /// The skeleton a new relic is copied from.
    pub template: Utf8PathBuf,
    /// The lifecycle reference, which also carries the external-relic list.
    pub graduation: Utf8PathBuf,
    /// The externally-managed `PATH` lane.
    pub local_bin: Utf8PathBuf,
    /// The publish helper — a sourced shell ABI two external repositories also
    /// call, and therefore not something this binary reimplements.
    pub install_on_path: Utf8PathBuf,
}

/// Why the roots could not be resolved.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// There is no home directory to hang them off.
    #[error("cannot determine the home directory")]
    NoHome,
}

impl Paths {
    /// Resolve every root against `home`.
    #[must_use]
    pub fn under(home: Utf8PathBuf) -> Self {
        Self {
            public: home.join(".config/relics"),
            private: home.join(".config/attic"),
            bin: home.join(".config/bin"),
            template: home.join(".config/reliquary/template"),
            graduation: home.join(".config/reliquary/GRADUATION.md"),
            local_bin: home.join(".local/bin"),
            install_on_path: home.join(".config/reliquary/lib/install-on-path.sh"),
            home,
        }
    }

    /// Resolve them against this process's home directory.
    ///
    /// # Errors
    ///
    /// [`Error::NoHome`] when there is none.
    pub fn from_env() -> Result<Self, Error> {
        relic_core::path::home()
            .map(Self::under)
            .ok_or(Error::NoHome)
    }

    /// The shared `PATH` registry.
    #[must_use]
    pub fn registry(&self) -> Utf8PathBuf {
        self.local_bin.join(".reliquary-managed")
    }
}
