//! First-class capability and policy decisions plus the deterministic
//! delivery/outbox simulation core (issue #52).
//!
//! Every outbound action passes an explicit per-link decision built from
//! independent capability (`supported|partial|unsupported|unknown`) and
//! authorization (`allowed|denied`) observations; only an `allowed`
//! outcome may produce an outbox intent. The outbox engine is a pure
//! deterministic state machine over typed state: leased dispatch uses a
//! fencing token honored to its deadline, retries keep their idempotency
//! key, echoes never spawn fresh intents, foreign-scope and
//! detached-link events never mutate a link, stale link generations
//! cancel their intents at every consume point, and
//! acceptance-before-acknowledgement surfaces as an explicit durable
//! `unknown_delivery` observation that stays un-re-sendable until an
//! explicit reconciliation resolves it.
//! A deterministic injected clock (`now_ms`) owns all timing; there is
//! no host I/O, thread, scheduler, or network access anywhere in this
//! module.

use std::collections::BTreeMap;

/// The closed decision outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DecisionOutcome {
    /// Both observations permit the exact action/field set.
    Allowed,
    /// A known denial: a domain outcome, never a diagnostic.
    Denied,
    /// The capability does not support the action (or is explicitly
    /// unsupported); unknown support is not permission.
    Unsupported,
    /// Undecidable required input (missing revision-bound observation).
    Indeterminate,
}

impl DecisionOutcome {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Unsupported => "unsupported",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// The closed capability result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilityOutcome {
    /// The exact action/field set is supported.
    Supported,
    /// An explicit permitted subset is supported.
    Partial,
    /// Not supported, with an explicit reason.
    Unsupported,
    /// Support cannot be proven from revision-bound facts.
    Unknown,
}

impl CapabilityOutcome {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

/// The closed authorization result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AuthorizationOutcome {
    /// The actor/scope/field policy allows the action.
    Allowed,
    /// The policy denies the action.
    Denied,
}

impl AuthorizationOutcome {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }
}

/// The closed link lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LinkState {
    /// The link is active.
    Active,
    /// The link is paused.
    Paused,
    /// The link has an unresolved conflict.
    Conflicted,
    /// The link is detached (identity reservation and history remain).
    Detached,
}

impl LinkState {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Conflicted => "conflicted",
            Self::Detached => "detached",
        }
    }
}

/// The closed sync mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SyncMode {
    /// No synchronization.
    Disabled,
    /// Inbound only.
    Inbound,
    /// Outbound only.
    Outbound,
    /// Both directions.
    Bidirectional,
}

impl SyncMode {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
            Self::Bidirectional => "bidirectional",
        }
    }
}

/// The independent inputs of one decision.
#[derive(Clone, Debug)]
pub struct DecisionInput {
    /// The bound action symbol.
    pub action: String,
    /// The requested field mask (sorted, deduplicated).
    pub fields: Vec<String>,
    /// The capability observation.
    pub capability: CapabilityOutcome,
    /// The authorization observation.
    pub authorization: AuthorizationOutcome,
    /// The current link state.
    pub link_state: LinkState,
    /// The link sync mode.
    pub sync_mode: SyncMode,
    /// The explicit permitted subset of a `partial` capability (sorted,
    /// deduplicated, never empty when present). `Supported` capabilities
    /// ignore it; `Partial` without it is undecidable.
    pub permitted_subset: Option<Vec<String>>,
}

/// Compose one typed decision from independent observations.
///
/// Capability and authorization must both permit the exact action and
/// field set. Partial support is allowed only with an explicit,
/// non-empty permitted subset that covers the requested mask (the
/// caller narrows `fields` to it); unknown support is never permission,
/// and a detached link refuses everything.
pub fn decide(input: &DecisionInput) -> DecisionOutcome {
    if input.link_state != LinkState::Active {
        return DecisionOutcome::Denied;
    }
    if matches!(input.sync_mode, SyncMode::Disabled | SyncMode::Inbound) {
        return DecisionOutcome::Denied;
    }
    if input.fields.is_empty() {
        return DecisionOutcome::Indeterminate;
    }
    if input.capability == CapabilityOutcome::Partial {
        // The doc contract: partial stays allowed only with a non-empty
        // permitted subset that actually covers the requested mask.
        let Some(permitted) = &input.permitted_subset else {
            return DecisionOutcome::Indeterminate;
        };
        if permitted.is_empty() || !input.fields.iter().all(|field| permitted.contains(field)) {
            return DecisionOutcome::Unsupported;
        }
    }
    match (input.capability, input.authorization) {
        (CapabilityOutcome::Supported, AuthorizationOutcome::Allowed) => DecisionOutcome::Allowed,
        (CapabilityOutcome::Partial, AuthorizationOutcome::Allowed) => DecisionOutcome::Allowed,
        (CapabilityOutcome::Unsupported, _) => DecisionOutcome::Unsupported,
        (CapabilityOutcome::Unknown, _) => DecisionOutcome::Indeterminate,
        (_, AuthorizationOutcome::Denied) => DecisionOutcome::Denied,
    }
}

