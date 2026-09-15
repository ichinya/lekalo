#!/usr/bin/env node
/**
 * Source-mode entry: the #43/#44 kernel run from the repository without
 * the vendored compiler bundle. Behavior, identity, and the protocol
 * surface are exactly the bundled artifact's minus scanner operations;
 * the scanner bundle (built by build.mjs) re-exports this kernel with
 * the vendored compiler attached.
 *
 * The one-shot main only runs when this file is the process entry;
 * `runIfEntry` compares module URLs, so test runners and importers never
 * trigger the stdin read.
 */
export * from "./src/kernel.mjs";
export { compilerMetadata, __setCompilerMetadata } from "./src/kernel.mjs";

import { runIfEntry } from "./src/kernel.mjs";

await runIfEntry(import.meta.url);
