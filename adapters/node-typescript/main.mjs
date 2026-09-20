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

import { runIfEntry, __setLaunchExtensions } from "./src/kernel.mjs";
import { descriptor as zodDescriptor } from "./src/zod-gen.mjs";

// Issue #45: source mode registers the Zod generator exactly like the
// bundle tail does, so kernel-level tests exercise generate without the
// vendored compiler bundle. The scanner stays bundle-only: it requires
// the attached compiler.
__setLaunchExtensions([zodDescriptor]);

await runIfEntry(import.meta.url);
