# context capsule
- identity: dev.lekalo.context@1.0.0
- contract: lekalo/context/v1.0.0
- model version: 1.0.0
- mode: symbol
- project: planner
- roots: operation:planner.focus_task
- ir digest: sha256:7ffc4255fd8d1e3fa34cf14c4a1196199fd5fe32aed5c2fb7aee2075c6e7e88c
- estimator: dev.lekalo.estimator.chars-4@1.0.0
- estimator digest: sha256:e2517bff6268ffbb00f677f49ba75480502e05c3ed38a673d8919dbe9ef9ed79
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
