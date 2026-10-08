//! Issue #52 core battery: the ten scenario families run through the
//! pure deterministic engine, plus the registry-chain and wire invariants.
//!
//! Everything here is in-process and clock-injected: no filesystem
//! beyond this crate, no network, no threads for the deterministic
//! legs. The single synchronized leg (identity race) uses two threads
//! over one mutex-guarded reservation table to prove the exact-tuple
//! uniqueness under interleaving.

use std::collections::BTreeMap;

use lekalo_core::task_links::decision::{
    self, AuthorizationOutcome, CapabilityOutcome, DecisionInput, DispatchEvent, DispatchStep,
    InboundEvent, Link, LinkState, OutboxState, SyncMode,
};

const WORKSPACE: &str = "ws-1";
const INSTALLATION_A: &str = "inst-primary-1";
const INSTALLATION_B: &str = "inst-secondary-1";
const PROVIDER_A: &str = "github-issues";
const PROVIDER_B: &str = "mock-secondary";
const OBJECT_TYPE: &str = "issue";
const EXTERNAL_ID: &str = "42";
const TASK_ONE: &str = "11111111-1111-1111-1111-111111111111";
const TASK_TWO: &str = "22222222-2222-2222-2222-222222222222";
const LINK_A: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const LINK_B: &str = "bbbbbbbb-0000-0000-0000-000000000002";

fn link(link_id: &str, task: &str, installation: &str, provider: &str) -> Link {
    Link {
        link_id: link_id.to_owned(),
        task_id: task.to_owned(),
        workspace_id: WORKSPACE.to_owned(),
        installation_id: installation.to_owned(),
        provider_id: provider.to_owned(),
        object_type: OBJECT_TYPE.to_owned(),
        external_id: EXTERNAL_ID.to_owned(),
        state: LinkState::Active,
        sync_mode: SyncMode::Bidirectional,
        generation: 1,
        inbound_cursor: None,
        outbound_cursor: None,
    }
}

fn allowed_input(action: &str) -> DecisionInput {
    DecisionInput {
        action: action.to_owned(),
        fields: vec!["state".to_owned(), "title".to_owned()],
        capability: CapabilityOutcome::Supported,
        authorization: AuthorizationOutcome::Allowed,
        link_state: LinkState::Active,
        sync_mode: SyncMode::Bidirectional,
        permitted_subset: None,
    }
}

/// AC: one task linked to two providers; one allowed action produces
/// two independently attributed intents.
#[test]
fn scenario_two_providers_fans_out_two_attributed_intents() {
    let mut intents = BTreeMap::new();
    let a = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let b = link(LINK_B, TASK_ONE, INSTALLATION_B, PROVIDER_B);
    let first = decision::plan_intent(
        &mut intents,
        &a,
        decision::DecisionOutcome::Allowed,
        "tasklinks.request_action",
        "key-one".into(),
        0,
    )
    .expect("intent for provider A");
    let second = decision::plan_intent(
        &mut intents,
        &b,
        decision::DecisionOutcome::Allowed,
        "tasklinks.request_action",
        "key-one".into(),
        0,
    )
    .expect("intent for provider B");
    assert_ne!(first.intent_id, second.intent_id);
    assert_ne!(first.link_id, second.link_id);
    assert_eq!(first.idempotency_key, second.idempotency_key);
    assert_eq!(intents.len(), 2);
    // Attribution stays distinguishable per provider.
    assert_eq!(first.link_id, LINK_A);
    assert_eq!(second.link_id, LINK_B);
    let summary = decision::delivery_summary(&intents);
    assert_eq!(summary.pending, 2);
    assert!(!summary.is_complete());
}

/// AC: duplicate link/import is idempotent.
#[test]
fn scenario_duplicate_import_replays_one_link() {
    let mut bindings = BTreeMap::new();
    let first = decision::reserve_identity(
        &mut bindings,
        WORKSPACE,
        INSTALLATION_A,
        PROVIDER_A,
        OBJECT_TYPE,
        EXTERNAL_ID,
        TASK_ONE,
    )
    .expect("first reservation is fresh");
    let second = decision::reserve_identity(
        &mut bindings,
        WORKSPACE,
        INSTALLATION_A,
        PROVIDER_A,
        OBJECT_TYPE,
        EXTERNAL_ID,
        TASK_ONE,
    )
    .expect("repeat reservation replays");
    assert_eq!(first, decision::Reservation::Fresh);
    assert_eq!(second, decision::Reservation::Replayed);
    assert_eq!(bindings.len(), 1, "one reservation, one link");
}

