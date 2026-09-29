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

### Requirement: Ownership scope

The planner SHALL scope every planning command and query to the
actor's declared workspace and user, SHALL deny cross-tenant access
through the declared policy, and SHALL keep provider-owned task data
separate from the local planning fields: provider events SHALL NOT
overwrite planned_for, position, focused_at, paused_at or
reorder_version.
