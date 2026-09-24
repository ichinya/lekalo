//! Deterministic PostgreSQL identifier quoting (issue #69).
//!
//! Emitted SQL always quotes identifiers as `"name"`: the storage-name
//! grammar (`^[a-z][a-z0-9_]*$`, at most 63 bytes) never escapes
//! itself, so quoting is unconditional, deterministic, and safe
//! against reserved words. Names are emitted verbatim lowercase —
//! never folded, never guessed.

use crate::storage_projection::id::StorageName;

/// Quote one storage identifier for emitted SQL.
pub fn quote(name: &StorageName) -> String {
    format!("\"{}\"", name.as_str())
}

/// Quote a dotted qualified name (`"table"."column"`).
pub fn quote_qualified(table: &StorageName, column: &StorageName) -> String {
    format!("{}.{}", quote(table), quote(column))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_is_always_present_and_verbatim() {
        let name = StorageName::parse("task").expect("name");
        assert_eq!(quote(&name), "\"task\"");
        let reserved = StorageName::parse("select").expect("name");
        assert_eq!(quote(&reserved), "\"select\"");
        let column = StorageName::parse("tenant_id").expect("name");
        assert_eq!(quote_qualified(&name, &column), "\"task\".\"tenant_id\"");
    }
}