/// One outbox intent: the durable unit of external delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intent {
    /// Stable originating intent id.
    pub intent_id: String,
    /// The originating link.
    pub link_id: String,
    /// The link generation the intent was created under.
    pub link_generation: u64,
    /// The bound action.
    pub action: String,
    /// The idempotency key (stable across retries).
    pub idempotency_key: String,
    /// The current outbox state.
    pub state: OutboxState,
    /// Attempts made so far.
    pub attempts: u64,
    /// Next eligible attempt time (injected clock milliseconds).
    pub next_attempt_ms: u64,
    /// The active lease fencing token (leased state only).
    pub lease_token: Option<u64>,
    /// The lease expiry instant (injected clock milliseconds); a claim
    /// after this instant is fenced out and the lease is recoverable.
    pub lease_expires_ms: Option<u64>,
    /// The provider receipt digest after acknowledgement.
    pub receipt_digest: Option<String>,
}

/// The closed outbox state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum OutboxState {
    /// Eligible for claim.
    Pending,
    /// Claimed under a fencing lease.
    Leased,
    /// The provider accepted the write; the local acknowledgement has
    /// not arrived. The durable record must reconcile or report unknown
    /// delivery, never silently re-send and never claim delivered.
    AcceptedPendingAck,
    /// Delivered and acknowledged.
    Delivered,
    /// Waiting for the deterministic backoff.
    RetryWait,
    /// A provider conflict stopped the intent.
    Conflicted,
    /// Cancelled (revoked authority, detached link).
    Cancelled,
    /// Bounded attempts exhausted.
    DeadLetter,
    /// The durable unknown-delivery state: recovery inside the
    /// acceptance-before-acknowledgement window lands here. The state
    /// is persistent and non-dispatchable — no claim and no dispatch
    /// applies — until the explicit reconciliation command resolves it
    /// to delivered or back to pending.
    UnknownDelivery,
}

impl OutboxState {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Leased => "leased",
            Self::AcceptedPendingAck => "accepted_pending_ack",
            Self::Delivered => "delivered",
            Self::RetryWait => "retry_wait",
            Self::Conflicted => "conflicted",
            Self::Cancelled => "cancelled",
            Self::DeadLetter => "dead_letter",
            Self::UnknownDelivery => "unknown_delivery",
        }
    }
}

/// One simulated link with its identity and cursors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    /// Stable link id.
    pub link_id: String,
    /// Owning task id.
    pub task_id: String,
    /// Workspace scope.
    pub workspace_id: String,
    /// Installation scope.
    pub installation_id: String,
    /// Provider scope.
    pub provider_id: String,
    /// Object type.
    pub object_type: String,
    /// External id (exact text comparison).
    pub external_id: String,
    /// Current state.
    pub state: LinkState,
    /// Sync mode.
    pub sync_mode: SyncMode,
    /// Generation (bumped on reattach).
    pub generation: u64,
    /// Inbound cursor (provider revision).
    pub inbound_cursor: Option<String>,
    /// Outbound cursor (provider revision).
    pub outbound_cursor: Option<String>,
}

/// One attributed inbound provider event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundEvent {
    /// Scoped provider event identity (dedup key).
    pub event_identity: String,
    /// Owning workspace.
    pub workspace_id: String,
    /// Originating installation.
    pub installation_id: String,
    /// Originating provider.
    pub provider_id: String,
    /// Provider object type.
    pub object_type: String,
    /// Originating external object.
    pub external_id: String,
    /// Provider revision.
    pub revision: String,
    /// Causation id preserved through the chain.
    pub causation_id: String,
}

/// One outbox transition event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchEvent {
    /// Provider accepted the write (the durable record moves to
    /// `accepted_pending_ack`; it is not delivered until the ack).
    ProviderAccepted,
    /// The provider rejected or timed out (retry-eligible).
    ProviderFailed,
    /// A provider conflict stopped the intent.
    ProviderConflict,
    /// The acknowledgement arrived after acceptance.
    Acknowledged,
    /// Authority was revoked or the link detached before dispatch.
    AuthorityRevoked,
}

