# Planner reference

## Purpose

Define the shared, synthetic HTTP planner behavior across maintained Node and Laravel examples. The reference profile is bounded; it is not an obligatory application architecture.

## Requirements

### Requirement: PLANNER-REQ-001 Owner-scoped focus

The application SHALL expose only planning rows belonging to the selected synthetic actor and workspace. It SHALL allow at most one active focus for that actor, including overlapping transactions.

#### Scenario: Concurrent focus
- **GIVEN** two planned tasks for one actor and workspace
- **WHEN** two independent HTTP transactions concurrently focus different tasks
- **THEN** exactly one succeeds, the other returns `focus_conflict`, and exactly one database row is focused

### Requirement: PLANNER-REQ-002 Planning and calendar queries

The application SHALL support plan, unplan, exact-permutation reorder, pause, Today, carry-over and backlog. Today SHALL be derived from the declared clock and actor timezone; carry-over SHALL preserve the original planned day.

#### Scenario: Local midnight
- **GIVEN** the clock is 2026-01-02T00:30:00Z and tasks exist on January 1 and January 2
- **WHEN** the actor reads Today in America/Los_Angeles
- **THEN** only the January 1 task is returned

### Requirement: PLANNER-REQ-003 Atomic replay and conflict

The application SHALL atomically persist each mutation with an actor/workspace-scoped idempotency receipt. Identical replay SHALL return the original response; changed input under the same key SHALL return a conflict. A rolled-back request SHALL leave neither effects nor a receipt. Reorder SHALL reject stale versions and incomplete or duplicate permutations without partial writes.

#### Scenario: Rollback and retry
- **GIVEN** test-only rollback injection is enabled
- **WHEN** a plan mutation fails after its write but before commit
- **THEN** the planning row and receipt are absent, and a retry with the same key can succeed