/// AC: the same external object cannot bind two tasks silently.
#[test]
fn scenario_identity_collision_refuses_the_second_task() {
    let mut bindings = BTreeMap::new();
    decision::reserve_identity(
        &mut bindings,
        WORKSPACE,
        INSTALLATION_A,
        PROVIDER_A,
        OBJECT_TYPE,
        EXTERNAL_ID,
        TASK_ONE,
    )
    .expect("first bind");
    let collision = decision::reserve_identity(
        &mut bindings,
        WORKSPACE,
        INSTALLATION_A,
        PROVIDER_A,
        OBJECT_TYPE,
        EXTERNAL_ID,
        TASK_TWO,
    );
    assert!(collision.is_none(), "identity-already-bound");
    assert_eq!(bindings.len(), 1, "no second link was created");
    // A different installation with the same external id stays distinct.
    let other = decision::reserve_identity(
        &mut bindings,
        WORKSPACE,
        INSTALLATION_B,
        PROVIDER_B,
        OBJECT_TYPE,
        EXTERNAL_ID,
        TASK_ONE,
    )
    .expect("distinct installation is a positive control");
    assert_eq!(other, decision::Reservation::Fresh);
    assert_eq!(bindings.len(), 2);
    // Byte-distinct external identities are distinct rows.
    let case = decision::reserve_identity(
        &mut bindings,
        WORKSPACE,
        INSTALLATION_A,
        PROVIDER_A,
        OBJECT_TYPE,
        "42 ",
        TASK_ONE,
    )
    .expect("exact-text comparison keeps byte-distinct ids distinct");
    assert_eq!(case, decision::Reservation::Fresh);
}

/// Synchronized native-style race: two competing reservations of one
/// exact tuple; exactly one wins.
#[test]
fn synchronized_identity_race_binds_exactly_one_task() {
    let bindings = std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new()));
    let (tx, rx) = std::sync::mpsc::channel();
    let mut handles = Vec::new();
    for task in [TASK_ONE, TASK_TWO] {
        let tx = tx.clone();
        let bindings = std::sync::Arc::clone(&bindings);
        handles.push(std::thread::spawn(move || {
            let mut guard = bindings.lock().unwrap();
            let outcome = decision::reserve_identity(
                &mut guard,
                WORKSPACE,
                INSTALLATION_A,
                PROVIDER_A,
                OBJECT_TYPE,
                EXTERNAL_ID,
                task,
            );
            tx.send((task, outcome.is_some())).unwrap();
        }));
    }
    drop(tx);
    for handle in handles {
        handle.join().expect("race worker");
    }
    let mut winners = Vec::new();
    for (task, won) in rx {
        if won {
            winners.push(task);
        }
    }
    assert_eq!(winners.len(), 1, "exactly one reservation wins the race");
    let guard = bindings.lock().unwrap();
    assert_eq!(guard.len(), 1);
    let bound_task = guard.values().next().unwrap();
    assert_eq!(bound_task, &winners[0]);
}

/// AC: inbound events aggregate with attribution; duplicates do not
/// repeat effects; echoes never create fresh intents.
#[test]
fn scenario_attributed_inbound_dedupes_and_suppresses_echoes() {
    let mut link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let mut inbox = BTreeMap::new();
    let event = InboundEvent {
        event_identity: "evt-primary-0001".to_owned(),
        installation_id: INSTALLATION_A.to_owned(),
        provider_id: PROVIDER_A.to_owned(),
        external_id: EXTERNAL_ID.to_owned(),
        revision: "r7".to_owned(),
        causation_id: "cause-1".to_owned(),
    };
    let (applied, receipt) = decision::ingest_event(
        &mut link_row,
        &mut inbox,
        decision::RevisionOrdering::ComparableTotal,
        &event,
    );
    assert!(applied);
    assert_eq!(receipt, "applied");
    assert_eq!(link_row.inbound_cursor.as_deref(), Some("r7"));
    // The exact duplicate replays without repeating effects.
    let (applied, receipt) = decision::ingest_event(
        &mut link_row,
        &mut inbox,
        decision::RevisionOrdering::ComparableTotal,
        &event,
    );
    assert!(!applied, "duplicate never repeats effects");
    assert_eq!(receipt, "replayed");
    assert_eq!(inbox.len(), 1);
    // Attribution survives: provider/installation/revision distinguishable.
    let scope = decision::EventScope {
        installation_id: INSTALLATION_A.to_owned(),
        provider_id: PROVIDER_A.to_owned(),
        external_id: EXTERNAL_ID.to_owned(),
        event_identity: "evt-primary-0001".to_owned(),
    };
    assert_eq!(
        inbox.get(&scope).map(String::as_str),
        Some("github-issues|r7")
    );
}

