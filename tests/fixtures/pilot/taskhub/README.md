# taskhub

A synthetic pnpm-workspace consumer used by the issue #49 pilot: the
first end-to-end proof of observed mode over realistic existing code.
Every package, symbol, and credential in this tree is fictional.

## Architecture

- `packages/tasks` — the task domain: the `Task` record, its
  `TASK_STATE` lifecycle (`backlog` / `focused` / `done`), and the
  domain functions (`createTask`, `listTasks`, `transitionTask`).
- `packages/events` — the external-task event envelope: `TaskEvent`
  carries `id`, `kind`, `occurredAt` (transport time as a string), and
  a typed `payload` union, plus the envelope codec functions.
- `packages/integrations` — the integration contracts: calendar and
  webhook sync payloads and the narrow `IntegrationClient` interface
  every delivery target implements.
- `packages/sync` — the worker flow: consumes `TaskEvent`s and calls
  the integration client (`toPayload`, `syncEvent`).
- `apps/api` — the minimal HTTP surface: a framework-free route table
  (`ROUTES`) and the `handle` dispatch function, documented by the
  committed `openapi.json`.

Dependency direction: `sync` and `api` consume `tasks`/`events`;
`sync` additionally consumes `integrations`. The `tasks`/`events`/
`integrations` packages are leaves.

## Fixture discipline

The workspace scripts delegate to pnpm (`pnpm -r build` and friends)
and every package mirrors the usual `build`/`typecheck`/`test` script
surface, but nothing in the Lekalo flow ever executes them: gates
assert the confirmed command surface, never execution fidelity. The
`.env` file holds a fake secret and is never read by any Lekalo
surface; `.env.example` documents the shape.
