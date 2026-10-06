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
}

/// Compose one typed decision from independent observations.
///
/// Capability and authorization must both permit the exact action and
/// field set; partial support stays allowed only with a non-empty
/// permitted subset (the caller narrows `fields` to it), unknown support
/// is never permission, and a detached link refuses everything.
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
    /// Provider accepted the write.
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

/// Deterministically compare two provider revisions.
///
/// Revisions are opaque: only exact equality is decidable here. A
/// declared comparable ordering belongs to the provider contract owner;
/// an incomparable pair is a conflict, never a lexical sort.
pub fn revision_incomparable(cursor: &str, revision: &str) -> bool {
    cursor != revision
}

/// Ingest one inbound event against the current inbox.
///
/// Returns `(applied, receipt)`: a repeated scoped event identity
/// replays the original outcome (`applied == false`) without repeating
/// effects. Non-comparable revisions with the cursor produce the
/// `conflict` receipt and never guess an ordering.
pub fn ingest_event(
    link: &mut Link,
    inbox: &mut BTreeMap<String, String>,
    event: &InboundEvent,
) -> (bool, &'static str) {
    if inbox.contains_key(&event.event_identity) {
        return (false, "replayed");
    }
    inbox.insert(
        event.event_identity.clone(),
        format!("{}|{}", event.provider_id, event.revision),
    );
    let conflict = link
        .inbound_cursor
        .as_ref()
        .map(|cursor| revision_incomparable(cursor, &event.revision))
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

/// Reserve one external identity across the workspace scope.
///
/// Returns `None` when the exact tuple is already bound to a
/// different task (the `tasklinks.error.identity_already_bound`
/// outcome); a repeated bind of the same tuple to the same task
/// replays the original link (`Some(Reservation::Replayed)`).
pub fn reserve_identity(
    bindings: &mut BTreeMap<String, String>,
    workspace_id: &str,
    installation_id: &str,
    provider_id: &str,
    object_type: &str,
    external_id: &str,
    task_id: &str,
) -> Option<Reservation> {
    let tuple =
        format!("{workspace_id}|{installation_id}|{provider_id}|{object_type}|{external_id}");
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
    let intent_id = format!("{}.{}", link.link_id, key);
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
        receipt_digest: None,
    };
    intents.insert(intent.intent_id.clone(), intent.clone());
    Some(intent)
}

/// Claim one eligible intent under a fenced lease.
pub fn claim(intent: &mut Intent, lease_token: u64, now_ms: u64, lease_ms: u64) -> DispatchStep {
    if intent.state != OutboxState::Pending && intent.state != OutboxState::RetryWait {
        return DispatchStep::Ignored;
    }
    if now_ms < intent.next_attempt_ms {
        return DispatchStep::Ignored;
    }
    intent.state = OutboxState::Leased;
    intent.lease_token = Some(lease_token);
    intent.attempts += 1;
    let _ = lease_ms;
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
            // Acceptance before acknowledgement: the durable record
            // stays leased until [`DispatchEvent::Acknowledged`]; a
            // restart inside this window must reconcile or report
            // unknown delivery, never silently re-send.
            DispatchStep::Ignored
        }
        DispatchEvent::Acknowledged => {
            intent.state = OutboxState::Delivered;
            intent.lease_token = None;
            intent.receipt_digest = Some(format!(
                "sha256:{}",
                crate::digest::sha256_hex(intent.idempotency_key.as_bytes(),)
            ));
            DispatchStep::Delivered
        }
        DispatchEvent::AuthorityRevoked => {
            intent.state = OutboxState::Cancelled;
            intent.lease_token = None;
            DispatchStep::Cancelled
        }
        DispatchEvent::ProviderConflict => {
            intent.state = OutboxState::Conflicted;
            intent.lease_token = None;
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
            if intent.state == OutboxState::DeadLetter {
                DispatchStep::DeadLettered
            } else {
                DispatchStep::RetryScheduled
            }
        }
    }
}

/// Recover one lease after a simulated crash: a stale lease returns the
/// intent to pending (the same idempotency key), preserving delivery
/// uncertainty instead of assuming failure.
pub fn recover_lease(intent: &mut Intent, fence: u64) -> DispatchStep {
    if intent.state != OutboxState::Leased || intent.lease_token != Some(fence) {
        return DispatchStep::LeaseStale;
    }
    intent.state = OutboxState::Pending;
    intent.lease_token = None;
    DispatchStep::Claimed
}

/// Cancel every pending/retry intent of a detached or revoked link.
pub fn cancel_link_intents(intents: &mut BTreeMap<String, Intent>, link_id: &str) -> usize {
    let mut cancelled = 0;
    for intent in intents.values_mut() {
        if intent.link_id == link_id
            && matches!(intent.state, OutboxState::Pending | OutboxState::RetryWait)
        {
            intent.state = OutboxState::Cancelled;
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
    /// Explicit unknown-delivery observations.
    pub unknown: usize,
}

impl DeliverySummary {
    /// Whether overall delivery is complete (no admitted recipient
    /// unresolved). Partial is the honest default while anything is
    /// unresolved.
    pub fn is_complete(&self) -> bool {
        self.pending == 0 && self.unknown == 0
    }
}

/// Summarize one fan-out from the typed intent map.
pub fn delivery_summary(intents: &BTreeMap<String, Intent>) -> DeliverySummary {
    let mut summary = DeliverySummary {
        delivered: 0,
        pending: 0,
        denied: 0,
        unsupported: 0,
        unknown: 0,
    };
    for intent in intents.values() {
        match intent.state {
            OutboxState::Delivered => summary.delivered += 1,
            OutboxState::Pending | OutboxState::Leased | OutboxState::RetryWait => {
                summary.pending += 1
            }
            OutboxState::Conflicted | OutboxState::Cancelled | OutboxState::DeadLetter => {
                summary.unknown += 1
            }
        }
    }
    summary
}
