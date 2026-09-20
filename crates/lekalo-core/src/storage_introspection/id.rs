//! Typed identifiers of the storage-introspection evidence (issue
//! #117).
//!
//! The test schema name is the one bound identifier of the evidence
//! document: one lowercase bounded name under the declared lifecycle
//! prefix grammar. Hosts, ports, users, and data source names are
//! inexpressible — this is the only location surface the family has.

use crate::scenario::id::IdError;

/// A validated lowercase test schema name (`lekalo_test_planner`). One
/// bounded snake-case segment, never quoted, dotted, or path-shaped.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SchemaName(String);

impl SchemaName {
    /// Validate and keep the exact schema name.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 64 {
            return Err(IdError::Length);
        }
        let bytes = text.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(IdError::Shape);
        }
        if !bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
        {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_names_reject_paths_hosts_and_mixed_case() {
        assert!(SchemaName::parse("lekalo_test_planner").is_ok());
        assert!(SchemaName::parse("Schema").is_err());
        assert!(SchemaName::parse("../escape").is_err());
        assert!(SchemaName::parse("db.internal.example:3306").is_err());
        assert!(SchemaName::parse("").is_err());
        assert!(SchemaName::parse(&"a".repeat(65)).is_err());
    }
}