/// The outcome of one dispatch step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchStep {
    /// The claim succeeded (state moved to leased under the token).
    Claimed,
    /// The lease was stale (fenced out; nothing changed).
    LeaseStale,
    /// The lease expired by time (the intent becomes recoverable).
    LeaseExpired,
    /// The intent's link generation moved on: the intent is fenced out
    /// and cancelled (a stale generation never dispatches).
    GenerationStale,
    /// The provider accepted; the intent awaits acknowledgement.
    AcceptedPendingAck,
    /// Recovery inside the acceptance-before-ack window: the outcome is
    /// not locally provable and needs reconciliation.
    UnknownDelivery,
    /// The intent was delivered and acknowledged.
    Delivered,
    /// The intent moved to retry_wait.
    RetryScheduled,
    /// The intent moved to dead_letter.
    DeadLettered,
    /// The intent moved to conflicted.
    Conflicted,
    /// The intent was cancelled.
    Cancelled,
    /// A live lease was recovered back to pending (the key is kept).
    Recovered,
    /// The step was a no-op (wrong state, exhausted, not yet eligible).
    Ignored,
}

/// The explicit outcome of one unknown-delivery reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reconciliation {
    /// The provider (or its receipt) proved the write landed.
    Delivered,
    /// The write provably never landed; the intent may dispatch again.
    NotDelivered,
}

/// The declared revision-ordering behavior of one provider contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionOrdering {
    /// The contract declares a comparable total order; forward revisions
    /// apply, equal revisions replay, and only backward revisions are
    /// conflicts.
    ComparableTotal,
    /// Revisions are opaque; only exact equality is decidable and any
    /// non-equal pair is a conflict, never a lexical sort.
    IncomparableDeclared,
}

/// Compare one cursor/revision pair under a declared ordering.
///
/// Returns `true` when the pair is incomparable (a conflict receipt,
/// no cursor movement). Under `ComparableTotal` a forward revision is
/// comparable progress, not a conflict; under `IncomparableDeclared`
/// every non-equal pair is a conflict, never a lexical sort.
pub fn revision_incomparable(ordering: RevisionOrdering, cursor: &str, revision: &str) -> bool {
    match ordering {
        RevisionOrdering::ComparableTotal => {
            // Opaque tokens: the engine proves only sameness. A declared
            // comparable contract resolves forward/equal through its own
            // revision numbers; the engine models that resolution by a
            // monotonic numeric suffix when one exists, and falls back
            // to incomparable when it cannot decide.
            if cursor == revision {
                return false;
            }
            // The suffix convention is only decidable within one
            // revision family: distinct prefixes are unrelated tokens,
            // never a lexical progress ordering.
            match (cursor.split_once('-'), revision.split_once('-')) {
                (Some((left_prefix, left)), Some((right_prefix, right)))
                    if left_prefix == right_prefix =>
                {
                    match (left.parse::<u64>(), right.parse::<u64>()) {
                        (Ok(l), Ok(r)) => r <= l,
                        _ => true,
                    }
                }
                _ => true,
            }
        }
        RevisionOrdering::IncomparableDeclared => cursor != revision,
    }
}

/// The typed scope key of one inbound event: the full
/// workspace/installation/provider/object/external tuple plus the
/// scoped event identity. String concatenation with a separator is not
/// injective, so the map key is the struct itself.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EventScope {
    /// Owning workspace.
    pub workspace_id: String,
    /// Originating installation.
    pub installation_id: String,
    /// Originating provider.
    pub provider_id: String,
    /// Provider object type.
    pub object_type: String,
    /// External object id (exact text).
    pub external_id: String,
    /// Scoped event identity within the scope.
    pub event_identity: String,
}

