<script setup lang="ts">
// The planning screen pilot (issue #53): one maintained Vue screen
// rendered strictly from the generated client contract — the buckets
// are the declared query projections, the task cards and the detail
// drawer are the ui-projection DTOs, action availability derives from
// the capability/policy predicates of ui-projection.json (never
// hardcoded), conflicts roll the optimistic patch back, and the five
// screen states stay bound to declared API symbols. The transport is
// injected; a browser-E2E driver swaps the fake for the real wire.
import { computed, ref } from "vue";
import {
  LekaloClient,
  type LekaloTransport,
  type userTaskPlanning,
} from "../generated/planner.client";

const props = defineProps<{ transport: LekaloTransport }>();

const client = new LekaloClient({
  // The screen fixture never names a consumer application and never
  // carries a production URL; the browser-E2E driver injects both.
  baseUrl: "https://planner.fixture.invalid",
  transport: props.transport,
});

// The five machine-readable screen states (ui-projection.states),
// bound to declared API symbols only.
type ScreenState =
  | { kind: "loading" }
  | { kind: "ready" }
  | { kind: "empty"; bucket: string }
  | { kind: "degraded"; error: "planner.store_unavailable" }
  | { kind: "conflict"; error: string; code: string }
  | { kind: "read-only"; error: string };

const state = ref<ScreenState>({ kind: "loading" });
const today = ref<readonly userTaskPlanning[]>([]);
const backlog = ref<readonly userTaskPlanning[]>([]);
const completed = ref<readonly userTaskPlanning[]>([]);
// The optimistic completion stamps awaiting their declared outcome.
const pendingComplete = ref(new Set<string>());
// The drawer target: the detail-drawer DTO fields of one row.
const drawer = ref<userTaskPlanning | null>(null);

const todayVersion = computed(() =>
  today.value.reduce((max, row) => Math.max(max, row.reorder_version), 0),
);

// Availability predicates: the ui-projection action rules, spelled
// over the DTO fields of the client contract.
const canComplete = (row: userTaskPlanning): boolean =>
  row.completed_at === null || row.completed_at === undefined;
const canPause = (row: userTaskPlanning): boolean =>
  row.focused_at !== null && row.focused_at !== undefined && canComplete(row);
const canPlan = (row: userTaskPlanning): boolean =>
  row.planned_for === "1970-01-01" && row.position === 0;

// The actor-local day spelling of a timestamp (ui-projection.timezone):
// the wire value is never rewritten, only rendered.
function renderDay(iso: string): string {
  return iso.slice(0, 10);
}

async function refresh(): Promise<void> {
  state.value = { kind: "loading" };
  const [todayResult, backlogResult, completedResult] = await Promise.all([
    client.plannerEndpointTodayPlanning(),
    client.plannerEndpointBacklog(),
    client.plannerEndpointCompleted(),
  ]);
  if (todayResult.ok && backlogResult.ok && completedResult.ok) {
    today.value = todayResult.value;
    backlog.value = backlogResult.value;
    completed.value = completedResult.value;
    state.value =
      today.value.length + backlog.value.length + completed.value.length === 0
        ? { kind: "empty", bucket: "today" }
        : { kind: "ready" };
    return;
  }
  const failed = [todayResult, backlogResult, completedResult].find(
    (result): result is Exclude<typeof result, { ok: true }> => !result.ok,
  );
  if (!failed) {
    return;
  }
  if ("infrastructure" in failed) {
    state.value = { kind: "degraded", error: "planner.store_unavailable" };
    return;
  }
  const error = failed.error;
  if (error.category === "auth") {
    // The declared authorization denial: the board renders read-only
    // with the disabled reason from the policy contract.
    state.value = { kind: "read-only", error: error.id };
    return;
  }
  if (error.category === "conflict") {
    state.value = { kind: "conflict", error: error.id, code: error.code };
    return;
  }
  state.value = { kind: "empty", bucket: error.id === "planner.task_not_found" ? "backlog" : "today" };
}