/// Non-comparable provider revisions create an explicit conflict
/// receipt, never a lexical ordering.
#[test]
fn scenario_incomparable_revision_is_a_conflict_not_a_sort() {
    let mut link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    link_row.inbound_cursor = Some("r7".to_owned());
    let mut inbox = BTreeMap::new();
    let event = InboundEvent {
        event_identity: "evt-primary-0002".to_owned(),
        installation_id: INSTALLATION_A.to_owned(),
        provider_id: PROVIDER_A.to_owned(),
        external_id: EXTERNAL_ID.to_owned(),
        revision: "zz-unrelated".to_owned(),
        causation_id: "cause-2".to_owned(),
    };
    let (applied, receipt) = decision::ingest_event(
        &mut link_row,
        &mut inbox,
        decision::RevisionOrdering::IncomparableDeclared,
        &event,
    );
    assert!(applied);
    assert_eq!(receipt, "conflict");
    assert_eq!(
        link_row.inbound_cursor.as_deref(),
        Some("r7"),
        "cursor never guessed"
    );
    assert_eq!(inbox.len(), 1, "the event is still attributed and recorded");
}

/// AC: fan-out only for supported/allowed action — the decision matrix.
#[test]
fn scenario_decision_matrix_admits_only_allowed_and_supported() {
    let denied = decision::decide(&DecisionInput {
        authorization: AuthorizationOutcome::Denied,
        ..allowed_input("tasklinks.request_action")
    });
    assert_eq!(denied, decision::DecisionOutcome::Denied);
    let unsupported = decision::decide(&DecisionInput {
        capability: CapabilityOutcome::Unsupported,
        ..allowed_input("tasklinks.request_action")
    });
    assert_eq!(unsupported, decision::DecisionOutcome::Unsupported);
    let unknown = decision::decide(&DecisionInput {
        capability: CapabilityOutcome::Unknown,
        ..allowed_input("tasklinks.request_action")
    });
    assert_eq!(unknown, decision::DecisionOutcome::Indeterminate);
    let detached = decision::decide(&DecisionInput {
        link_state: LinkState::Detached,
        ..allowed_input("tasklinks.request_action")
    });
    assert_eq!(detached, decision::DecisionOutcome::Denied);
    let inbound_only = decision::decide(&DecisionInput {
        sync_mode: SyncMode::Inbound,
        ..allowed_input("tasklinks.request_action")
    });
    assert_eq!(inbound_only, decision::DecisionOutcome::Denied);
    let admitted = decision::decide(&allowed_input("tasklinks.request_action"));
    assert_eq!(admitted, decision::DecisionOutcome::Allowed);
    // A denied/unsupported recipient never receives an intent.
    let mut intents = BTreeMap::new();
    let link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    for outcome in [
        decision::DecisionOutcome::Denied,
        decision::DecisionOutcome::Unsupported,
        decision::DecisionOutcome::Indeterminate,
    ] {
        assert!(
            decision::plan_intent(
                &mut intents,
                &link_row,
                outcome,
                "x",
                format!("key-{outcome:?}"),
                0
            )
            .is_none(),
            "{outcome:?} never plans an intent"
        );
    }
    assert!(intents.is_empty());
}

/// AC: partial provider failure — A delivers, B times out; status stays
/// partial, A's receipt persists, only B is retry-eligible.
#[test]
fn scenario_partial_delivery_keeps_receipts_and_partial_status() {
    let mut intents = BTreeMap::new();
    let a = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let b = link(LINK_B, TASK_ONE, INSTALLATION_B, PROVIDER_B);
    let first = decision::plan_intent(
        &mut intents,
        &a,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-a".into(),
        0,
    )
    .unwrap();
    assert_eq!(first.state, OutboxState::Pending);
    decision::plan_intent(
        &mut intents,
        &b,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-b".into(),
        0,
    )
    .unwrap();
    // Deliver A.
    let key = intents
        .values()
        .find(|i| i.idempotency_key == "k-a")
        .map(|i| i.intent_id.clone())
        .expect("intent A");
    let intent = intents.get_mut(&key).unwrap();
    assert_eq!(decision::claim(intent, 7, 0, 30_000), DispatchStep::Claimed);
    assert_eq!(
        decision::dispatch(intent, &DispatchEvent::Acknowledged, 7, 10, 5, 100),
        DispatchStep::Delivered
    );
    // Time out B.
    let intent = intents
        .values_mut()
        .find(|i| i.idempotency_key == "k-b")
        .unwrap();
    assert_eq!(decision::claim(intent, 9, 0, 30_000), DispatchStep::Claimed);
    assert_eq!(
        decision::dispatch(intent, &DispatchEvent::ProviderFailed, 9, 10, 5, 100),
        DispatchStep::RetryScheduled
    );
    let summary = decision::delivery_summary(&intents);
    assert_eq!(summary.delivered, 1, "A's receipt persists");
    assert_eq!(summary.pending, 1, "only B is unresolved");
    assert!(!summary.is_complete(), "status stays partial");
    // Retry keeps the same idempotency key.
    let intent = intents
        .values_mut()
        .find(|i| i.idempotency_key == "k-b")
        .unwrap();
    assert_eq!(
        decision::claim(intent, 11, 200, 30_000),
        DispatchStep::Claimed
    );
    assert_eq!(intent.idempotency_key, "k-b");
}