/// Ingest one inbound event against the current inbox.
///
/// The engine enforces full admission itself: the event's complete
/// workspace/installation/provider/object/external tuple must match
/// the link's own identity (`foreign-scope`), the link must be active
/// (`detached`), and its sync mode must admit inbound traffic
/// (`inbound-disabled`). The inbox is keyed by the typed scope tuple,
/// so events in different workspace or object scopes never alias one
/// another. Refused events reserve no receipt, mutate no cursor, and
/// touch no state. Returns `(applied, receipt)`: a repeated scoped
/// event identity replays the original outcome
/// (`applied == false`) without repeating effects. Non-comparable
/// revisions with the cursor produce the `conflict` receipt and never
/// guess an ordering.
pub fn ingest_event(
    link: &mut Link,
    inbox: &mut BTreeMap<EventScope, String>,
    ordering: RevisionOrdering,
    event: &InboundEvent,
) -> (bool, &'static str) {
    // A detached link observes nothing: history is retained, but no
    // further event mutates the aggregate or its cursors.
    if link.state == LinkState::Detached {
        return (false, "detached");
    }
    // Inbound traffic needs an inbound-admitting sync mode.
    if matches!(link.sync_mode, SyncMode::Disabled | SyncMode::Outbound) {
        return (false, "inbound-disabled");
    }
    // The link's own identity tuple fences foreign-scope events: an
    // event for another workspace/installation/provider/object never
    // applies and never advances this link's cursor.
    if event.workspace_id != link.workspace_id
        || event.installation_id != link.installation_id
        || event.provider_id != link.provider_id
        || event.object_type != link.object_type
        || event.external_id != link.external_id
    {
        return (false, "foreign-scope");
    }
    let scope = EventScope {
        workspace_id: event.workspace_id.clone(),
        installation_id: event.installation_id.clone(),
        provider_id: event.provider_id.clone(),
        object_type: event.object_type.clone(),
        external_id: event.external_id.clone(),
        event_identity: event.event_identity.clone(),
    };
    // A receipt for a different link's scope is not deduplicated here:
    // the caller only passes events whose scope belongs to this link.
    if inbox.contains_key(&scope) {
        return (false, "replayed");
    }
    inbox.insert(scope, format!("{}|{}", event.provider_id, event.revision));
    let conflict = link
        .inbound_cursor
        .as_ref()
        .map(|cursor| revision_incomparable(ordering, cursor, &event.revision))
        .unwrap_or(false);
    if conflict {
        return (true, "conflict");
    }
    link.inbound_cursor = Some(event.revision.clone());
    (true, "applied")
}

/// The outcome of one identity reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reservation {
    /// The tuple was fresh and is now bound.
    Fresh,
    /// The same tuple for the same task replays the original link.
    Replayed,
}

/// The typed external-identity tuple: the full five-part scope. The
/// map key is the struct itself; string concatenation with a separator
/// is not injective and would alias distinct identities.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExternalIdentity {
    /// Workspace scope.
    pub workspace_id: String,
    /// Installation scope.
    pub installation_id: String,
    /// Provider scope.
    pub provider_id: String,
    /// Provider object type.
    pub object_type: String,
    /// External id (exact text).
    pub external_id: String,
}

/// Reserve one external identity across the workspace scope.
///
/// Returns `None` when the exact tuple is already bound to a
/// different task (the `tasklinks.error.identity_already_bound`
/// outcome); a repeated bind of the same tuple to the same task
/// replays the original link (`Some(Reservation::Replayed)`).
pub fn reserve_identity(
    bindings: &mut BTreeMap<ExternalIdentity, String>,
    workspace_id: &str,
    installation_id: &str,
    provider_id: &str,
    object_type: &str,
    external_id: &str,
    task_id: &str,
) -> Option<Reservation> {
    let tuple = ExternalIdentity {
        workspace_id: workspace_id.to_owned(),
        installation_id: installation_id.to_owned(),
        provider_id: provider_id.to_owned(),
        object_type: object_type.to_owned(),
        external_id: external_id.to_owned(),
    };
    match bindings.get(&tuple) {
        Some(existing) if existing != task_id => None,
        Some(_) => Some(Reservation::Replayed),
        None => {
            bindings.insert(tuple, task_id.to_owned());
            Some(Reservation::Fresh)
        }
    }
}

/// One recorded conflict awaiting explicit resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictRecord {
    /// Stable conflict id.
    pub conflict_id: String,
    /// The originating link.
    pub link_id: String,
    /// The base revision the link state was built on.
    pub base_revision: String,
    /// The observed revision that raised the conflict (the current
    /// provider-side version an expected-version precondition must
    /// match).
    pub observed_revision: String,
    /// The lifecycle state.
    pub state: ConflictState,
}

/// The closed conflict lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictState {
    /// Unresolved: writes through the link are blocked.
    Open,
    /// Explicitly resolved under a matching expected-version
    /// precondition; the resolution is audited.
    Resolved,
}

/// One immutable conflict-resolution audit entry. The audit log is
/// append-only by construction: no API rewrites or removes entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionAudit {
    /// The resolved conflict.
    pub conflict_id: String,
    /// The originating link.
    pub link_id: String,
    /// The expected version the resolver presented (the precondition
    /// that matched).
    pub expected_revision: String,
    /// The resolved revision.
    pub resolved_revision: String,
    /// The resolving actor class.
    pub actor: String,
}

/// The outcome of one explicit resolution attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolveOutcome {
    /// The precondition matched and the resolver was authorized; the
    /// conflict closed and the audit entry was appended.
    Resolved,
    /// The expected version did not match the observed revision: the
    /// stale resolver is refused with no state change and no audit.
    Stale,
    /// The resolver carries no current scoped authorization: refused
    /// before the record is even touched.
    Denied,
    /// The conflict id is unknown.
    Unknown,
    /// The conflict was already resolved (a replay of the same
    /// resolution); the audit log is not duplicated.
    Replayed,
}

