//! The typed NFR constraint model (issue #85).
//!
//! Every member of every constraint is one closed typed value: the
//! kind vocabulary is partitioned by dimension and can never mix the
//! runtime and ai-budget rows; the requirement carries the declared
//! bound exactly as authored (comparator, value or range, unit, and
//! the kind-specific members), with no defaults and no universal
//! thresholds anywhere in the core; the measurement names the method
//! and the gate the evidence must come from. A constraint is a claim —
//! satisfaction is decided by the report engine against evidence, and
//! nothing in this module can assert it.

use std::fmt;

use crate::scenario::id::NamespacedId;

use super::environment::{Environment, OwnerRef, Token};
use super::id::{ConstraintId, Decimal, IsoDate};

/// Why one textual value is not a legal closed-vocabulary member.
/// Reuses the scenario id error taxonomy.
pub type ValueError = crate::scenario::id::IdError;

macro_rules! closed_enum {
    ($(#[$meta:meta])* $name:ident, $as_str:ident, $parse:ident, { $($variant:ident => $key:literal),+ $(,)? }) => {
        /// A closed vocabulary value.
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// The closed vocabulary in canonical order.
            pub const KEYS: [Self; count_variants!($($variant),+)] = [$(Self::$variant),+];

            /// The exact wire spelling.
            pub const fn $as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }

            /// Parse the exact wire spelling.
            pub fn $parse(text: &str) -> Option<Self> {
                match text {
                    $($key => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.$as_str())
            }
        }
    };
}

macro_rules! count_variants {
    () => { 0usize };
    ($first:ident $(, $rest:ident)* $(,)?) => { 1usize + count_variants!($($rest),*) };
}

closed_enum! {
    /// The closed dimension vocabulary (issue #85 AC#7): runtime NFRs
    /// and AI-generation budgets are separate dimensions and can never
    /// share a row.
    Dimension, as_str, parse, {
        Runtime => "runtime",
        AiBudget => "ai-budget",
    }
}

closed_enum! {
    /// The closed runtime-kind vocabulary.
    RuntimeKind, as_str, parse, {
        Latency => "latency",
        Throughput => "throughput",
        Concurrency => "concurrency",
        Availability => "availability",
        Durability => "durability",
        Timeout => "timeout",
        RetryBudget => "retry-budget",
        ResourceLimit => "resource-limit",
        Retention => "retention",
        Consistency => "consistency",
        Freshness => "freshness",
        Rpo => "rpo",
        Rto => "rto",
        Deployment => "deployment",
        RuntimeConstraint => "runtime-constraint",
        AccessibilityRef => "accessibility-ref",
        SecurityRef => "security-ref",
        PrivacyRef => "privacy-ref",
    }
}

closed_enum! {
    /// The closed ai-budget kind vocabulary.
    AiBudgetKind, as_str, parse, {
        AiCost => "ai-cost",
        AiToken => "ai-token",
        AiCompute => "ai-compute",
    }
}

closed_enum! {
    /// The closed scope-kind vocabulary.
    ScopeKind, as_str, parse, {
        Project => "project",
        Module => "module",
        Operation => "operation",
        Endpoint => "endpoint",
    }
}

closed_enum! {
    /// The closed bound comparator vocabulary.
    Comparator, as_str, parse, {
        Lt => "lt",
        Lte => "lte",
        Eq => "eq",
        Gte => "gte",
        Gt => "gt",
        Within => "within",
        Outside => "outside",
    }
}

closed_enum! {
    /// The closed percentile vocabulary; legal for latency only.
    Percentile, as_str, parse, {
        P50 => "p50",
        P95 => "p95",
        P99 => "p99",
    }
}

closed_enum! {
    /// The closed unit vocabulary. Legal spellings per kind are
    /// coherence-checked at validation; the core never invents one.
    Unit, as_str, parse, {
        Milliseconds => "milliseconds",
        Seconds => "seconds",
        Minutes => "minutes",
        Hours => "hours",
        Days => "days",
        Months => "months",
        Years => "years",
        Percent => "percent",
        Nines => "nines",
        Count => "count",
        RequestsPerSecond => "requests-per-second",
        OperationsPerSecond => "operations-per-second",
        Cores => "cores",
        Megabytes => "megabytes",
        Gigabytes => "gigabytes",
        Terabytes => "terabytes",
        Tokens => "tokens",
        Currency => "currency",
    }
}

