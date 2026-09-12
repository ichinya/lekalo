//! The in-memory entity state of one reference evaluation (issue
//! #107).
//!
//! The store is plain deterministic data: entity id -> row key ->
//! field name -> closed typed value, held in byte-sorted maps so
//! iteration order never depends on insertion order. Row keys are the
//! canonical JSON of the entity's declared identity fields with their
//! values, which makes row addressing, ordering, and serialization
//! identical on every host. The store never touches the filesystem,
//! the network, or the wall clock, and every mutation flows through
//! the executor's staged transaction discipline.

use std::collections::BTreeMap;

use crate::scenario::value::TypedValue;

/// One entity row: field name -> typed value, byte-sorted.
pub(crate) type Row = BTreeMap<String, TypedValue>;

/// The whole in-memory state of one evaluation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Store {
    /// entity id -> row key -> row fields.
    rows: BTreeMap<String, BTreeMap<String, Row>>,
}

impl Store {
    /// The rows of one entity, ordered by row key.
    pub(crate) fn rows_of(&self, entity: &str) -> Vec<(&String, &Row)> {
        self.rows
            .get(entity)
            .map(|rows| rows.iter().collect())
            .unwrap_or_default()
    }

    /// One row by exact key.
    pub(crate) fn row(&self, entity: &str, key: &str) -> Option<&Row> {
        self.rows.get(entity).and_then(|rows| rows.get(key))
    }

    /// The total number of rows across every entity.
    pub(crate) fn row_count(&self) -> usize {
        self.rows.values().map(BTreeMap::len).sum()
    }

    /// Insert or replace one row under its exact key.
    pub(crate) fn put_row(&mut self, entity: &str, key: String, row: Row) {
        self.rows
            .entry(entity.to_owned())
            .or_default()
            .insert(key, row);
    }

    /// Merge established fields into the row under `key`: existing
    /// fields are overwritten by the (already conflict-checked)
    /// established values.
    pub(crate) fn merge_row(&mut self, entity: &str, key: String, fields: Row) {
        self.rows
            .entry(entity.to_owned())
            .or_default()
            .entry(key)
            .or_default()
            .extend(fields);
    }

    /// Every entity that holds at least one row, byte-sorted.
    pub(crate) fn entities(&self) -> Vec<&String> {
        self.rows.keys().collect()
    }

    /// The canonical row key of one entity's identity fields: the
    /// compact JSON array of `[field, value]` pairs byte-sorted by
    /// field name.
    pub(crate) fn row_key(identity: &[String], fields: &Row) -> Option<String> {
        let mut pairs: Vec<(&String, &TypedValue)> = Vec::new();
        for field in identity {
            let value = fields.get(field)?;
            pairs.push((field, value));
        }
        pairs.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
        let members: Vec<String> = pairs
            .iter()
            .map(|(field, value)| {
                format!(
                    "[{},{}]",
                    super::canonical::string(field),
                    super::canonical::typed_value(value)
                )
            })
            .collect();
        Some(format!("[{}]", members.join(",")))
    }
}
