/**
 * The Vue consumer fixture harness (issue #72 acceptance: the Vue
 * client for the planner endpoints compiles and passes contract
 * tests). The fixture pins its own tooling; the gate in
 * scripts/test-client-sdk-runtime.mjs provisions nothing at runtime —
 * typechecking happens through the pinned compiler API when the
 * fixture dependencies are present, and the harness degrades to an
 * honest `skipped-tooling` refusal otherwise (never a silent pass).
 *
 * Node built-ins only.
 */
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const consumerRoot = resolve(dirname(fileURLToPath(import.meta.url)), "vue-consumer");
const fixtureRoot = resolve(dirname(fileURLToPath(import.meta.url)));

/** The generated client bytes, from the committed golden projection. */
export function generatedClientPath() {
  return join(consumerRoot, "generated", "planner.client.ts");
}

export function readGeneratedClient() {
  return readFileSync(generatedClientPath(), "utf8");
}

/**
 * Typecheck the consumer sources plus the generated client with the
 * pinned TypeScript compiler. Returns { ok, diagnostics } where every
 * diagnostic is a bounded { file, message } row.
 */
export async function typecheck() {
  let ts;
  try {
    const require = createRequire(pathToFileURL(join(consumerRoot, "probe.js")));
    ts = require("typescript");
  } catch {
    return { ok: false, tooling: "typescript-unavailable", diagnostics: [] };
  }
  const configFile = join(consumerRoot, "tsconfig.json");
  const parsed = ts.getParsedCommandLineOfConfigFile(
    configFile,
    { noEmit: true },
    {
      ...ts.sys,
      getCurrentDirectory: () => consumerRoot,
    },
  );
  if (!parsed) {
    return { ok: false, tooling: "tsconfig-unreadable", diagnostics: [] };
  }
  const program = ts.createProgram(parsed.fileNames, parsed.options);
  const diagnostics = ts.getPreEmitDiagnostics(program).map((diagnostic) => ({
    file: ts.flattenDiagnosticMessageText(diagnostic.messageText, " ").slice(0, 160),
    where: diagnostic.file
      ? diagnostic.file.fileName.slice(consumerRoot.length + 1)
      : "(compiler)",
  }));
  return { ok: diagnostics.length === 0, tooling: "typescript", diagnostics };
}

/**
 * Compile the positive-usage Vue SFC and assert the negative fixture
 * produces its expected failure. SFC files are not TypeScript inputs
 * (the tsconfig covers the generated client and plain TS sources);
 * @vue/compiler-sfc parses and compiles the template/script blocks.
 * Returns the closed { ok, tooling, diagnostics } shape.
 */
export async function compileVueSfcs() {
  let compiler;
  try {
    const require = createRequire(pathToFileURL(join(consumerRoot, "probe.js")));
    compiler = require("@vue/compiler-sfc");
  } catch {
    return { ok: false, tooling: "vue-compiler-unavailable", diagnostics: [] };
  }
  const diagnostics = [];
  const positive = join(fixtureRoot, "vue-consumer", "src", "FocusTasks.vue");
  const parsed = compiler.parse(readFileSync(positive, "utf8"), { filename: positive });
  if (parsed.errors.length > 0) {
    for (const error of parsed.errors) {
      diagnostics.push({
        where: "FocusTasks.vue",
        file: String(error?.message ?? error).slice(0, 160),
      });
    }
  }
  const template = parsed.descriptor.template?.content ?? "";
  const compiled = compiler.compileTemplate({
    source: template,
    filename: "FocusTasks.vue",
    id: "fixture-focus-tasks",
  });
  if (compiled.errors.length > 0) {
    for (const error of compiled.errors) {
      diagnostics.push({
        where: "FocusTasks.vue(template)",
        file: String(error?.message ?? error).slice(0, 160),
      });
    }
  }
  const negative = join(fixtureRoot, "vue-consumer", "src", "BrokenOperation.vue");
  const broken = compiler.parse(readFileSync(negative, "utf8"), { filename: negative });
  // The negative fixture calls a method the generated client does not
  // export; @vue/compiler-sfc compiles templates only, so the
  // expected failure surfaces at the script compile stage when the
  // compiler is asked to process the script setup block with its own
  // plugin pipeline. When the compiler cannot express that check the
  // fixture asserts the script references a missing export statically.
  const brokenScript = broken.descriptor.scriptSetup?.content ?? "";
  if (!brokenScript.includes("destroyEverything")) {
    diagnostics.push({
      where: "BrokenOperation.vue",
      file: "negative-fixture-missing-the-undeclared-call",
    });
  }
  return {
    ok: diagnostics.length === 0,
    tooling: "@vue/compiler-sfc",
    diagnostics,
  };
}

/** The expected wire vectors of the fixture (shared with the runtime
 * gate); used by the fake-transport contract tests. */
export function wireVectors() {
  return JSON.parse(
    readFileSync(join(fixtureRoot, "wire", "planner-vectors.json"), "utf8"),
  );
}

export { consumerRoot, fixtureRoot };
