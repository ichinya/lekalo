<script setup lang="ts">
// The positive-usage consumer: imports the generated planner client
// types and the result union, drives a fake transport, and renders
// the decoded planner task rows. The list-typed list_tasks call
// decodes to `task[]` directly — array methods work on the typed
// value with no casts (round 2). Typechecked strict by the fixture
// harness; compiled by @vue/compiler-sfc in the contract gate.
import { computed, ref } from "vue";
import {
  LekaloClient,
  type LekaloTransport,
  type LekaloResult,
  type task,
} from "../generated/planner.client";

const fakeTransport: LekaloTransport = {
  async send(request) {
    void request;
    return {
      status: 200,
      headers: {},
      body: JSON.stringify([
        {
          task_id: "0b6e3d4e-8f2a-4c31-9d5f-2a7b8c9d0e1f",
          tenant: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
          title: "Ship the client SDK",
          state: "focused",
          due: null,
        },
      ]),
    };
  },
};

const client = new LekaloClient({
  baseUrl: "https://planner.internal.example",
  transport: fakeTransport,
});

const rows = ref<readonly task[]>([]);
const loading = ref(false);

async function refresh(): Promise<void> {
  loading.value = true;
  const result: LekaloResult<task[]> = await client.listTasks();
  loading.value = false;
  if (result.ok) {
    // The decoded value IS the declared array: no cast, and array
    // methods typecheck against the projected element type.
    rows.value = result.value.map((row) => row);
  }
}

const titles = computed(() => rows.value.map((row) => row.title));
const firstState = computed(() => rows.value[0]?.state ?? "backlog");
void refresh;
void firstState;
</script>

<template>
  <section aria-label="Planner tasks">
    <p v-if="loading">Loading…</p>
    <ul v-else>
      <li v-for="title in titles" :key="title">{{ title }}</li>
    </ul>
  </section>
</template>
