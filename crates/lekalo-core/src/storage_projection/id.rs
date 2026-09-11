//! Typed identifiers of the storage-projection attachment (issue
//! #65).
//!
//! Every wire string becomes one closed typed value before it can
//! exist in an attachment. Record references reuse the accepted #6
//! semantic grammar ([`SemanticId`]) and the accepted namespaced
//! grammar ([`NamespacedId`]); the entity key and the storage name are
//! new here, and both are closed, bounded, and refuse anything
//! path-shaped, URL-shaped, credential-shaped, or runtime-shaped
//! before it can serialize.

use std::fmt;

use crate::scenario::id::IdError;

/// A validated stable entity key inside one attachment (`task`,
/// `task_external_link`). The key is independent of the Model symbol
/// id and of every storage table name: a rename of either never moves
/// it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EntityKey(String);

impl EntityKey {
    /// Validate and keep the exact key text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 64 {
            return Err(IdError::Length);
        }
        if !snake_segment(text) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated lowercase storage identifier for a table or column
/// (`task`, `tenant_id`). Never quoted, dotted, or path-shaped.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StorageName(String);

impl StorageName {
    /// Validate and keep the exact storage name.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 63 {
            return Err(IdError::Length);
        }
        if !snake_segment(text) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StorageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One closed lowercase snake-case segment: `[a-z][a-z0-9_]*` with at
/// most 63 bytes (the callers own their specific length bounds).
pub(crate) fn snake_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_keys_reject_paths_urls_and_mixed_case() {
        assert!(EntityKey::parse("task_external_link").is_ok());
        assert!(EntityKey::parse("Task").is_err());
        assert!(EntityKey::parse("../escape").is_err());
        assert!(EntityKey::parse("https://evil.example").is_err());
        assert!(EntityKey::parse("").is_err());
        assert!(EntityKey::parse(&"a".repeat(65)).is_err());
    }

    #[test]
    fn storage_names_reject_quoted_and_dotted_text() {
        assert!(StorageName::parse("tenant_id").is_ok());
        assert!(StorageName::parse("tenant.id").is_err());
        assert!(StorageName::parse("\"tenant\"").is_err());
        assert!(StorageName::parse("SELECT").is_err());
        assert!(StorageName::parse("").is_err());
        assert!(StorageName::parse(&"a".repeat(64)).is_err());
    }
}
