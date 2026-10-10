//! Closed issue #41 managed wire documents.
//!
//! Every optional reference has an explicit state; unknown members, key
//! duplicates, and unsorted collections are refused before any semantic
//! decision is made.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const VERSION: &str = "0.8.0";
/// The negotiated managed target-protocol successor token.
pub const PROTOCOL: &str = "0.8.0";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
pub enum State<T> {
    Known { value: T },
    Absent,
}

impl<T> State<T> {
    pub fn known(value: T) -> Self {
        Self::Known { value }
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Known { value } => Some(value),
            Self::Absent => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Observed,
    Contracted,
    Managed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequirementKind {
    Assertion,
    Command,
    Effect,
    Entity,
    Event,
    Expression,
    Invariant,
    Job,
    Policy,
    Port,
    Query,
    Scenario,
    Transaction,
}

impl RequirementKind {
    /// Whether the kind carries a rendered implementation and test.
    pub const fn renders(self) -> bool {
        matches!(self, Self::Command | Self::Query)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    Full,
    Partial,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanAction {
    Promotion,
    Regeneration,
    Upgrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputAction {
    Create,
    Replace,
    Delete,
    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactRole {
    Custody,
    Implementation,
    Manifest,
    Map,
    Test,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionState {
    Prepared,
    Publishing,
    Committed,
    Restoring,
    Recovered,
    Abandoned,
}

impl TransactionState {
    /// Whether this state still owns project bytes and blocks mutation.
    pub const fn unresolved(self) -> bool {
        matches!(self, Self::Prepared | Self::Publishing | Self::Restoring)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionAction {
    Apply,
    Rollback,
    Resolve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingOp {
    Publish,
    Delete,
    Restore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioOutcome {
    Match,
    Mismatch,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EquivalenceVerdict {
    Match,
    Mismatch,
    Incomplete,
}

/// One scalar sample value; ordering follows the JSON scalar classes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Flag(bool),
    Count(f64),
    Text(String),
    Nothing,
}

// `Eq` is derived through the token domain only where needed; samples are
// compared by their canonical JSON, so plain `PartialEq` is exact here.

macro_rules! dto {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

dto!(Reference {
    id: String,
    version: String,
    digest: String
});
// The generation-authority pins of one coverage document: the canonical
// Model, the canonical typed IR, and the exact lock revision the evidence
// was derived under. A coverage document without its authority is not
// evidence.
dto!(AuthorityPin {
    model: String,
    ir: String,
    lock: String
});
dto!(Extension {
    symbol: String,
    binding: String,
    contract_digest: String
});
dto!(ExtensionCheck {
    symbol: String,
    binding: String,
    contract_digest: String,
    validated: bool
});
dto!(Projection {
    from: String,
    to: String
});
dto!(Sample { input: BTreeMap<String, Scalar>, output: BTreeMap<String, Scalar> });
dto!(Adapter {
    entry: String,
    package_digest: String,
    protocol: String
});
dto!(InputPin {
    path: String,
    digest: String
});
dto!(GenerationDigest {
    generation_digest: String
});
dto!(Conflict {
    path: String,
    reason: String
});

dto!(TargetAssignment {
    target: String,
    profile_ref: Reference,
    implementation_ref: Reference,
    generated_root: String,
    extensions: Vec<Extension>
});
dto!(ModuleAssignment { module: String, mode: Mode, targets: Vec<TargetAssignment> });
dto!(Ownership { schema_version: String, identity: String, project: String, modules: Vec<ModuleAssignment> });

/// Requirement: the optional `detail` note is skipped when absent so the
/// canonical serialization matches the raw closed document byte for byte.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub kind: RequirementKind,
    pub support: Support,
    pub witness: State<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub projection: Vec<Projection>,
    pub sample: State<Sample>,
}
dto!(Coverage {
    schema_version: String,
    identity: String,
    module: String,
    target: String,
    recipe: Reference,
    adapter: Adapter,
    authority: State<AuthorityPin>,
    requirements: Vec<Requirement>,
    extensions: Vec<ExtensionCheck>,
    complete: bool
});

dto!(PlanOutput {
    path: String,
    action: OutputAction,
    role: ArtifactRole,
    owner_module: String,
    owner_target: String,
    before_digest: State<String>,
    after_digest: State<String>
});
dto!(GenerationPlan {
    schema_version: String,
    identity: String,
    action: PlanAction,
    modules: Vec<String>,
    targets: Vec<String>,
    input_closure: Vec<InputPin>,
    before_manifest: State<GenerationDigest>,
    coverage_digest: String,
    extensions: Vec<Extension>,
    outputs: Vec<PlanOutput>,
    plan_digest: String
});

/// `ref` is a reserved word; the wire spelling stays `ref`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreOp {
    pub path: String,
    pub digest: String,
    #[serde(rename = "ref")]
    pub object_ref: String,
}

dto!(RecordOutput {
    path: String,
    role: ArtifactRole,
    owner_module: String,
    owner_target: String,
    digest: String,
    before_ref: State<String>,
    after_ref: State<String>
});
dto!(GenerationRecord {
    schema_version: String,
    identity: String,
    plan_digest: String,
    generation_digest: String,
    previous_generation: State<GenerationDigest>,
    input_closure: Vec<InputPin>,
    outputs: Vec<RecordOutput>,
    ownership_digest: String,
    lock_pin: State<String>,
    record_digest: String
});

dto!(DeleteOp {
    path: String,
    digest: String
});
dto!(RollbackPlan {
    schema_version: String,
    identity: String,
    current_generation: State<GenerationDigest>,
    destination_generation_digest: String,
    restore: Vec<RestoreOp>,
    delete: Vec<DeleteOp>,
    manifest_digest: String,
    plan_digest: String
});

dto!(Pending {
    path: String,
    op: PendingOp
});
// `beforeDigest` is the exact drifted before-state a resolve preview
// pinned; the confirmation digest covers it, so a file that moved after
// the reviewed preview invalidates the confirmation.
dto!(Transaction {
    schema_version: String,
    identity: String,
    transaction_id: String,
    state: TransactionState,
    action: TransactionAction,
    plan_digest: State<String>,
    generation_digest: State<GenerationDigest>,
    before_manifest_digest: State<String>,
    after_manifest_digest: State<String>,
    pending: Vec<Pending>,
    affected_files: Vec<String>,
    conflict: State<Conflict>,
    before_digest: State<String>
});

dto!(ScenarioVerdict {
    scenario: String,
    digest: String,
    verdict: ScenarioOutcome
});
// One executed-outcome receipt: the digest over the canonical JSON of
// the observed output the scenario's retained executable test produced
// for its pinned sample. Unlike the caller-authored `verdict` labels,
// this pin is independently recomputed by the gate from the retained
// reviewed coverage (the deterministic handler is the pinned projection
// over the pinned sample), so a receipt that disagrees with the
// recomputed executed behavior can never certify `match` (r4 finding 2).
dto!(ScenarioExecution {
    scenario: String,
    observed: String
});
dto!(Equivalence {
    schema_version: String,
    identity: String,
    module: String,
    target: String,
    implementation_digest: String,
    reference_digest: State<String>,
    scenarios: Vec<ScenarioVerdict>,
    execution: Vec<ScenarioExecution>,
    gaps: Vec<String>,
    verdict: EquivalenceVerdict
});