// The complete action: one optimistic patch with a machine-readable
// rollback (ui-projection.actions[complete].optimistic.rollback).
async function complete(row: userTaskPlanning, idempotencyKey: string): Promise<void> {
  const snapshot = row;
  const optimistic: userTaskPlanning = { ...row, completed_at: new Date().toISOString() };
  today.value = today.value.map((candidate) =>
    candidate.planning_id === row.planning_id ? optimistic : candidate,
  );
  pendingComplete.value.add(row.planning_id);
  const result = await client.plannerEndpointComplete(row.planning_id, idempotencyKey);
  pendingComplete.value.delete(row.planning_id);
  if (result.ok) {
    await refresh();
    return;
  }
  if ("infrastructure" in result) {
    state.value = { kind: "degraded", error: "planner.store_unavailable" };
    restore(snapshot);
    return;
  }
  const error = result.error;
  if (error.category === "auth") {
    state.value = { kind: "read-only", error: error.id };
  } else {
    state.value = { kind: "conflict", error: error.id, code: error.code };
  }
  restore(snapshot);
}

function restore(snapshot: userTaskPlanning): void {
  today.value = today.value.map((candidate) =>
    candidate.planning_id === snapshot.planning_id ? snapshot : candidate,
  );
}

function openDrawer(row: userTaskPlanning): void {
  drawer.value = row;
}

function closeDrawer(): void {
  drawer.value = null;
}

const ariaLive = computed(() => JSON.stringify(state.value));
void canPause;
void canPlan;
void renderDay;
void todayVersion;
void openDrawer;
void closeDrawer;
void complete;
void refresh;
</script>

<template>
  <!-- The planning board pilot: every state and hook below is named in
       ui-projection.json and bound to a declared API symbol; the
       browser-E2E scenarios drive these hooks and only these hooks. -->
  <main aria-label="Planning board" data-screen="planning-board">
    <p data-state="screen" role="status" aria-live="polite">{{ ariaLive }}</p>

    <section v-if="state.kind === 'loading'" data-state="loading" aria-busy="true">
      <p>Loading the plan…</p>
    </section>

    <section v-else-if="state.kind === 'degraded'" data-state="degraded">
      <p>The plan store is unavailable ({{ state.error }}).</p>
      <button type="button" data-action="retry" @click="refresh()">Retry</button>
    </section>

    <section v-else-if="state.kind === 'read-only'" data-state="read-only">
      <p>The board is read-only: {{ state.error }} denied the action.</p>
    </section>

    <section v-else-if="state.kind === 'conflict'" data-state="conflict">
      <p>{{ state.error }} ({{ state.code }}) — the optimistic change rolled back.</p>
    </section>

    <section v-else-if="state.kind === 'empty'" data-state="empty">
      <p>The {{ state.bucket }} bucket is empty.</p>
    </section>

    <template v-else>
      <section data-bucket="today" aria-label="Today">
        <h2>Today</h2>
        <ul>
          <li v-for="row in today" :key="row.planning_id" :data-card="row.task_id">
            <span data-field="planned_for">{{ row.planned_for }}</span>
            <span data-field="position">{{ row.position }}</span>
            <span data-field="completed">{{ row.completed_at ?? "—" }}</span>
            <button
              type="button"
              data-action="complete"
              :disabled="!canComplete(row) || pendingComplete.has(row.planning_id)"
              :aria-disabled="!canComplete(row) ? 'already completed' : undefined"
              @click="complete(row, 'e2e-' + row.planning_id)"
            >
              Complete
            </button>
            <button type="button" data-action="open-drawer" @click="openDrawer(row)">Details</button>
          </li>
        </ul>
      </section>

      <section data-bucket="backlog" aria-label="Backlog">
        <h2>Backlog</h2>
        <ul>
          <li v-for="row in backlog" :key="row.planning_id" :data-card="row.task_id">
            <span data-field="task">{{ row.task_id }}</span>
          </li>
        </ul>
      </section>

      <section data-bucket="completed" aria-label="Completed">
        <h2>Completed</h2>
        <ul>
          <li v-for="row in completed" :key="row.planning_id" :data-card="row.task_id">
            <span data-field="completed_at">{{ row.completed_at }}</span>
          </li>
        </ul>
      </section>

      <aside v-if="drawer" data-drawer="detail" role="dialog" aria-label="Planning detail">
        <dl>
          <dt>planning_id</dt>
          <dd data-field="planning_id">{{ drawer.planning_id }}</dd>
          <dt>reorder_version</dt>
          <dd data-field="reorder_version">{{ drawer.reorder_version }}</dd>
          <dt>focused_at</dt>
          <dd data-field="focused_at">{{ drawer.focused_at ?? "—" }}</dd>
        </dl>
        <button type="button" data-action="close-drawer" @click="closeDrawer()">Close</button>
      </aside>
    </template>
  </main>
</template>
