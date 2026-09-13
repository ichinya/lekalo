//! The closed query declaration surface of the query-model attachment
//! (issue #64).
//!
//! One declaration binds one `query` definition of the pinned Model to
//! its portable read semantics: the source entity, the result
//! cardinality, the declared parameters, the closed filter/sort/
//! pagination contract, the selected projection, the semantic-link
//! includes, the consistency profile, the read-cost hints, the policy
//! and scenario references, and the foreign escape hatch. Every
//! surface is closed and bounded; nothing here can express a write,
//! raw target SQL, or target-specific text.

use crate::scenario::id::{FieldName, SemanticId};

use super::filter::FilterExpr;
use super::id::ParameterName;
use super::version::{
    MAX_INCLUDES, MAX_INCLUDE_PATH, MAX_POLICY_REFS, MAX_SCENARIO_REFS, MAX_SELECTION,
    MAX_SORT_KEYS,
};

/// The closed result-cardinality vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Cardinality {
    /// Exactly one result row.
    One,
    /// Zero or one result row.
    Optional,
    /// Zero or more result rows, unbounded.
    List,
    /// One bounded page of a larger result.
    Page,
    /// A potentially unbounded ordered stream.
    Stream,
}

impl Cardinality {
    /// The exact wire text of the cardinality.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::Optional => "optional",
            Self::List => "list",
            Self::Page => "page",
            Self::Stream => "stream",
        }
    }

    /// The cardinality for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "one" => Self::One,
            "optional" => Self::Optional,
            "list" => Self::List,
            "page" => Self::Page,
            "stream" => Self::Stream,
            _ => return None,
        })
    }

    /// Whether the cardinality requires a declared total order.
    pub const fn requires_total_order(self) -> bool {
        matches!(self, Self::Page | Self::Stream)
    }

    /// Whether the cardinality admits a pagination block.
    pub const fn admits_pagination(self) -> bool {
        matches!(self, Self::Page | Self::Stream)
    }
}

/// The closed consistency/freshness profile vocabulary. A declared
/// profile is a requirement on the executing target, never a
/// guarantee by declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Consistency {
    /// The strongest read the target owns; every read sees the
    /// latest committed state.
    Strong,
    /// Reads may lag within the declared staleness bound.
    Bounded,
    /// Reads may serve arbitrarily stale state.
    StaleOk,
}

impl Consistency {
    /// The exact wire text of the profile.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Bounded => "bounded",
            Self::StaleOk => "stale-ok",
        }
    }

    /// The profile for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "strong" => Self::Strong,
            "bounded" => Self::Bounded,
            "stale-ok" => Self::StaleOk,
            _ => return None,
        })
    }
}

/// One declared query parameter with its model type expression.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Parameter {
    /// The closed parameter name.
    pub name: ParameterName,
    /// The model type expression (`planner.due_date`,
    /// `list<planner.task_state>`).
    pub parameter_type: String,
}

/// One sort key with its direction.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SortKey {
    /// The source-entity field name.
    pub field: FieldName,
    /// The sort direction.
    pub direction: Direction,
}

/// The closed sort-direction vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Direction {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

impl Direction {
    /// The exact wire text of the direction.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    /// The direction for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "asc" => Self::Asc,
            "desc" => Self::Desc,
            _ => return None,
        })
    }
}

/// The closed pagination contract. One shape for every target: the
/// Node and PHP (and any other) projections consume this exact wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pagination {
    /// The pagination strategy.
    pub strategy: PaginationStrategy,
    /// The inclusive maximum page size.
    pub limit: i64,
    /// The offset-strategy row offset (`offset` only).
    pub offset: Option<i64>,
    /// The cursor parameter carrying the continuation key (`cursor`
    /// only); the parameter must be declared.
    pub key_parameter: Option<ParameterName>,
}

/// The closed pagination-strategy vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PaginationStrategy {
    /// Limit/offset windowing.
    Offset,
    /// Keyset continuation over the declared sort.
    Cursor,
}

impl PaginationStrategy {
    /// The exact wire text of the strategy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offset => "offset",
            Self::Cursor => "cursor",
        }
    }

    /// The strategy for one wire token, or nothing.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "offset" => Self::Offset,
            "cursor" => Self::Cursor,
            _ => return None,
        })
    }
}

/// One selected output field of the projection.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SelectionField {
    /// The source-entity field name.
    pub field: FieldName,
    /// The output name; defaults to the field name.
    pub alias: Option<FieldName>,
}

/// One semantic-link include: a bounded path of relation fields from
/// the source entity to a joined entity.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Include {
    /// The ordered relation-field path (at least one segment).
    pub path: Vec<FieldName>,
    /// The output name of the joined value; defaults to the last
    /// path segment.
    pub alias: Option<FieldName>,
}

