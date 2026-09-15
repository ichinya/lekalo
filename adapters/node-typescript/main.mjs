#!/usr/bin/env node
/**
 * Source-mode entry: the #43/#44 kernel run from the repository without
 * the vendored compiler bundle. Behavior, identity, and the protocol
 * surface are exactly the bundled artifact's minus scanner operations;
 * the scanner bundle (built by build.mjs) re-exports this kernel with
 * the vendored compiler attached.
 *
 * The one-shot main only runs when this file is the process entry; the
 * module identity (adapter digest, --version-json probe) resolves to
 * the kernel source itself.
 *
 * The `isMain` probe mirrors src/kernel.mjs: argv[1] is compared to this
 * exact module URL, so test runners and importers never trigger the
 * stdin read.
 */
import { pathToFileURL } from "node:url";

export * from "./src/kernel.mjs";
export { compilerMetadata, __setCompilerMetadata } from "./src/kernel.mjs";

import { main } from "./src/kernel.mjs";

const isMain = (() => {
  if (typeof process === "undefined" || !process.argv?.[1]) {
    return false;
  }
  try {
    return import.meta.url === pathToFileURL(process.argv[1]).href;
  } catch {
    return false;
  }
})();

if (isMain) {
  await main();
}