closed_enum! {
    /// The closed resource vocabulary of resource limits.
    Resource, as_str, parse, {
        Cpu => "cpu",
        Memory => "memory",
        Storage => "storage",
        Connections => "connections",
        Threads => "threads",
    }
}

closed_enum! {
    /// The closed mode vocabulary shared by consistency (strong,
    /// bounded, stale-ok) and deployment (platform, topology, region).
    Mode, as_str, parse, {
        Strong => "strong",
        Bounded => "bounded",
        StaleOk => "stale-ok",
        Platform => "platform",
        Topology => "topology",
        Region => "region",
    }
}

closed_enum! {
    /// The closed window unit of retry budgets.
    WindowUnit, as_str, parse, {
        Seconds => "seconds",
        Minutes => "minutes",
        Hours => "hours",
    }
}

closed_enum! {
    /// The closed enforcement vocabulary (issue #85 AC#5).
    Enforcement, as_str, parse, {
        Mandatory => "mandatory",
        Advisory => "advisory",
    }
}

closed_enum! {
    /// The closed measurement-method vocabulary. `declaration` is the
    /// no-measurement spelling: it is legal only under advisory
    /// enforcement and reports unverified, never satisfied.
    Method, as_str, parse, {
        Benchmark => "benchmark",
        LoadTest => "load-test",
        SoakTest => "soak-test",
        SloReport => "slo-report",
        ChaosExperiment => "chaos-experiment",
        SyntheticProbe => "synthetic-probe",
        AuditDocument => "audit-document",
        Declaration => "declaration",
    }
}

closed_enum! {
    /// The closed target-profile support vocabulary.
    Support, as_str, parse, {
        Partial => "partial",
        Full => "full",
    }
}

impl Support {
    /// Whether `actual` satisfies a requirement of `minimum` strength.
    pub const fn satisfies(minimum: Self, actual: Self) -> bool {
        (minimum as u8) <= (actual as u8)
    }
}

/// The closed kind vocabulary, partitioned by dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Kind {
    /// A runtime-dimension kind.
    Runtime(RuntimeKind),
    /// An ai-budget-dimension kind.
    AiBudget(AiBudgetKind),
}

impl Kind {
    /// The exact wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Runtime(kind) => kind.as_str(),
            Self::AiBudget(kind) => kind.as_str(),
        }
    }

    /// Parse the wire spelling into the kind that owns it.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(kind) = RuntimeKind::parse(text) {
            return Some(Self::Runtime(kind));
        }
        AiBudgetKind::parse(text).map(Self::AiBudget)
    }

    /// The dimension this kind belongs to. The partition is total:
    /// every kind has exactly one dimension.
    pub const fn dimension(self) -> Dimension {
        match self {
            Self::Runtime(_) => Dimension::Runtime,
            Self::AiBudget(_) => Dimension::AiBudget,
        }
    }

    /// Whether the kind carries a numeric bound (metric, comparator,
    /// value/range, unit).
    pub const fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Runtime(
                RuntimeKind::Latency
                    | RuntimeKind::Throughput
                    | RuntimeKind::Concurrency
                    | RuntimeKind::Availability
                    | RuntimeKind::Durability
                    | RuntimeKind::Timeout
                    | RuntimeKind::ResourceLimit
                    | RuntimeKind::Retention
                    | RuntimeKind::Freshness
                    | RuntimeKind::Rpo
                    | RuntimeKind::Rto
            ) | Self::AiBudget(_)
        )
    }

    /// Whether the kind is an owner-held reference (no numeric bound).
    pub const fn is_reference(self) -> bool {
        matches!(
            self,
            Self::Runtime(
                RuntimeKind::AccessibilityRef | RuntimeKind::SecurityRef | RuntimeKind::PrivacyRef
            )
        )
    }

    /// Whether the measurement of this kind must name a gate.
    pub const fn requires_gate_ref(self) -> bool {
        self.is_numeric() || matches!(self, Self::Runtime(RuntimeKind::RetryBudget))
    }
}

/// The closed scope of one constraint: a semantic surface the bound
/// applies to. The reference resolves against the bound IR at
/// resolution time; wire validation only checks the grammar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scope {
    kind: ScopeKind,
    reference: String,
}

