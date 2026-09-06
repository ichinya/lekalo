//! The closed isolation-level vocabulary and its owner-published
//! compatibility relation (issue #24).
//!
//! Compatibility is a normative partial order, never a lexical string
//! comparison. `none < read_committed < repeatable_read < serializable`;
//! `snapshot` has exactly one declared relation (it satisfies itself and
//! is satisfied by `serializable` per the relation table) — it is never
//! assumed equivalent to `repeatable_read`, and `serializable` is
//! stronger only where the table declares it. An undeclared relation is
//! `unknown`, and unknown never passes a strict requirement.

use std::fmt;

/// The closed v1 isolation level.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum IsolationLevel {
    /// No declared guarantee.
    None,
    /// Committed reads only.
    ReadCommitted,
    /// Repeatable reads.
    RepeatableRead,
    /// A consistent snapshot (non-equivalent to repeatable_read by
    /// owner decision; the relation table carries the exact facts).
    Snapshot,
    /// Serializable execution.
    Serializable,
}

impl IsolationLevel {
    /// Every level in the closed registry order.
    pub const LEVELS: [IsolationLevel; 5] = [
        IsolationLevel::None,
        IsolationLevel::ReadCommitted,
        IsolationLevel::RepeatableRead,
        IsolationLevel::Snapshot,
        IsolationLevel::Serializable,
    ];

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ReadCommitted => "read_committed",
            Self::RepeatableRead => "repeatable_read",
            Self::Snapshot => "snapshot",
            Self::Serializable => "serializable",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::LEVELS
            .iter()
            .copied()
            .find(|level| level.key() == key)
    }

    /// The owner-published compatibility answer: `Some(true)` when this
    /// (provided) level is declared to satisfy `required`, `Some(false)`
    /// when declared insufficient, `None` when the relation is
    /// undeclared (unknown — never silently satisfied). `snapshot` has
    /// no declared relation with the committed-read chain in either
    /// direction, so it is never lexically ordered.
    pub fn satisfies(self, required: IsolationLevel) -> Option<bool> {
        match (required, self) {
            (level, provided) if level == provided => Some(true),
            (Self::None, _) => Some(true),
            (Self::ReadCommitted, Self::None) => Some(false),
            (Self::ReadCommitted, Self::Snapshot) => None,
            (Self::ReadCommitted, _) => Some(true),
            (Self::RepeatableRead, Self::ReadCommitted | Self::None) => Some(false),
            (Self::RepeatableRead, Self::Snapshot) => None,
            (Self::RepeatableRead, _) => Some(true),
            (Self::Serializable, Self::Snapshot) => None,
            (Self::Serializable, _) => Some(false),
            (Self::Snapshot, _) => None,
        }
    }
}

impl fmt::Display for IsolationLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_relation_holds() {
        assert_eq!(
            IsolationLevel::ReadCommitted.satisfies(IsolationLevel::None),
            Some(true)
        );
        assert_eq!(
            IsolationLevel::RepeatableRead.satisfies(IsolationLevel::ReadCommitted),
            Some(true)
        );
        assert_eq!(
            IsolationLevel::Serializable.satisfies(IsolationLevel::RepeatableRead),
            Some(true)
        );
        assert_eq!(
            IsolationLevel::None.satisfies(IsolationLevel::ReadCommitted),
            Some(false)
        );
    }

    #[test]
    fn snapshot_is_not_lexically_ordered() {
        // Undeclared relations stay unknown — never true.
        assert_eq!(
            IsolationLevel::Snapshot.satisfies(IsolationLevel::RepeatableRead),
            None
        );
        assert_eq!(
            IsolationLevel::RepeatableRead.satisfies(IsolationLevel::Snapshot),
            None
        );
        assert_eq!(
            IsolationLevel::Snapshot.satisfies(IsolationLevel::ReadCommitted),
            None
        );
        assert_eq!(
            IsolationLevel::Serializable.satisfies(IsolationLevel::Snapshot),
            None
        );
        assert_eq!(
            IsolationLevel::Snapshot.satisfies(IsolationLevel::Snapshot),
            Some(true)
        );
    }
}