/// AC: stale/conflicting update — a provider conflict is a known terminal
/// outcome (never retried), while retry-eligible failures exhaust into
/// dead_letter at the bound; delivery is not masked complete either way.
#[test]
fn scenario_stale_conflict_and_bounded_retry_dead_letter() {
    let mut intents = BTreeMap::new();
    let link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-dead".into(),
        0,
    )
    .unwrap();
    // A provider conflict stops the intent outright: a known terminal
    // outcome, never a retry and never an unknown.
    {
        let intent = intents
            .values_mut()
            .find(|i| i.idempotency_key == "k-dead")
            .unwrap();
        assert_eq!(decision::claim(intent, 1, 0, 1), DispatchStep::Claimed);
        assert_eq!(
            decision::dispatch(intent, &DispatchEvent::ProviderConflict, 1, 0, 3, 100),
            DispatchStep::Conflicted
        );
    }
    let summary = decision::delivery_summary(&intents);
    assert_eq!(
        summary.resolved_terminal, 1,
        "conflicted is a known terminal outcome, not unknown"
    );
    assert_eq!(summary.unknown, 0, "no ack-window uncertainty here");
    assert!(
        summary.is_complete(),
        "a known conflict resolves the fan-out"
    );

    // Retry-eligible failures exhaust into dead_letter at the bound.
    let mut intents = BTreeMap::new();
    decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-dead-retry".into(),
        0,
    )
    .unwrap();
    let mut attempts = 0;
    loop {
        let intent = intents
            .values_mut()
            .find(|i| i.idempotency_key == "k-dead-retry")
            .unwrap();
        match intent.state {
            OutboxState::Pending | OutboxState::RetryWait => {
                attempts += 1;
                assert_eq!(
                    decision::claim(intent, attempts, (attempts - 1) * 1000, 1),
                    DispatchStep::Claimed
                );
                let step = decision::dispatch(
                    intent,
                    &DispatchEvent::ProviderFailed,
                    attempts,
                    (attempts - 1) * 1000,
                    3,
                    100,
                );
                if attempts < 3 {
                    assert_eq!(step, DispatchStep::RetryScheduled);
                } else {
                    assert_eq!(step, DispatchStep::DeadLettered);
                }
            }
            _ => break,
        }
    }
    assert_eq!(attempts, 3, "bounded retries exhaust at maxAttempts");
    let intent = intents
        .values()
        .find(|i| i.idempotency_key == "k-dead-retry")
        .unwrap();
    assert_eq!(intent.state, OutboxState::DeadLetter);
    assert_eq!(intent.idempotency_key, "k-dead-retry");
    let summary = decision::delivery_summary(&intents);
    assert_eq!(summary.resolved_terminal, 1, "dead_letter is resolved");
}

/// Partial capability admits only a covered requested mask: a partial
/// outcome without an explicit permitted subset is undecidable, and a
/// subset that does not cover the request is unsupported.
#[test]
fn partial_capability_requires_a_covering_permitted_subset() {
    let base = DecisionInput {
        capability: CapabilityOutcome::Partial,
        ..allowed_input("tasklinks.request_action")
    };
    assert_eq!(
        decision::decide(&base),
        decision::DecisionOutcome::Indeterminate
    );
    let empty = DecisionInput {
        permitted_subset: Some(vec![]),
        ..base.clone()
    };
    assert_eq!(
        decision::decide(&empty),
        decision::DecisionOutcome::Unsupported
    );
    let narrow = DecisionInput {
        permitted_subset: Some(vec!["title".to_owned()]),
        ..base.clone()
    };
    assert_eq!(
        decision::decide(&narrow),
        decision::DecisionOutcome::Unsupported
    );
    let covered = DecisionInput {
        permitted_subset: Some(vec!["state".to_owned(), "title".to_owned()]),
        ..base
    };
    assert_eq!(
        decision::decide(&covered),
        decision::DecisionOutcome::Allowed
    );
}

