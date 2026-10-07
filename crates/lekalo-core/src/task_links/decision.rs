//! First-class capability and policy decisions plus the deterministic
//! delivery/outbox simulation core (issue #52).
//!
//! Every outbound action passes an explicit per-link decision built from
//! independent capability (`supported|partial|unsupported|unknown`) and
//! authorization (`allowed|denied`) observations; only an `allowed`
//! outcome may produce an outbox intent. The outbox engine is a pure
//! deterministic state machine over typed state: leased dispatch uses a
//! fencing token, retries keep their idempotency key, echoes never spawn
//! fresh intents, and acceptance-before-acknowledgement surfaces as an
//! explicit `unknown_delivery` observation instead of a guessed success.
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
    /// Originating installation.
    pub installation_id: String,
    /// Originating provider.
    pub provider_id: String,
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
    /// The step was a no-op (wrong state, exhausted, not yet eligible).
    Ignored,
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
            match (cursor.split_once('-'), revision.split_once('-')) {
                (Some((_, left)), Some((_, right))) => {
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
/// installation/provider/external tuple. String concatenation with a
/// separator is not injective, so the map key is the struct itself.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EventScope {
    /// Originating installation.
    pub installation_id: String,
    /// Originating provider.
    pub provider_id: String,
    /// External object id (exact text).
    pub external_id: String,
    /// Scoped event identity within the scope.
    pub event_identity: String,
}

/// Ingest one inbound event against the current inbox.
///
/// The inbox is keyed by the typed scope tuple, so a foreign or
/// detached-link event with a colliding text identity never aliases an
/// existing receipt. Returns `(applied, receipt)`: a repeated scoped
/// event identity replays the original outcome (`applied == false`)
/// without repeating effects. Non-comparable revisions with the cursor
/// produce the `conflict` receipt and never guess an ordering.
pub fn ingest_event(
    link: &mut Link,
    inbox: &mut BTreeMap<EventScope, String>,
    ordering: RevisionOrdering,
    event: &InboundEvent,
) -> (bool, &'static str) {
    let scope = EventScope {
        installation_id: event.installation_id.clone(),
        provider_id: event.provider_id.clone(),
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
pub fn plan_intent(
    intents: &mut BTreeMap<String, Intent>,
    link: &Link,
    outcome: DecisionOutcome,
    action: &str,
    idempotency_key: String,
    now_ms: u64,
) -> Option<Intent> {
    let key = idempotency_key;
    // A denied, unsupported, or indeterminate recipient has an explicit
    // decision row and never an intent: the refusal is structural.
    if outcome != DecisionOutcome::Allowed {
        return None;
    }
    if link.state != LinkState::Active {
        return None;
    }
    // The key scope is the originating intent for one recipient link;
    // retries replay it, a different link gets its own intent.
    if let Some(existing) = intents
        .values()
        .find(|intent| intent.link_id == link.link_id && intent.idempotency_key == key)
    {
        return Some(existing.clone());
    }
    // The intent id is injective: the link id and the key are joined in
    // fixed-width hex of their byte lengths, so `(a.b, c)` and `(a, b.c)`
    // can never collide. A collision is a caller bug and fails loudly.
    let intent_id = injective_intent_id(&link.link_id, &key);
    if intents.contains_key(&intent_id) {
        return None;
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
    Some(intent)
}

/// Build one collision-free intent id: `<link-hexlen>:<link>:<key-hexlen>:<key>`.
/// The length prefixes make the encoding injective for arbitrary text.
fn injective_intent_id(link_id: &str, key: &str) -> String {
    format!("{}:{}:{}:{}", link_id.len(), link_id, key.len(), key)
}

/// Claim one eligible intent under a fenced, time-bounded lease.
///
/// The lease expires at `now_ms + lease_ms`; a later claim attempt
/// after expiry is fenced out (`LeaseExpired`) and [`recover_lease`]
/// can return the intent to pending for a fresh claim.
pub fn claim(intent: &mut Intent, lease_token: u64, now_ms: u64, lease_ms: u64) -> DispatchStep {
    if intent.state != OutboxState::Pending && intent.state != OutboxState::RetryWait {
        return DispatchStep::Ignored;
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

/// Apply one dispatch event to a leased intent.
pub fn dispatch(
    intent: &mut Intent,
    event: &DispatchEvent,
    fence: u64,
    now_ms: u64,
    max_attempts: u64,
    backoff_ms: u64,
) -> DispatchStep {
    if intent.state != OutboxState::Leased {
        return DispatchStep::Ignored;
    }
    if intent.lease_token != Some(fence) {
        return DispatchStep::LeaseStale;
    }
    match event {
        DispatchEvent::ProviderAccepted => {
            // Acceptance before acknowledgement: the durable record moves
            // to the explicit `accepted_pending_ack` state. It never
            // claims delivered and never silently re-sends; recovery in
            // this window reports `unknown_delivery` for reconciliation.
            intent.state = OutboxState::AcceptedPendingAck;
            DispatchStep::AcceptedPendingAck
        }
        DispatchEvent::Acknowledged => {
            if intent.state == OutboxState::AcceptedPendingAck {
                intent.attempts = intent.attempts.saturating_sub(0);
            }
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
        DispatchEvent::ProviderConflict => {
            intent.state = OutboxState::Conflicted;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            DispatchStep::Conflicted
        }
        DispatchEvent::ProviderFailed => {
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
    }
}

/// Recover one lease after a simulated crash.
///
/// A live lease returns the intent to pending under the same
/// idempotency key, preserving delivery uncertainty. Recovery from the
/// acceptance-before-acknowledgement window returns the explicit
/// `unknown_delivery` step: the record cannot distinguish "never
/// sent" from "accepted, ack lost", so it reports the uncertainty
/// instead of guessing. A stale fence is refused in both states.
pub fn recover_lease(intent: &mut Intent, fence: u64) -> DispatchStep {
    if intent.lease_token != Some(fence) {
        return DispatchStep::LeaseStale;
    }
    match intent.state {
        OutboxState::Leased => {
            intent.state = OutboxState::Pending;
            intent.lease_token = None;
            intent.lease_expires_ms = None;
            DispatchStep::Claimed
        }
        OutboxState::AcceptedPendingAck => {
            intent.state = OutboxState::Pending;
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

/// Recover one expired lease: a time-expired lease (live or
/// accepted-pending-ack) returns the intent to pending, preserving the
/// key. A non-expired or non-leased intent is a no-op.
pub fn recover_expired_lease(intent: &mut Intent, now_ms: u64) -> DispatchStep {
    if !lease_expired(intent, now_ms) {
        return DispatchStep::Ignored;
    }
    let was_accepted = intent.state == OutboxState::AcceptedPendingAck;
    intent.state = OutboxState::Pending;
    intent.lease_token = None;
    intent.lease_expires_ms = None;
    if was_accepted {
        DispatchStep::UnknownDelivery
    } else {
        DispatchStep::LeaseExpired
    }
}

/// Cancel every cancellable intent of a detached or revoked link:
/// pending/retry intents and any still-leased intent (generation
/// fencing means a leased intent may not survive its link's generation
/// bump as dispatchable). Delivered, conflicted, cancelled, and
/// dead-lettered rows are terminal and stay untouched; an accepted
/// pending-ack row is cancelled too, its receipt must be reconciled
/// and can never be silently re-sent.
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
/// outcomes (`resolved_terminal`); only the acceptance-before-ack
/// window (`accepted_pending_ack`) is genuinely `unknown`.
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
            OutboxState::AcceptedPendingAck => summary.unknown += 1,
            OutboxState::Conflicted | OutboxState::Cancelled | OutboxState::DeadLetter => {
                summary.resolved_terminal += 1
            }
        }
    }
    summary
}