/// The typed resolution authority of one resolver: the current,
/// scoped authorization decision over the exact link and field scope.
/// A resolver without an explicit allowed decision is never admitted,
/// whatever revision it presents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionAuthority {
    /// The resolving actor class (admitted, never free text).
    pub actor: String,
    /// The current authorization decision for this resolution.
    pub authorization: AuthorizationOutcome,
    /// The allowed field scope of the resolution (the exact fields the
    /// admitted decision covers).
    pub fields: Vec<String>,
}

impl ResolutionAuthority {
    /// One checked authority: the actor is a bounded token and the
    /// field scope is non-empty, so an unscoped "resolve everything"
    /// authority cannot be constructed.
    pub fn new(
        actor: &str,
        authorization: AuthorizationOutcome,
        fields: Vec<String>,
    ) -> Option<Self> {
        let actor_ok = !actor.is_empty() && actor.len() <= 128;
        let fields_ok = !fields.is_empty()
            && fields
                .iter()
                .all(|field| !field.is_empty() && field.len() <= 128);
        (actor_ok && fields_ok).then(|| Self {
            actor: actor.to_owned(),
            authorization,
            fields,
        })
    }
}

/// Resolve one conflict under an explicit expected-version
/// precondition and a current scoped authority, appending an immutable
/// audit entry atomically with the resolution.
///
/// Admission order: an unauthorized resolver is refused outright
/// (`Denied`, no state change, no audit) whatever revision it names; a
/// resolver whose expected version does not equal the record's
/// observed revision is stale and refused outright; a fresh admitted
/// resolution closes the conflict and appends exactly one audit entry;
/// resolving an already-resolved record replays without a duplicate.
pub fn resolve_conflict(
    conflicts: &mut BTreeMap<String, ConflictRecord>,
    audit_log: &mut Vec<ResolutionAudit>,
    conflict_id: &str,
    expected_revision: &str,
    authority: &ResolutionAuthority,
) -> ResolveOutcome {
    // Authority first: an unadmitted actor never reaches the record,
    // and an empty expected revision is not a precondition at all.
    if authority.authorization != AuthorizationOutcome::Allowed || expected_revision.is_empty() {
        return ResolveOutcome::Denied;
    }
    let Some(record) = conflicts.get_mut(conflict_id) else {
        return ResolveOutcome::Unknown;
    };
    if record.state == ConflictState::Resolved {
        return ResolveOutcome::Replayed;
    }
    if record.observed_revision != expected_revision {
        return ResolveOutcome::Stale;
    }
    record.state = ConflictState::Resolved;
    audit_log.push(ResolutionAudit {
        conflict_id: conflict_id.to_owned(),
        link_id: record.link_id.clone(),
        expected_revision: expected_revision.to_owned(),
        resolved_revision: record.observed_revision.clone(),
        actor: authority.actor.to_owned(),
    });
    ResolveOutcome::Resolved
}

/// Detach one link: state only. History, receipts, conflicts, and the
/// external object are retained by construction; nothing here reaches a
/// provider.
pub fn detach_link(link: &mut Link) {
    link.state = LinkState::Detached;
    link.generation += 1;
}

/// Plan one outbox intent for an admitted decision.
///
/// A denied/unsupported recipient never receives an intent; callers
/// must only invoke this after [`decide`] returned [`DecisionOutcome::Allowed`].
/// Replay identity is the full immutable request: the same link, the
/// same idempotency key, the SAME action, and the SAME link generation
/// replay the original intent; any other reuse of the key is an
/// explicit [`PlanOutcome::ConflictingKey`] refusal — a different
/// action or a reattached generation gets its own identity, never a
/// borrowed one.
pub fn plan_intent(
    intents: &mut BTreeMap<String, Intent>,
    link: &Link,
    outcome: DecisionOutcome,
    action: &str,
    idempotency_key: String,
    now_ms: u64,
) -> PlanOutcome {
    let key = idempotency_key;
    // A denied, unsupported, or indeterminate recipient has an explicit
    // decision row and never an intent: the refusal is structural.
    if outcome != DecisionOutcome::Allowed {
        return PlanOutcome::NotAdmitted;
    }
    if link.state != LinkState::Active {
        return PlanOutcome::NotAdmitted;
    }
    // Replay identity is the originating request tuple, not the bare
    // key: a different action or a different generation is a conflicting
    // reuse of the key and is refused explicitly, never aliased.
    if let Some(existing) = intents
        .values()
        .find(|intent| intent.link_id == link.link_id && intent.idempotency_key == key)
    {
        if existing.action == action && existing.link_generation == link.generation {
            return PlanOutcome::Replayed(existing.clone());
        }
        return PlanOutcome::ConflictingKey;
    }
    // The intent id is injective: the link id and the key are joined in
    // fixed-width hex of their byte lengths, so `(a.b, c)` and `(a, b.c)`
    // can never collide. A collision is a caller bug and fails loudly.
    let intent_id = injective_intent_id(&link.link_id, &key);
    if intents.contains_key(&intent_id) {
        return PlanOutcome::NotAdmitted;
    }
    let intent = Intent {
        intent_id: intent_id.clone(),
        link_id: link.link_id.clone(),
        link_generation: link.generation,
        action: action.to_owned(),
        idempotency_key: key,
        state: OutboxState::Pending,
        attempts: 0,
        next_attempt_ms: now_ms,
        lease_token: None,
        lease_expires_ms: None,
        receipt_digest: None,
    };
    intents.insert(intent.intent_id.clone(), intent.clone());
    PlanOutcome::Planned(intent)
}

