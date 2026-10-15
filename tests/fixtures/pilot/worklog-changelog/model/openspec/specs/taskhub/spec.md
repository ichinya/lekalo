# Taskhub Specification

## Purpose

Keep task completion, private worklogs, provider comments, and the
opt-in public changelog independent, auditable, and safely publishable.

## Requirements

### Requirement: Complete task locally

The system SHALL let a workspace member complete one task through an
explicit local completion operation, and SHALL NOT enable any worklog,
provider comment, or changelog effect as an implicit consequence of
that completion.

### Requirement: Private worklog

The system SHALL let the task author create and edit a private worklog
record scoped to one tenant and workspace, and SHALL keep that record
invisible to public projections.

### Requirement: Explicit provider comment

The system SHALL create a provider comment request only through an
explicit user selection that carries a correlation key, SHALL
deduplicate replayed requests, SHALL conflict on the same key with a
different payload, and SHALL record provider delivery as an
independent outcome with its own commit boundary.

### Requirement: Opt-in changelog draft

The system SHALL create a changelog entry only through an explicit
opt-in draft operation, and SHALL NOT create drafts, snapshots, or
publish effects as an implicit consequence of completion, worklog, or
provider paths.

### Requirement: Gated publication

The system SHALL publish a changelog entry only when a fresh approval
binds the exact canonical preview bytes, revision, tenant,
destination, publishing authority, and policy revision, and SHALL
refuse publication when the approval is missing, stale, or foreign.

### Requirement: Immutable snapshot

The system SHALL store each published entry as an append-only
immutable snapshot revision with a content digest and snapshot schema
version, SHALL keep stored bytes unchanged when source worklogs or
drafts are edited, and SHALL create a new revision on republish.

### Requirement: Withdraw keeps audit

The system SHALL hide a withdrawn entry from the public projection on
authorized withdrawal, SHALL keep the immutable history and the
lifecycle audit under restricted access, and SHALL never let a delayed
publish worker resurrect a withdrawn entry.

### Requirement: Public projection safety

The public endpoint SHALL serve only the allowlisted snapshot fields
through the public DTO, SHALL exclude private worklog bodies, internal
identifiers, provider and correlation metadata, and restricted audit,
and SHALL apply one shared preview and final rendering policy that
rejects script content, unsafe or encoded URL schemes, and control
characters.