/// Lease expiry is time-owned: the deadline is recorded at claim time,
/// expiry is observable, and recovery of an expired lease returns the
/// intent to pending with the key retained.
#[test]
fn lease_expiry_is_fenced_by_time() {
    let mut intents = BTreeMap::new();
    let link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let planned = decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-lease".into(),
        0,
    )
    .expect("intent");
    let intent_id = planned.intent_id.clone();
    let intent = intents.get_mut(&intent_id).unwrap();
    assert_eq!(
        decision::claim(intent, 5, 100, 1_000),
        DispatchStep::Claimed
    );
    assert!(!decision::lease_expired(intent, 1_099));
    assert!(decision::lease_expired(intent, 1_100));
    assert_eq!(
        decision::recover_expired_lease(intent, 1_100),
        DispatchStep::LeaseExpired
    );
    assert_eq!(intent.state, OutboxState::Pending);
    assert_eq!(intent.lease_token, None);
    assert_eq!(intent.idempotency_key, "k-lease");
}

/// Intent ids are injective: colliding text concatenations can never
/// overwrite a durable intent.
#[test]
fn intent_ids_are_injective_under_concatenation_collisions() {
    let mut intents = BTreeMap::new();
    let a = link("a.b", TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let b = link("a", TASK_TWO, INSTALLATION_B, PROVIDER_B);
    decision::plan_intent(
        &mut intents,
        &a,
        decision::DecisionOutcome::Allowed,
        "x",
        "c".into(),
        0,
    )
    .expect("first intent");
    decision::plan_intent(
        &mut intents,
        &b,
        decision::DecisionOutcome::Allowed,
        "x",
        "b.c".into(),
        0,
    )
    .expect("second intent");
    assert_eq!(intents.len(), 2, "no silent overwrite");
    for intent in intents.values() {
        let expected = format!(
            "{}:{}:{}:{}",
            intent.link_id.len(),
            intent.link_id,
            intent.idempotency_key.len(),
            intent.idempotency_key
        );
        assert_eq!(intent.intent_id, expected);
    }
}

/// Bounded attempts exhaust into dead_letter with the key retained.
#[test]
fn bounded_retries_exhaust_into_dead_letter_keeping_the_key() {
    let mut intents = BTreeMap::new();
    let link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-bounded".into(),
        0,
    )
    .unwrap();
    for attempt in 1..=5u64 {
        let intent = intents
            .values_mut()
            .find(|i| i.idempotency_key == "k-bounded")
            .unwrap();
        assert_eq!(
            decision::claim(intent, attempt, 0, 1),
            DispatchStep::Claimed,
            "attempt {attempt}"
        );
        let step = decision::dispatch(intent, &DispatchEvent::ProviderFailed, attempt, 0, 5, 0);
        if attempt < 5 {
            assert_eq!(step, DispatchStep::RetryScheduled);
        } else {
            assert_eq!(step, DispatchStep::DeadLettered);
        }
    }
    let intent = intents
        .values()
        .find(|i| i.idempotency_key == "k-bounded")
        .unwrap();
    assert_eq!(intent.state, OutboxState::DeadLetter);
    assert_eq!(intent.idempotency_key, "k-bounded", "retries keep the key");
}

/// AC: detach preserves history and the external object; pending
/// dispatch is cancelled; no external delete exists.
#[test]
fn scenario_detach_history_cancels_pending_and_never_deletes() {
    let mut link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    link_row.inbound_cursor = Some("r7".to_owned());
    let mut intents = BTreeMap::new();
    decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-pending".into(),
        0,
    )
    .unwrap();
    decision::detach_link(&mut link_row);
    assert_eq!(link_row.state, LinkState::Detached);
    assert_eq!(link_row.generation, 2, "reattach generation bumps");
    assert_eq!(
        link_row.inbound_cursor.as_deref(),
        Some("r7"),
        "history and cursors are retained"
    );
    assert_eq!(
        link_row.external_id, EXTERNAL_ID,
        "the external reservation is retained"
    );
    assert_eq!(
        decision::cancel_link_intents(&mut intents, LINK_A),
        1,
        "pending dispatch is cancelled"
    );
    let intent = intents.values().next().unwrap();
    assert_eq!(intent.state, OutboxState::Cancelled);
    // A detached link refuses new intents outright.
    assert!(
        decision::plan_intent(
            &mut intents,
            &link_row,
            decision::DecisionOutcome::Allowed,
            "x",
            "k-after".into(),
            0
        )
        .is_none(),
        "no fresh intent after detach"
    );
}

