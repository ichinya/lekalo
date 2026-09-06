//! Pessimistic lock requirement vocabulary (issue #24).
//!
//! Lock scopes, modes, acquisition points, and timeout policies are
//! closed vocabularies independent of every target framework: resources,
//! keys, and ranges are typed semantic selectors, never SQL, raw paths,
//! or target lock names.

use std::fmt;

/// The bounded selector scope of one lock.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LockScope {
    /// The whole resource entity.
    Entity,
    /// One typed key of the resource.
    Key,
    /// One bounded typed key range.
    Range,
}

impl LockScope {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Key => "key",
            Self::Range => "range",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "entity" => Some(Self::Entity),
            "key" => Some(Self::Key),
            "range" => Some(Self::Range),
            _ => None,
        }
    }
}

impl fmt::Display for LockScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// The lock mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LockMode {
    /// Concurrent readers, exclusive writers.
    Shared,
    /// Exclusive access.
    Exclusive,
}

impl LockMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Exclusive => "exclusive",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "shared" => Some(Self::Shared),
            "exclusive" => Some(Self::Exclusive),
            _ => None,
        }
    }
}

impl fmt::Display for LockMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// When the lock must be held.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LockAcquisition {
    /// Before the first read inside the boundary.
    BeforeRead,
    /// Before the first write inside the boundary.
    BeforeWrite,
}

impl LockAcquisition {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::BeforeRead => "before_read",
            Self::BeforeWrite => "before_write",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "before_read" => Some(Self::BeforeRead),
            "before_write" => Some(Self::BeforeWrite),
            _ => None,
        }
    }
}

impl fmt::Display for LockAcquisition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// The declared behavior when the lock cannot be acquired.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LockTimeout {
    /// Fail immediately with the typed conflict outcome.
    FailFast,
    /// Block, but never past a bounded deadline.
    BlockingWithDeadline,
}

impl LockTimeout {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::FailFast => "fail_fast",
            Self::BlockingWithDeadline => "blocking_with_deadline",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "fail_fast" => Some(Self::FailFast),
            "blocking_with_deadline" => Some(Self::BlockingWithDeadline),
            _ => None,
        }
    }
}

impl fmt::Display for LockTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_vocabularies_round_trip() {
        for (key, scope) in [
            ("entity", LockScope::Entity),
            ("key", LockScope::Key),
            ("range", LockScope::Range),
        ] {
            assert_eq!(LockScope::from_key(key), Some(scope));
            assert_eq!(scope.key(), key);
        }
        assert_eq!(LockMode::from_key("exclusive"), Some(LockMode::Exclusive));
        assert_eq!(LockMode::from_key("cas"), None);
        assert_eq!(
            LockAcquisition::from_key("before_write"),
            Some(LockAcquisition::BeforeWrite)
        );
        assert_eq!(
            LockTimeout::from_key("blocking_with_deadline"),
            Some(LockTimeout::BlockingWithDeadline)
        );
    }
}
