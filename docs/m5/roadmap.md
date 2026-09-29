# The greenfield pilot roadmap (M5)

The first contracted consumer pilot targets **PHP + Laravel (modular
monolith) on the backend and Vue 3 + TypeScript + Vite on the
frontend**, with an explicit HTTP/JSON API + OpenAPI transport and
PostgreSQL storage. This is the primary greenfield pilot path: new
vertical slices start on it.

The existing experimental Node.js/TypeScript implementation is **not**
removed. It stays the **observed baseline**: the source of the pinned
event/API contracts, the integration protocol, and the control
implementation the equivalence gate compares against. It does not
define the new backend's architecture.

## The architecture line

```text
OpenSpec requirements
  -> Lekalo Model (language-independent; neutrality-audited)
  -> Laravel command/query/policy/storage bindings (contracted mode)
  -> HTTP/OpenAPI projection (#46)
  -> generated/checked TypeScript client (#72)
  -> maintained Vue screen (checked projection, ADR-0046)
  -> Mago / Laratesto / native gates
  -> HLV evidence through AIFHub Extension
```

API, queue worker, and scheduler are process roles of one Laravel
application; the frontend is a separate Vue application; provider
adapters stay external services. The semantic model never grows
Laravel, Eloquent, Vue, or TypeScript concepts — the acceptance gate is
mechanical (`scripts/test-planner-model-neutrality.mjs`).

## Status: what already stands (issue #114)

| Path leg | State | Where to look |
| --- | --- | --- |
| Node observed baseline pinned | landed | `tests/fixtures/pilot/observed-baseline/`, gate `scripts/test-observed-baseline.mjs` |
| Model neutrality audited | landed | `scripts/test-planner-model-neutrality.mjs`, evidence `tests/fixtures/php-laravel/routes/evidence/model-neutrality.audit.json` |
| Laravel command/query/policy/storage bindings | landed (#50/#59) | `scripts/test-php-laravel-operations.mjs` (13 stages) |
| HTTP/OpenAPI projection + runtime battery | landed (#50/#60) | `scripts/test-php-laravel-routes.mjs` (9 stages) |
| Generated/checked TS client + maintained Vue screen | landed (#53/#72) | `scripts/test-php-laravel-ui.mjs` (6 stages) |
| Mago gate | landed (#55; real toolchain checksum-pinned) | `scripts/test-mago-integration.mjs` |
| Composer/Laravel native gates | landed (#61) | `scripts/test-php-laravel-native-gates.mjs` |
| Portable scenarios: authorization, idempotency, transaction, concurrent focus | landed (#114) | six-scenario corpus, `scripts/test-php-laravel-scenario-tests.mjs` |
| Neutral Node ↔ Laravel equivalence + removal-gate report | landed (#114) | `scripts/test-php-laravel-parity.mjs`, `docs/m5/issue-114-equivalence-report.md` |
| One-command pilot path | landed (#114) | `scripts/test-pilot-laravel-vue.mjs` |

Walk the whole path with one command:

```sh
node scripts/test-pilot-laravel-vue.mjs
```

## Transition plan (issue #114, plan steps → state)

1. **Pin the Node baselines** — done; the baseline fixture pins the
   event envelope, the `lekalo.target/v1@0.3.2` integration protocol,
   the planner OpenAPI documents, and the six-scenario behavior rows,
   digest-pinned and re-verified against live bytes.
2. **Model without target concepts** — done since #50/#53 for the
   planning family; #114 makes the property a standing gate with
   committed evidence.
3. **Laravel target profile and fixture** — done (#50: the fixture app,
   the maintained boundary under `App\Lekalo`).
4. **The daily-planning module as maintained Laravel code** — done
   (#50/#53: one port body + one binding line per governed operation).
5. **OpenAPI and the Vue TypeScript client** — done (#46/#72/#53: one
   join, five projections, byte-checked client, strict screen
   typecheck).
6. **Portable scenarios through Laratesto/native tests** — done (#56
   harness; #114 grew the corpus to authorization + transaction legs).
7. **Semantic outcomes and equivalence vs the Node baseline** — done
   (#56 comparison machinery; #114 evidence + the written gate).
8. **Decide old-backend removal only after parity** — the standing
   rule; the gate is `docs/m5/issue-114-equivalence-report.md` §4. No
   removal is proposed: the Node baseline stays, and the report lists
   exactly what a future removal claim must show.

## Next slices (post-#114 candidates)

- Grow the portable corpus over the planning family
  (plan/move/unplan/reorder/pause/complete + queries) — mechanical,
  template exists.
- A race evaluator for the concurrent-focus leg (both backends must
  answer it or diverge visibly).
- Close the requirements-trace gate gap (`completeness: partial`) when
  the release-verification lanes land.
- PostgreSQL execution leg of the migration pipeline beyond the
  recorded skip (#57).