/// The typed outcome of one intent plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanOutcome {
    /// A fresh intent was planned and recorded.
    Planned(Intent),
    /// The exact originating request (link, key, action, generation)
    /// replayed its original intent.
    Replayed(Intent),
    /// The key is already bound to a different action or a different
    /// link generation: a conflicting reuse, refused explicitly.
    ConflictingKey,
    /// The decision did not admit an intent.
    NotAdmitted,
}

impl PlanOutcome {
    /// The intent when one was planned or replayed.
    pub fn into_intent(self) -> Option<Intent> {
        match self {
            Self::Planned(intent) | Self::Replayed(intent) => Some(intent),
            _ => None,
        }
    }
}

/// Build one collision-free intent id: `<link-hexlen>:<link>:<key-hexlen>:<key>`.
/// The length prefixes make the encoding injective for arbitrary text.
fn injective_intent_id(link_id: &str, key: &str) -> String {
    format!("{}:{}:{}:{}", link_id.len(), link_id, key.len(), key)
}

/// Claim one eligible intent under a fenced, time-bounded lease.
///
/// The lease expires at `now_ms + lease_ms`; a later dispatch after
/// expiry is fenced out (`LeaseExpired`) and [`recover_lease`] can
/// return the intent to pending for a fresh claim. Every fence is
/// enforced before a lease is handed out: the durable retry budget
/// (`attempts >= max_attempts` dead-letters the row, including after
/// claim/crash/recover cycles), the current link generation (a stale
/// generation cancels the intent), and the persistent
/// `unknown_delivery` state (claimable only after an explicit
/// [`reconcile_unknown_delivery`]).
pub fn claim(
    intent: &mut Intent,
    lease_token: u64,
    now_ms: u64,
    lease_ms: u64,
    link_generation: u64,
    max_attempts: u64,
) -> DispatchStep {
    if intent.state != OutboxState::Pending && intent.state != OutboxState::RetryWait {
        return DispatchStep::Ignored;
    }
    // The durable retry budget is enforced before every send: repeated
    // claim/crash/recover cycles can never bypass the bound.
    if intent.attempts >= max_attempts {
        intent.state = OutboxState::DeadLetter;
        intent.lease_token = None;
        intent.lease_expires_ms = None;
        return DispatchStep::DeadLettered;
    }
    if intent.link_generation != link_generation {
        intent.state = OutboxState::Cancelled;
        intent.lease_token = None;
        intent.lease_expires_ms = None;
        return DispatchStep::GenerationStale;
    }
    if now_ms < intent.next_attempt_ms {
        return DispatchStep::Ignored;
    }
    if lease_ms == 0 {
        return DispatchStep::Ignored;
    }
    intent.state = OutboxState::Leased;
    intent.lease_token = Some(lease_token);
    intent.lease_expires_ms = Some(now_ms.saturating_add(lease_ms));
    intent.attempts += 1;
    DispatchStep::Claimed
}

