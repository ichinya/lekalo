//! The bounded affected-seed set (issue #18).
//!
//! A seed is one changed semantic subject with the change records that
//! touch it and the side it changed on. Seeds are direct diff facts with
//! `direct-diff` origin: this module never materializes reverse or
//! transitive dependents (#13 owns graph traversal, #16 owns impact
//! radius).

use super::identity::Subject;

/// One affected-symbol seed.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct AffectedSeed {
    pub(crate) subject: Subject,
    pub(crate) side: super::change::Side,
    pub(crate) change_ids: Vec<String>,
}

impl AffectedSeed {
    /// The stable semantic subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The comparison side.
    pub const fn side(&self) -> super::change::Side {
        self.side
    }

    /// The change identities, in canonical order.
    pub fn change_ids(&self) -> &[String] {
        &self.change_ids
    }

    /// The seed origin: always `direct-diff` in v1.
    pub const fn origin(&self) -> &'static str {
        "direct-diff"
    }
}
