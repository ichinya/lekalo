# Planner Specification

## Purpose

Keep the daily plan explicit, ordered and owned by its user.

## Requirements

### Requirement: Plan the day

The planner SHALL let a workspace user plan an existing task into one
calendar day with a position, and SHALL refuse a second planning row
for the same workspace, user and task. The today board SHALL list the
planning rows of the actor-local calendar day, and the stored date of
a row SHALL never change because a read rendered it.

### Requirement: Move and unplan

The planner SHALL move a planned task to an explicitly named day and
position, and SHALL remove a planned task from the plan, both scoped
to the owning workspace and user. A planning row that does not belong
to the actor's workspace and user SHALL be invisible and never
removable.

### Requirement: Pause focus

The planner SHALL stamp the pause time of one planned task from the
declared clock at the moment of the command.

### Requirement: Carry over

The planner SHALL serve planning rows of earlier days as carry-over at
their original calendar day. A carry-over read SHALL NOT rewrite the
stored planned_for of any row.

### Requirement: Optimistic reorder

The planner SHALL reorder one planned day under an optimistic reorder
version: the command SHALL carry the expected version, SHALL refuse
the whole day with a declared conflict when it no longer matches, and
SHALL persist the new order and the next version atomically.

### Requirement: Complete a planned task

The planner SHALL let a workspace user complete one planned task from
its planning row, SHALL stamp the completion time from the declared
clock at the moment of the command, and SHALL refuse a second
completion with a declared conflict. The planner SHALL serve the
completed rows of the actor as one query projection, newest day first,
without rewriting any stored field of the completed row.

### Requirement: The planning screen renders the API projections

The planning screen SHALL render its buckets (today, backlog, focus,
completed) and its task card and detail drawer strictly from the
declared query projections and their typed client contract. Every
rendered field SHALL trace to one API type field; no screen-local
copy of a server type exists. Action availability SHALL derive from
the declared policy and capability contracts — an action whose gate
denies, or whose precondition error is declared, renders disabled or
hidden with the machine-readable reason, never hardcoded availability.

### Requirement: Screen states stay traceable to API symbols

The screen SHALL express loading, empty, degraded, conflict and
read-only as machine-readable states bound to declared API symbols:
loading to the in-flight query, empty to an empty declared list,
degraded to the declared infrastructure failure, conflict to a
declared conflict-category error, and read-only to the declared
authorization denial. Every state transition SHALL be explainable by
one resolved API symbol; a state with no API symbol is a documented
gap, not an invention.

### Requirement: Optimistic updates roll back on declared conflicts

The screen MAY apply an optimistic update only when the command's
rollback contract is machine-readable: the affected projection, the
declared conflict errors, and the restored snapshot rule are declared
before the request. A declared conflict SHALL roll the affected card
or order back to its last consistent rendering and surface the typed
error identity; a non-declared failure SHALL surface the transport
failure honestly without inventing a domain error.

### Requirement: Dates stay actor-local

The screen SHALL treat `planned_for` as the actor-local calendar day:
day input, bucket membership and carry-over rendering SHALL use the
actor's declared timezone, and a timestamp SHALL render in the
actor's zone without ever rewriting the stored value. A read SHALL
never shift a stored date; midnight-boundary renders split
explicitly, never silently.

### Requirement: Accessibility expectations ride the UI tests

The screen SHALL carry its accessibility expectations as OpenSpec
UI-test requirements bound to the same scenarios as the behavior:
announced state changes (loading, empty, degraded, conflict),
keyboard-reachable actions whose availability renders as an
accessible name with its disabled reason, and focus management on
drawer open/close. Accessibility lives in the requirement and
scenario layer; no UI DSL is required to keep it.

### Requirement: Ownership scope

The planner SHALL scope every planning command and query to the
actor's declared workspace and user, SHALL deny cross-tenant access
through the declared policy, and SHALL keep provider-owned task data
separate from the local planning fields: provider events SHALL NOT
overwrite planned_for, position, focused_at, paused_at or
reorder_version.
