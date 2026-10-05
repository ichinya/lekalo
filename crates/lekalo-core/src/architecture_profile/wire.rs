//! Closed issue #84 documents. Every unavailable value has an explicit state.
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "0.6.5";
pub const RECIPE: &str = "architecture-core/1";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
pub enum State<T> {
    Known { value: T },
    Unknown,
    Unsupported,
    Withheld,
}
impl<T> State<T> {
    pub fn known(value: T) -> Self {
        Self::Known { value }
    }
    pub fn value(&self) -> Option<&T> {
        if let Self::Known { value } = self {
            Some(value)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Coverage {
    Complete,
    Partial,
    Unsupported,
    Disabled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

macro_rules! dto {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
dto!(Reference {
    id: String,
    version: String,
    digest: String
});
dto!(Rule {
    id: String,
    revision: u64,
    owner: String,
    producer: String,
    measurement: String,
    coverage: Coverage,
    mandatory: bool,
    blocking_basis: String,
    severity: Severity,
    rationale: String,
    alternative: String
});
dto!(Catalog { schema_version: String, identity: String, recipe: String,
    registry_ref: Reference, rules: Vec<Rule> });
dto!(Limit {
    maximum: u64,
    calibration_ref: String
});
dto!(Selection { id: String, enabled: bool, severity: Severity, required: bool, limit: State<Limit> });
dto!(Profile { id: String, version: String, extends: State<Reference>, rules: Vec<Selection> });
dto!(Scope {
    kind: String,
    id: String
});
dto!(Assignment {
    scope: Scope,
    profile_ref: Reference
});
dto!(Document { schema_version: String, identity: String, catalog_ref: Reference,
    profiles: Vec<Profile>, assignments: Vec<Assignment> });
dto!(Resolved { schema_version: String, identity: String, profile_ref: Reference,
    source_digest: String, catalog_ref: Reference, chain: Vec<Reference>, rules: Vec<Selection> });
dto!(Snapshot { schema_version: String, identity: String, document_digest: String,
    catalog_ref: Reference, profiles: Vec<Resolved>, assignments: Vec<Assignment> });
dto!(Row { rule_id: String, module: String, producer: String, measurement: String,
    coverage: Coverage, value: State<u64>, witnesses: Vec<String>, severity: Severity,
    required: bool, limit: State<Limit>, disposition: String, condition_digest: String,
    rationale: String, alternative: String });
dto!(Report { schema_version: String, identity: String, recipe: String, scope: Scope,
    document_digest: String, model_ref: String, ir_ref: String, snapshot_ref: String,
    profiles: Vec<Resolved>, rows: Vec<Row>, baseline_ref: State<String>, assessment: String });
dto!(Change {
    scope: String,
    rule_id: String,
    member: String,
    strength: String,
    before: String,
    after: String
});
dto!(Diff { schema_version: String, identity: String, base_ref: String, candidate_ref: String,
    comparable: bool, changes: Vec<Change> });
dto!(Debt {
    rule_id: String,
    module: String,
    condition_digest: String,
    owner: String,
    reason: String,
    review_ref: String,
    expires_on: String
});
dto!(Adoption { schema_version: String, identity: String, snapshot_ref: String,
    baseline_ref: String, entries: Vec<Debt> });