impl Scope {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(kind: ScopeKind, reference: String) -> Self {
        Self { kind, reference }
    }

    /// The scope kind.
    pub const fn kind(&self) -> ScopeKind {
        self.kind
    }

    /// The semantic reference.
    pub fn reference(&self) -> &str {
        &self.reference
    }
}

/// The declared requirement: the bound exactly as authored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Requirement {
    metric: Option<Token>,
    percentile: Option<Percentile>,
    comparator: Option<Comparator>,
    value: Option<RequirementValue>,
    unit: Option<Unit>,
    resource: Option<Resource>,
    mode: Option<Mode>,
    tokens: Vec<Token>,
    max_attempts: Option<Decimal>,
    window: Option<(Decimal, WindowUnit)>,
    reference: Option<OwnerRef>,
}

/// The scalar bound: one value or an inclusive range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequirementValue {
    /// One canonical decimal.
    Scalar(Decimal),
    /// An inclusive range for `within`/`outside`.
    Range { min: Decimal, max: Decimal },
}

impl Requirement {
    /// Assemble from validated parts (wire internal).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        metric: Option<Token>,
        percentile: Option<Percentile>,
        comparator: Option<Comparator>,
        value: Option<RequirementValue>,
        unit: Option<Unit>,
        resource: Option<Resource>,
        mode: Option<Mode>,
        tokens: Vec<Token>,
        max_attempts: Option<Decimal>,
        window: Option<(Decimal, WindowUnit)>,
        reference: Option<OwnerRef>,
    ) -> Self {
        Self {
            metric,
            percentile,
            comparator,
            value,
            unit,
            resource,
            mode,
            tokens,
            max_attempts,
            window,
            reference,
        }
    }

    /// The declared metric name, when the kind carries one.
    pub fn metric(&self) -> Option<&str> {
        self.metric.as_ref().map(Token::as_str)
    }

    /// The declared percentile, when the kind carries one.
    pub const fn percentile(&self) -> Option<Percentile> {
        self.percentile
    }

    /// The declared comparator, when the kind carries one.
    pub const fn comparator(&self) -> Option<Comparator> {
        self.comparator
    }

    /// The declared scalar or range bound.
    pub const fn value(&self) -> Option<&RequirementValue> {
        self.value.as_ref()
    }

    /// The declared unit.
    pub const fn unit(&self) -> Option<Unit> {
        self.unit
    }

    /// The declared resource, for resource limits.
    pub const fn resource(&self) -> Option<Resource> {
        self.resource
    }

    /// The declared mode, for consistency and deployment.
    pub const fn mode(&self) -> Option<Mode> {
        self.mode
    }

    /// The declared token set, for deployment and runtime constraints.
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// The declared retry bound (attempt count and window).
    pub const fn max_attempts(&self) -> Option<&Decimal> {
        self.max_attempts.as_ref()
    }

    /// The declared retry window and its unit.
    pub const fn window(&self) -> Option<&(Decimal, WindowUnit)> {
        self.window.as_ref()
    }

    /// The owner-held standard/policy reference, for reference kinds.
    pub const fn reference(&self) -> Option<&OwnerRef> {
        self.reference.as_ref()
    }
}

/// The declared measurement custody: the method and the gate or
/// evidence kinds the measured result must come from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Measurement {
    method: Method,
    gate_ref: Option<NamespacedId>,
    evidence_kinds: Vec<Method>,
}

impl Measurement {
    /// Assemble from validated parts (wire internal). An empty
    /// evidence-kinds list means "this method"; anything else must
    /// contain the method.
    pub(crate) fn assemble(
        method: Method,
        gate_ref: Option<NamespacedId>,
        evidence_kinds: Vec<Method>,
    ) -> Self {
        let evidence_kinds = if evidence_kinds.is_empty() {
            vec![method]
        } else {
            evidence_kinds
        };
        Self {
            method,
            gate_ref,
            evidence_kinds,
        }
    }

    /// The declared method.
    pub const fn method(&self) -> Method {
        self.method
    }

    /// The declared gate reference, required for measurable kinds.
    pub const fn gate_ref(&self) -> Option<&NamespacedId> {
        self.gate_ref.as_ref()
    }