/// The bounded read-cost hints. Hints are planner requirements and
/// review evidence — never an execution guarantee by declaration.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CostHint {
    /// The inclusive expected maximum row count.
    pub max_rows: Option<i64>,
    /// Whether the read is declared expensive for planners.
    pub expensive: Option<bool>,
}

/// The foreign/custom escape hatch (ADR-0033): the query's mapping is
/// delegated to a foreign implementation and managed generation is
/// blocked for it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ForeignRef {
    /// The closed capability token naming the delegated concern
    /// (`full-text-search`, `window-rank`).
    pub capability: String,
    /// The bounded reason the mapping is foreign.
    pub reason: String,
}

/// One finished query declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryDecl {
    /// The bound `query` definition of the pinned Model.
    pub query: SemanticId,
    /// The source entity of the read.
    pub source: SemanticId,
    /// The result cardinality.
    pub cardinality: Cardinality,
    /// The consistency/freshness profile.
    pub consistency: Consistency,
    /// The declared staleness bound in seconds (`bounded` only).
    pub max_staleness: Option<i64>,
    /// The declared input parameters.
    pub parameters: Vec<Parameter>,
    /// The closed filter expression, when declared.
    pub filter: Option<FilterExpr>,
    /// The ordered sort keys, when declared.
    pub sort: Option<Vec<SortKey>>,
    /// The pagination block, when declared.
    pub pagination: Option<Pagination>,
    /// The selected projection fields; `None` selects all source
    /// fields.
    pub selection: Option<Vec<SelectionField>>,
    /// The semantic-link includes, when declared.
    pub includes: Vec<Include>,
    /// The authorization policy references.
    pub policies: Vec<SemanticId>,
    /// The scenario references that pin the mapping behavior.
    pub scenarios: Vec<SemanticId>,
    /// The read-cost hints, when declared.
    pub cost: Option<CostHint>,
    /// The foreign escape hatch, when declared.
    pub foreign: Option<ForeignRef>,
}

impl QueryDecl {
    /// The canonical sort key for attachment ordering: the query id.
    pub(crate) fn sort_key(&self) -> &str {
        self.query.as_str()
    }

    /// Whether every collection member count stays inside the closed
    /// bounds (typed-level check; the wire decoder enforces the same
    /// limits incrementally).
    pub(crate) fn within_bounds(&self) -> bool {
        if self.parameters.len() > super::version::MAX_PARAMETERS
            || self.includes.len() > MAX_INCLUDES
            || self.policies.len() > MAX_POLICY_REFS
            || self.scenarios.len() > MAX_SCENARIO_REFS
        {
            return false;
        }
        if let Some(sort) = &self.sort {
            if sort.len() > MAX_SORT_KEYS {
                return false;
            }
        }
        if let Some(selection) = &self.selection {
            if selection.len() > MAX_SELECTION {
                return false;
            }
        }
        for include in &self.includes {
            if include.path.is_empty() || include.path.len() > MAX_INCLUDE_PATH {
                return false;
            }
        }
        if let Some(filter) = &self.filter {
            if !filter.within_bounds() {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinality_wire_round_trip() {
        for card in [
            Cardinality::One,
            Cardinality::Optional,
            Cardinality::List,
            Cardinality::Page,
            Cardinality::Stream,
        ] {
            assert_eq!(Cardinality::parse(card.as_str()), Some(card));
        }
        assert_eq!(Cardinality::parse("many"), None);
        assert!(Cardinality::Page.requires_total_order());
        assert!(Cardinality::Stream.requires_total_order());
        assert!(!Cardinality::List.requires_total_order());
        assert!(Cardinality::Page.admits_pagination());
        assert!(Cardinality::Stream.admits_pagination());
        assert!(!Cardinality::List.admits_pagination());
        assert!(!Cardinality::One.admits_pagination());
    }

    #[test]
    fn consistency_and_direction_wire_round_trip() {
        for profile in [
            Consistency::Strong,
            Consistency::Bounded,
            Consistency::StaleOk,
        ] {
            assert_eq!(Consistency::parse(profile.as_str()), Some(profile));
        }
        assert_eq!(Consistency::parse("eventual"), None);
        for direction in [Direction::Asc, Direction::Desc] {
            assert_eq!(Direction::parse(direction.as_str()), Some(direction));
        }
        assert_eq!(Direction::parse("up"), None);
        for strategy in [PaginationStrategy::Offset, PaginationStrategy::Cursor] {
            assert_eq!(PaginationStrategy::parse(strategy.as_str()), Some(strategy));
        }
        assert_eq!(PaginationStrategy::parse("keyset"), None);
    }
}