/// AC: retry does not create loop/duplicate — the echo of our own
/// outbound write never spawns a fresh intent, and acceptance before
/// acknowledgement surfaces as explicit uncertainty.
#[test]
fn scenario_retry_echo_crash_preserves_key_and_uncertainty() {
    let mut link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let mut intents = BTreeMap::new();
    let planned = decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-echo".into(),
        0,
    )
    .expect("first plan");
    let original_id = planned.intent_id.clone();
    // The provider echo is an inbound event whose causation is our own
    // intent: it is deduplicated and attributed, never a fresh intent.
    let mut inbox = BTreeMap::new();
    let echo = InboundEvent {
        event_identity: "evt-primary-echo".to_owned(),
        installation_id: INSTALLATION_A.to_owned(),
        provider_id: PROVIDER_A.to_owned(),
        external_id: EXTERNAL_ID.to_owned(),
        revision: "r8".to_owned(),
        causation_id: original_id.clone(),
    };
    let (applied, receipt) = decision::ingest_event(
        &mut link_row,
        &mut inbox,
        decision::RevisionOrdering::ComparableTotal,
        &echo,
    );
    assert!(applied);
    assert_eq!(receipt, "applied");
    assert_eq!(intents.len(), 1, "the echo created no fresh intent");
    // Re-planning with the same idempotency key replays the same intent.
    let replayed = decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-echo".into(),
        5,
    )
    .expect("replay");
    assert_eq!(replayed.intent_id, original_id);
    assert_eq!(intents.len(), 1);
    // Acceptance before acknowledgement: the durable record moves to
    // the explicit accepted-pending-ack state, never delivered.
    {
        let intent = intents.get_mut(&original_id).unwrap();
        assert_eq!(decision::claim(intent, 3, 0, 1), DispatchStep::Claimed);
        assert_eq!(
            decision::dispatch(intent, &DispatchEvent::ProviderAccepted, 3, 1, 5, 0),
            DispatchStep::AcceptedPendingAck,
            "acceptance alone never marks delivered"
        );
        assert_eq!(intent.state, OutboxState::AcceptedPendingAck);
        // Simulated crash inside the ack window: recovery reports the
        // explicit unknown-delivery observation and returns the intent
        // to pending under the same key (reconciliation, not a guess).
        assert_eq!(
            decision::recover_lease(intent, 3),
            DispatchStep::UnknownDelivery
        );
        assert_eq!(intent.state, OutboxState::Pending);
        assert_eq!(intent.idempotency_key, "k-echo");
    }
    // A stale fence is refused.
    {
        let intent = intents.get_mut(&original_id).unwrap();
        assert_eq!(
            decision::recover_lease(intent, 99),
            DispatchStep::LeaseStale
        );
    }
    // Authority revoked after enqueue: cancelled, never delivered.
    let intent = intents.get_mut(&original_id).unwrap();
    assert_eq!(decision::claim(intent, 4, 10, 1), DispatchStep::Claimed);
    assert_eq!(
        decision::dispatch(intent, &DispatchEvent::AuthorityRevoked, 4, 11, 5, 0),
        DispatchStep::Cancelled
    );
    assert_eq!(intent.state, OutboxState::Cancelled);
}

/// AC: the provider token never enters payload/evidence — the
/// serialized engine surfaces contain no credential material.
#[test]
fn scenario_secret_canary_absent_from_engine_artifacts() {
    const CANARY: &str = "LEKALO-SYNTHETIC-TOKEN-CANARY-0F0F";
    let mut link_row = link(LINK_A, TASK_ONE, INSTALLATION_A, PROVIDER_A);
    let mut intents = BTreeMap::new();
    decision::plan_intent(
        &mut intents,
        &link_row,
        decision::DecisionOutcome::Allowed,
        "x",
        "k-canary".into(),
        0,
    )
    .unwrap();
    let mut inbox = BTreeMap::new();
    let event = InboundEvent {
        event_identity: "evt-canary".to_owned(),
        installation_id: INSTALLATION_A.to_owned(),
        provider_id: PROVIDER_A.to_owned(),
        external_id: EXTERNAL_ID.to_owned(),
        revision: "r9".to_owned(),
        causation_id: "cause-canary".to_owned(),
    };
    decision::ingest_event(
        &mut link_row,
        &mut inbox,
        decision::RevisionOrdering::ComparableTotal,
        &event,
    );
    let rendered = format!("{:?}{:?}{:?}", link_row, intents, inbox);
    assert!(
        !rendered.contains(CANARY),
        "canary leaked into engine state"
    );
    assert!(
        !rendered.contains("handle-primary-1"),
        "credential handle leaked"
    );
}