    /// The declared evidence kinds, always containing the method.
    pub fn evidence_kinds(&self) -> &[Method] {
        &self.evidence_kinds
    }
}

/// The validity window and revision of one constraint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Validity {
    revision: String,
    valid_from: Option<IsoDate>,
    valid_until: Option<IsoDate>,
}

impl Validity {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(
        revision: String,
        valid_from: Option<IsoDate>,
        valid_until: Option<IsoDate>,
    ) -> Self {
        Self {
            revision,
            valid_from,
            valid_until,
        }
    }

    /// The exact constraint revision evidence must pin.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// The inclusive validity start.
    pub const fn valid_from(&self) -> Option<&IsoDate> {
        self.valid_from.as_ref()
    }

    /// The inclusive validity end.
    pub const fn valid_until(&self) -> Option<&IsoDate> {
        self.valid_until.as_ref()
    }
}

/// One required target-profile capability: the dotted capability id
/// plus the minimum support the resolved profile must provide.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CapabilityRequirement {
    id: String,
    minimum: Support,
}

impl CapabilityRequirement {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(id: String, minimum: Support) -> Self {
        Self { id, minimum }
    }

    /// The dotted capability id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The minimum support.
    pub const fn minimum(&self) -> Support {
        self.minimum
    }
}

/// One finished constraint: immutable, closed-vocabulary, safe to
/// share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constraint {
    constraint_id: ConstraintId,
    kind: Kind,
    scope: Scope,
    requirement: Requirement,
    enforcement: Enforcement,
    measurement: Measurement,
    environments: Vec<Environment>,
    capabilities: Vec<CapabilityRequirement>,
    validity: Validity,
    source_requirement: Option<(String, String)>,
}

impl Constraint {
    /// Assemble from validated parts (wire internal).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        constraint_id: ConstraintId,
        kind: Kind,
        scope: Scope,
        requirement: Requirement,
        enforcement: Enforcement,
        measurement: Measurement,
        environments: Vec<Environment>,
        capabilities: Vec<CapabilityRequirement>,
        validity: Validity,
        source_requirement: Option<(String, String)>,
    ) -> Self {
        Self {
            constraint_id,
            kind,
            scope,
            requirement,
            enforcement,
            measurement,
            environments,
            capabilities,
            validity,
            source_requirement,
        }
    }

    /// The stable constraint id.
    pub const fn constraint_id(&self) -> &ConstraintId {
        &self.constraint_id
    }

    /// The dimension this constraint lives in.
    pub const fn dimension(&self) -> Dimension {
        self.kind.dimension()
    }

    /// The closed kind.
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// The scope.
    pub const fn scope(&self) -> &Scope {
        &self.scope
    }

    /// The declared requirement.
    pub const fn requirement(&self) -> &Requirement {
        &self.requirement
    }

    /// The enforcement.
    pub const fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    /// The declared measurement custody.
    pub const fn measurement(&self) -> &Measurement {
        &self.measurement
    }

    /// The accepted environments, canonically ordered. An empty list
    /// accepts any single environment — still never merged.
    pub fn environments(&self) -> &[Environment] {
        &self.environments
    }

    /// The required capabilities.
    pub fn capabilities(&self) -> &[CapabilityRequirement] {
        &self.capabilities
    }

    /// The validity window.
    pub const fn validity(&self) -> &Validity {
        &self.validity
    }

    /// The source-requirement link `(source, requirement)`.
    pub const fn source_requirement(&self) -> Option<&(String, String)> {
        self.source_requirement.as_ref()
    }
}

/// One registered open question: the bounded id of an unverifiable
/// prose statement the owner holds outside the contract (text-free by
/// design), optionally tied to one constraint.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OpenQuestion {
    question_id: ConstraintId,
    related_constraint: Option<ConstraintId>,
}

impl OpenQuestion {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(
        question_id: ConstraintId,
        related_constraint: Option<ConstraintId>,
    ) -> Self {
        Self {
            question_id,
            related_constraint,
        }
    }

    /// The stable question id.
    pub const fn question_id(&self) -> &ConstraintId {
        &self.question_id
    }

    /// The related constraint, when one exists.
    pub const fn related_constraint(&self) -> Option<&ConstraintId> {
        self.related_constraint.as_ref()
    }
}
