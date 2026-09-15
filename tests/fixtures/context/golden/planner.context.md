# context capsule
- identity: dev.lekalo.context@0.2.16
- contract: lekalo/context/v0.2.16
- model version: 0.2.16
- mode: symbol
- project: planner
- roots: operation:planner.focus_task
- ir digest: sha256:2ed11578a45ee61635c51cc4ad5ff758db3bc2a23a867f0c70547c75188df8f7
- estimator: dev.lekalo.estimator.chars-4@0.2.16
- estimator digest: sha256:602e648c2ff7c58cace92876f6c834c5c1735ce594e3ba565d7b012f752fb0be
- budget: limit 5000, estimated 262, minimum required 262, fits true
- coverage: 13 candidates, 13 included, 0 excluded
- complete: true

## symbol
- operation:planner.focus_task (operation)
  subkind: command
  module: planner
  version: 1
  derived from: PLANNER-REQ-001
  description: Focus one task
  input task_id: planner.task_id (required)
  effects: planner.create_task

## policies
- policy:planner.deny_bulk_focus -> deny
  applies to: planner.focus_task
  description: Bulk focus is out of scope

## effects
- create operation:planner.focus_task -> canonical:planner.task (canonical)
- emit-event operation:planner.focus_task -> event:planner.task_focused (canonical)

## dependencies
- accepts operation:planner.focus_task -> type:planner.task_id (canonical)
- derived_from operation:planner.focus_task -> requirement:PLANNER-REQ-001 (canonical)
- references operation:planner.focus_task -> effect:planner.create_task (canonical)

## scenarios
- scenario:planner.focus_flow: Focusing a task emits the focused event.
  covers: planner.focus_task, planner.task_focused

## public-impact
- exposes endpoint:planner.api_focus -> operation:planner.focus_task (canonical)

## bindings
- target-binding:planner.binding_node -> node-typescript

## types
- type:planner.task_id (type)
  version: 1
  description: Stable task identifier
  base: uuid

## closure
- emits effect:planner.create_task -> event:planner.task_focused (canonical)
- references effect:planner.create_task -> entity:planner.task (canonical)

## gaps
- detected-effects-absent
- error-contracts-unrepresentable