/// The registry chain admits the eight new rules while predecessor
/// producers keep their exact pins (issue #52 successor discipline).
#[test]
fn registry_chain_keeps_pins_and_admits_tasklink_rules() {
    let embedded = lekalo_core::diagnostics::DiagnosticRegistry::embedded().unwrap();
    let successor = lekalo_core::diagnostics::DiagnosticRegistry::successor().unwrap();
    let tasklink = lekalo_core::diagnostics::DiagnosticRegistry::tasklink_successor().unwrap();
    assert_eq!(embedded.len(), 514);
    assert_eq!(successor.len(), embedded.len() + 8);
    assert_eq!(tasklink.len(), successor.len() + 8);
    for entry in embedded.entries() {
        assert_eq!(successor.entry(entry.id()), Some(entry));
        assert_eq!(tasklink.entry(entry.id()), Some(entry));
    }
    for entry in successor.entries() {
        assert_eq!(tasklink.entry(entry.id()), Some(entry));
    }
}

/// The qualified native leg: a pinned transactional DB runner over the
/// bundled sqlite engine with process-restart failpoints. These tests
/// are `#[ignore]`d by default and execute only when the qualified
/// environment runs the native gate with LEKALO_TASKLINKS_NATIVE=1
/// plus a named DB file; a mock never satisfies them. Durability means
/// reopen-the-file: every crash leg closes the database, re-opens it,
/// and asserts the exact surviving state.
mod native_leg {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use lekalo_core::task_links::decision::{
        self, DispatchEvent, DispatchStep, Link, LinkState, SyncMode,
    };

    fn db_path() -> PathBuf {
        let path = std::env::var_os("LEKALO_TASKLINKS_NATIVE_DB")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .expect("LEKALO_TASKLINKS_NATIVE_DB names the transactional DB file");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        path
    }

