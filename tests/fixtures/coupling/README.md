# Coupling synthetic conformance corpus

Planner is copied from the accepted context-budget workload. Taskhub and provider are authored semantic reference projections of the existing synthetic fixtures and production provider contract. They are not scanner output or measurements of the repository implementation.

| Reference | Searchable anchors | Bytes SHA-256 |
| --- | --- | --- |
| `tests/fixtures/context-budget/planner` | `planner.task / planner.focus_task` | directory copy, byte parity checked by gate |
| `tests/fixtures/pilot/taskhub/packages/tasks/src/index.ts` | `Task, TASK_STATE, TaskPage, NewTaskInput` | sha256:153889106a6821fa75e2ec6ea98cbfe318c842f78168ba28b032f96ab9d137e6 |
| `tests/fixtures/pilot/taskhub/packages/integrations/src/index.ts` | `IntegrationClient, CalendarSyncPayload, WebhookSyncPayload, DeliveryAck` | sha256:38298e7dcf3238e162f675c40e98dd9fde639e5d3561ad5647936a9e9f25834c |
| `tests/fixtures/pilot/taskhub/packages/sync/src/index.ts` | `SyncRequest, toPayload, syncEvent` | sha256:92ae674d710a895e233ce0c8a0b3cc56dc7e1c0a464bdf7f4af6c4a1d86168bc |
| `crates/lekalo-core/src/provider/operations.rs` | `OPERATIONS` | sha256:526c78bb5841c1c04874d22788fb2c5c00d4e3c64aef80fa6c1a227dbe0e0f8a |
| `contracts/provider-capabilities.schema.v0.6.4.json` | `provider manifest public boundary` | sha256:d8f3994ea190a306fd77c863106c054ce147605132b24cc31906da4287f47919 |

TaskPage.at is represented by an items reference to Task. SyncPayload union alternatives remain separate records because Model has no union type. Callable argument footprints use command inputs without declared writes. The local_request bridge is an explicit synthetic stress case. The normalized manifest has a shared operation input; it does not advertise coupling in the live provider. These limitations are intentional visible projections. transitionTask returns a copy and this corpus declares no shared mutable writes for it.

Evidence fixtures rebind current IR/graph/effect pins and pass each attachment owner's parser. Native trace rows are synthetic confirmed declarations; no native test or provider was executed. The fixture revision is the semantic digest used as an explicit current revision alias. Golden report/comparison/profile/evidence/change-input payloads are produced by the built CLI; invalid vectors test closed decoding and semantic joins. Regenerate deliberately with `node scripts/gen-coupling-fixtures.mjs`; gates remain read-only.