/// Apply one dispatch event to a leased (or acceptance-pending-ack)
/// intent.
///
/// Every path is fenced three ways: the state must be dispatchable,
/// the caller must hold the exact lease token, the intent's link
/// generation must still be current (`link_generation`), and the lease
/// must not have expired by time at `now_ms`. An expired lease is a
/// [`DispatchStep::LeaseExpired`] refusal that mutates nothing: only
/// [`recover_expired_lease`] may move an expired row. A stale
/// generation cancels the intent outright — a leased intent may not
/// survive its link's generation bump as dispatchable.
pub fn dispatch(
    intent: &mut Intent,
    event: &DispatchEvent,
    fence: u64,
    now_ms: u64,
    max_attempts: u64,
    backoff_ms: u64,
    link_generation: u64,
) -> DispatchStep {
    if intent.state != OutboxState::Leased && intent.state != OutboxState::AcceptedPendingAck {
        return DispatchStep::Ignored;
    }
    if intent.lease_token != Some(fence) {
        return DispatchStep::LeaseStale;
    }
    if intent.link_generation != link_generation {
        intent.state = OutboxState::Cancelled;
        intent.lease_token = None;
        intent.lease_expires_ms = None;
        return DispatchStep::GenerationStale;
    }
    if lease_expired(intent, now_ms) {
        return DispatchStep::LeaseExpired;
    }
    match event {
        DispatchEvent::ProviderAccepted => {
            if intent.state == OutboxState::AcceptedPendingAck {
                return DispatchStep::Ignored;
            }
            // Acceptance before acknowledgement: the durable record moves
            // to the explicit `accepted_pending_ack` state. It never
            // claims delivered and never silently re-sends; recovery in
            // this window reports `unknown_delivery` for reconciliation.
            intent.state = OutboxState::AcceptedPendingAck;
            DispatchStep::AcceptedPendingAck
        }
        DispatchEvent::Acknowledged => {
            // The acknowledgement completes the intent from either the
            // leased state or the acceptance-before-ack window: a live
            // worker that delivered successfully can always finish.
            intent.state = OutboxState::Delivered;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            intent.receipt_digest = Some(format!(
                "sha256:{}",
                crate::digest::sha256_hex(intent.idempotency_key.as_bytes(),)
            ));
            DispatchStep::Delivered
        }
        DispatchEvent::AuthorityRevoked => {
            intent.state = OutboxState::Cancelled;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            DispatchStep::Cancelled
        }
        // Failure and conflict paths are lease-state transitions: after
        // a provider acceptance there is nothing to retry, so those
        // events are no-ops inside the ack window.
        DispatchEvent::ProviderConflict if intent.state == OutboxState::Leased => {
            intent.state = OutboxState::Conflicted;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            DispatchStep::Conflicted
        }
        DispatchEvent::ProviderFailed if intent.state == OutboxState::Leased => {
            if intent.attempts >= max_attempts {
                intent.state = OutboxState::DeadLetter;
            } else {
                intent.state = OutboxState::RetryWait;
                intent.next_attempt_ms = now_ms.saturating_add(backoff_ms);
            }
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            if intent.state == OutboxState::DeadLetter {
                DispatchStep::DeadLettered
            } else {
                DispatchStep::RetryScheduled
            }
        }
        DispatchEvent::ProviderConflict | DispatchEvent::ProviderFailed => DispatchStep::Ignored,
    }
}

/// Recover one lease after a simulated crash.
///
/// A live lease returns the intent to pending under the same
/// idempotency key, preserving delivery uncertainty. Recovery from the
/// acceptance-before-acknowledgement window moves the durable record
/// to the persistent `unknown_delivery` state: the outcome cannot be
/// proven locally, the state is not dispatchable, and only the
/// explicit [`reconcile_unknown_delivery`] command resolves it. A
/// stale fence or a moved link generation is refused.
pub fn recover_lease(intent: &mut Intent, fence: u64, link_generation: u64) -> DispatchStep {
    if intent.lease_token != Some(fence) || intent.link_generation != link_generation {
        return DispatchStep::LeaseStale;
    }
    match intent.state {
        OutboxState::Leased => {
            intent.state = OutboxState::Pending;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            DispatchStep::Recovered
        }
        OutboxState::AcceptedPendingAck => {
            intent.state = OutboxState::UnknownDelivery;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            DispatchStep::UnknownDelivery
        }
        _ => DispatchStep::LeaseStale,
    }
}

/// Whether one lease has expired by time (`now_ms` past its deadline).
pub fn lease_expired(intent: &Intent, now_ms: u64) -> bool {
    matches!(
        intent.state,
        OutboxState::Leased | OutboxState::AcceptedPendingAck
    ) && intent
        .lease_expires_ms
        .is_some_and(|deadline| now_ms >= deadline)
}

/// Recover one expired lease: a time-expired live lease returns the
/// intent to pending; recovery from the ack window moves the row to
/// the persistent `unknown_delivery` state instead, so it stays
/// unre-sendable until reconciliation. A non-expired or non-leased
/// intent is a no-op. The link generation must still be current.
pub fn recover_expired_lease(
    intent: &mut Intent,
    now_ms: u64,
    link_generation: u64,
) -> DispatchStep {
    if intent.link_generation != link_generation {
        return DispatchStep::LeaseStale;
    }
    if !lease_expired(intent, now_ms) {
        return DispatchStep::Ignored;
    }
    let was_accepted = intent.state == OutboxState::AcceptedPendingAck;
    intent.state = if was_accepted {
        OutboxState::UnknownDelivery
    } else {
        OutboxState::Pending
    };
    intent.lease_token = None;
    intent.lease_expires_ms = None;
    if was_accepted {
        DispatchStep::UnknownDelivery
    } else {
        DispatchStep::LeaseExpired
    }
}