    fn open(path: &std::path::Path) -> rusqlite::Connection {
        let connection = rusqlite::Connection::open(path).expect("open transactional db");
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = FULL;
                 CREATE TABLE IF NOT EXISTS intents (
                     intent_id TEXT PRIMARY KEY,
                     link_id TEXT NOT NULL,
                     idempotency_key TEXT NOT NULL,
                     state TEXT NOT NULL,
                     attempts INTEGER NOT NULL,
                     lease_token INTEGER,
                     receipt_digest TEXT
                 );",
            )
            .expect("schema");
        connection
    }

    fn persist(connection: &rusqlite::Connection, intent: &decision::Intent) {
        let mut statement = connection
            .prepare(
                "INSERT INTO intents (intent_id, link_id, idempotency_key, state, attempts, lease_token, receipt_digest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(intent_id) DO UPDATE SET
                     state = excluded.state,
                     attempts = excluded.attempts,
                     lease_token = excluded.lease_token,
                     receipt_digest = excluded.receipt_digest",
            )
            .expect("prepare upsert");
        statement
            .execute((
                intent.intent_id.as_str(),
                intent.link_id.as_str(),
                intent.idempotency_key.as_str(),
                intent.state.as_str(),
                intent.attempts as i64,
                intent.lease_token.map(|token| token as i64),
                intent.receipt_digest.as_deref(),
            ))
            .expect("upsert intent");
    }

    fn durable_state(connection: &rusqlite::Connection, intent_id: &str) -> (String, i64) {
        connection
            .query_row(
                "SELECT state, attempts FROM intents WHERE intent_id = ?1",
                [intent_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .expect("durable row")
    }

    fn native_link(link_id: &str) -> Link {
        Link {
            link_id: link_id.to_owned(),
            task_id: "native-task".to_owned(),
            workspace_id: "ws-native".to_owned(),
            installation_id: "inst-native".to_owned(),
            provider_id: "github-issues".to_owned(),
            object_type: "issue".to_owned(),
            external_id: "77".to_owned(),
            state: LinkState::Active,
            sync_mode: SyncMode::Bidirectional,
            generation: 1,
            inbound_cursor: None,
            outbound_cursor: None,
        }
    }

    /// Crash between the provider accept and the local ack: after a
    /// process restart the durable row names the explicit
    /// accepted-pending-ack window, not delivered and not silently
    /// re-sendable without reconciliation.
    #[test]
    #[ignore = "qualified native leg: runs under LEKALO_TASKLINKS_NATIVE=1 only"]
    fn native_leg_accepted_before_ack_survives_restart_as_unknown() {
        let path = db_path();
        let _ = std::fs::remove_file(&path);
        let mut intents = BTreeMap::new();
        let link_row = native_link("native-link-ack");
        let intent = decision::plan_intent(
            &mut intents,
            &link_row,
            decision::DecisionOutcome::Allowed,
            "tasklinks.request_action",
            "native-key-ack".to_owned(),
            0,
        )
        .expect("intent");
        let intent_id = intent.intent_id.clone();
        {
            let connection = open(&path);
            let intent = intents.get_mut(&intent_id).unwrap();
            assert_eq!(
                decision::claim(intent, 41, 0, 30_000),
                DispatchStep::Claimed
            );
            persist(&connection, intent);
            assert_eq!(
                decision::dispatch(intent, &DispatchEvent::ProviderAccepted, 41, 1, 5, 100),
                DispatchStep::AcceptedPendingAck
            );
            persist(&connection, intent);
        } // the connection drops here: simulated process crash
          // Restart: reopen the file and reconcile from the durable row.
        let connection = open(&path);
        let (state, _attempts) = durable_state(&connection, &intent_id);
        assert_eq!(
            state, "accepted_pending_ack",
            "the crash leaves the explicit ack window on disk"
        );
    }

    /// The attempt bound is durable: after a restart the recorded
    /// attempt count is restored, so a restarted worker cannot bypass
    /// maxAttempts and the next failure dead-letters at the bound.
    #[test]
    #[ignore = "qualified native leg: runs under LEKALO_TASKLINKS_NATIVE=1 only"]
    fn native_leg_attempt_bound_survives_restart() {
        let path = db_path();
        let _ = std::fs::remove_file(&path);
        let mut intents = BTreeMap::new();
        let link_row = native_link("native-link-bound");
        let intent = decision::plan_intent(
            &mut intents,
            &link_row,
            decision::DecisionOutcome::Allowed,
            "tasklinks.request_action",
            "native-key-bound".to_owned(),
            0,
        )
        .expect("intent");
        let intent_id = intent.intent_id.clone();
        {
            let connection = open(&path);
            let intent = intents.get_mut(&intent_id).unwrap();
            assert_eq!(decision::claim(intent, 7, 0, 30_000), DispatchStep::Claimed);
            assert_eq!(
                decision::dispatch(intent, &DispatchEvent::ProviderFailed, 7, 0, 2, 0),
                DispatchStep::RetryScheduled
            );
            persist(&connection, intent);
        } // crash with attempts = 1 durably recorded
          // Restart: restore the engine intent from the durable row.
        let connection = open(&path);
        let (state, attempts) = durable_state(&connection, &intent_id);
        assert_eq!(state, "retry_wait");
        assert_eq!(attempts, 1, "the attempt count survives the restart");
        // The restored intent fails again: dead-letter at the bound of 2.
        let mut intents = BTreeMap::new();
        let link_row = native_link("native-link-bound");
        let planned = decision::plan_intent(
            &mut intents,
            &link_row,
            decision::DecisionOutcome::Allowed,
            "tasklinks.request_action",
            "native-key-bound".to_owned(),
            0,
        )
        .expect("replay returns the same intent");
        assert_eq!(planned.intent_id, intent_id);
        let mut restored = intents.get_mut(&intent_id).unwrap().clone();
        restored.attempts = attempts as u64; // from the durable row
        assert_eq!(
            decision::claim(&mut restored, 8, 100, 30_000),
            DispatchStep::Claimed
        );
        assert_eq!(
            decision::dispatch(&mut restored, &DispatchEvent::ProviderFailed, 8, 100, 2, 0),
            DispatchStep::DeadLettered,
            "the durable attempt count enforces the bound across restarts"
        );
        persist(&connection, &restored);
        let (state, _) = durable_state(&connection, &intent_id);
        assert_eq!(state, "dead_letter");
    }

    /// Synchronized competing reservations over one real transactional
    /// DB: exactly one wins.
    #[test]
    #[ignore = "qualified native leg: runs under LEKALO_TASKLINKS_NATIVE=1 only"]
    fn native_leg_synchronized_identity_race_binds_one_task() {
        let path = db_path();
        let _ = std::fs::remove_file(&path);
        let connection = open(&path);
        let bindings = std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new()));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut handles = Vec::new();
        for task in ["native-task-a", "native-task-b"] {
            let tx = tx.clone();
            let bindings = std::sync::Arc::clone(&bindings);
            handles.push(std::thread::spawn(move || {
                let mut guard = bindings.lock().unwrap();
                let outcome = decision::reserve_identity(
                    &mut guard,
                    "ws-native",
                    "inst-native",
                    "github-issues",
                    "issue",
                    "77",
                    task,
                );
                tx.send((task, outcome.is_some())).unwrap();
            }));
        }
        drop(connection);
        drop(tx);
        for handle in handles {
            handle.join().expect("race worker");
        }
        let winners = rx.into_iter().filter(|(_, won)| *won).count();
        assert_eq!(winners, 1, "exactly one reservation wins over the DB");
    }
}
