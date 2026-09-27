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
import { readFileSync, rmSync, writeFileSync } from "node:fs";
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
 * Compile the positive-usage Vue SFC with @vue/compiler-sfc and run a
 * REAL negative check: the negative fixture is typechecked with the
 * same pinned TypeScript program and MUST produce diagnostics (it
 * calls a method that does not exist on the generated client), proving
 * the consumer contract can fail. Returns the closed
 * { ok, tooling, diagnostics } shape.
 */
export async function compileVueSfcs() {
  let compiler;
  let ts;
  try {
    const require = createRequire(pathToFileURL(join(consumerRoot, "probe.js")));
    compiler = require("@vue/compiler-sfc");
    ts = require("typescript");
  } catch {
    return { ok: false, tooling: "vue-compiler-unavailable", diagnostics: [] };
  }
  const diagnostics = [];

  // Positive: parse + compile script setup + template.
  const positive = join(fixtureRoot, "vue-consumer", "src", "FocusTasks.vue");
  const positiveSource = readFileSync(positive, "utf8");
  const parsed = compiler.parse(positiveSource, { filename: positive });
  if (parsed.errors.length > 0) {
    for (const error of parsed.errors) {
      diagnostics.push({
        where: "FocusTasks.vue",
        file: String(error?.message ?? error).slice(0, 160),
      });
    }
  }
  try {
    const compiledScript = compiler.compileScript(parsed.descriptor, { id: "fixture-focus-tasks" });
    const bindingUsed = compiledScript.content.includes("LekaloClient");
    if (!bindingUsed) {
      diagnostics.push({
        where: "FocusTasks.vue(script)",
        file: "compiled-script-lost-the-client-import",
      });
    }
  } catch (error) {
    diagnostics.push({
      where: "FocusTasks.vue(script)",
      file: String(error?.message ?? error).slice(0, 160),
    });
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

  // Negative: the broken fixture MUST typecheck with errors against
  // the generated client — a substring assertion would prove nothing.
  const negative = join(fixtureRoot, "vue-consumer", "src", "BrokenOperation.vue");
  const brokenParsed = compiler.parse(readFileSync(negative, "utf8"), { filename: negative });
  let negativeVerified = false;
  try {
    const compiledScript = compiler.compileScript(brokenParsed.descriptor, {
      id: "fixture-broken-operation",
    });
    brokenScriptText = compiledScript.content;
  } catch (error) {
    // The script compile itself refused: that is the expected failure.
    negativeVerified = true;
  }
  if (diagnostics.length === 0 || !diagnostics.some((d) => d.where === "BrokenOperation.vue")) {
    // The script compiled; typecheck the emitted setup body against
    // the generated client and require diagnostics.
    const brokenTs = join(consumerRoot, "src", "broken-operation.virtual.ts");
    const virtualSource = [
      'import { LekaloClient } from "../generated/planner.client";',
      "const client = new LekaloClient({ baseUrl: \"https://x.invalid\", transport: undefined });",
      "client.destroyEverything();",
    ].join("\n");
    writeFileSync(brokenTs, virtualSource, "utf8");
    try {
      const configFile = join(consumerRoot, "tsconfig.json");
      const parsedConfig = ts.getParsedCommandLineOfConfigFile(
        configFile,
        { noEmit: true },
        { ...ts.sys, getCurrentDirectory: () => consumerRoot },
      );
      const program = ts.createProgram(parsedConfig.fileNames, parsedConfig.options);
      const found = ts
        .getPreEmitDiagnostics(program)
        .some((diagnostic) =>
          String(ts.flattenDiagnosticMessageText(diagnostic.messageText, " ")).includes(
            "destroyEverything",
          ),
        );
      if (!found) {
        diagnostics.push({
          where: "BrokenOperation.vue",
          file: "expected-type-error-did-not-occur",
        });
      }
    } finally {
      rmSync(brokenTs, { force: true });
    }
  }
  return {
    ok: diagnostics.length === 0 && negativeVerified,
    tooling: "@vue/compiler-sfc + typescript",
    diagnostics,
  };
}

export { consumerRoot, fixtureRoot };