/// Reconcile one durable unknown-delivery record (the explicit
/// reconciliation command for the acceptance-before-ack window).
///
/// `Reconciliation::Delivered` completes the intent as delivered with
/// its receipt; `Reconciliation::NotDelivered` returns the intent to
/// pending so it may claim again. An intent in any other state is a
/// no-op: reconciliation exists only for the recorded uncertainty.
pub fn reconcile_unknown_delivery(intent: &mut Intent, outcome: Reconciliation) -> DispatchStep {
    if intent.state != OutboxState::UnknownDelivery {
        return DispatchStep::Ignored;
    }
    match outcome {
        Reconciliation::Delivered => {
            intent.state = OutboxState::Delivered;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            intent.receipt_digest = Some(format!(
                "sha256:{}",
                crate::digest::sha256_hex(intent.idempotency_key.as_bytes(),)
            ));
            DispatchStep::Delivered
        }
        Reconciliation::NotDelivered => {
            intent.state = OutboxState::Pending;
            DispatchStep::Recovered
        }
    }
}

/// Cancel every cancellable intent of a detached or revoked link:
/// pending/retry intents and any still-leased intent (generation
/// fencing means a leased intent may not survive its link's generation
/// bump as dispatchable). Delivered, conflicted, cancelled, and
/// dead-lettered rows are terminal and stay untouched; accepted
/// pending-ack and durable unknown-delivery rows are cancelled too —
/// their receipts must be reconciled and can never be silently
/// re-sent.
pub fn cancel_link_intents(intents: &mut BTreeMap<String, Intent>, link_id: &str) -> usize {
    let mut cancelled = 0;
    for intent in intents.values_mut() {
        if intent.link_id == link_id
            && matches!(
                intent.state,
                OutboxState::Pending
                    | OutboxState::RetryWait
                    | OutboxState::Leased
                    | OutboxState::AcceptedPendingAck
                    | OutboxState::UnknownDelivery
            )
        {
            intent.state = OutboxState::Cancelled;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            cancelled += 1;
        }
    }
    cancelled
}

/// The recipient-level delivery summary of one fan-out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliverySummary {
    /// Delivered and acknowledged recipients.
    pub delivered: usize,
    /// Admitted but unresolved recipients (partial delivery).
    pub pending: usize,
    /// Explicitly denied recipients (decision row, no intent).
    pub denied: usize,
    /// Unsupported recipients (decision row, no intent).
    pub unsupported: usize,
    /// Known terminal outcomes (conflicted, cancelled, dead-letter):
    /// resolved results, not unknown ones.
    pub resolved_terminal: usize,
    /// Explicit unknown-delivery observations: intents in the
    /// acceptance-before-ack window whose outcome cannot be proven
    /// locally and needs reconciliation.
    pub unknown: usize,
}

impl DeliverySummary {
    /// Whether overall delivery is complete (no admitted recipient
    /// unresolved). Partial is the honest default while anything is
    /// unresolved: unknown-delivery windows and pending rows both keep
    /// the fan-out explicitly partial.
    pub fn is_complete(&self) -> bool {
        self.pending == 0 && self.unknown == 0
    }
}

/// Summarize one fan-out from the typed intent map. The taxonomy is
/// honest: conflicted/cancelled/dead-letter are *known* terminal
/// outcomes (`resolved_terminal`); the acceptance-before-ack window
/// (`accepted_pending_ack`) and any pending row carrying the durable
/// unknown-delivery marker are genuinely `unknown`.
pub fn delivery_summary(intents: &BTreeMap<String, Intent>) -> DeliverySummary {
    let mut summary = DeliverySummary {
        delivered: 0,
        pending: 0,
        denied: 0,
        unsupported: 0,
        resolved_terminal: 0,
        unknown: 0,
    };
    for intent in intents.values() {
        match intent.state {
            OutboxState::Delivered => summary.delivered += 1,
            OutboxState::Pending | OutboxState::Leased | OutboxState::RetryWait => {
                summary.pending += 1
            }
            OutboxState::AcceptedPendingAck | OutboxState::UnknownDelivery => summary.unknown += 1,
            OutboxState::Conflicted | OutboxState::Cancelled | OutboxState::DeadLetter => {
                summary.resolved_terminal += 1
            }
        }
    }
    summary
}
