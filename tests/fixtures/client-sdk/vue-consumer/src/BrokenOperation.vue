<script setup lang="ts">
// The negative fixture: calls a method that does not exist on the
// generated client. The consumer contract must be able to fail — the
// harness asserts this file produces a compile error, never a pass.
import { LekaloClient } from "../generated/planner.client";

const client = new LekaloClient({ baseUrl: "https://x.invalid", transport: undefined });
void client.destroyEverything();
</script>

<template>
  <section>this file must not compile</section>
</template>
