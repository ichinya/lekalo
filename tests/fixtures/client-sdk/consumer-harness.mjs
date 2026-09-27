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
 * Compile BOTH consumer SFCs with @vue/compiler-sfc and feed the
 * compiled script-setup blocks into the strict TypeScript program
 * (round 2): the SFC code that actually calls the generated client is
 * typechecked, not just the client module. The negative fixture MUST
 * produce real diagnostics (it calls a method that does not exist on
 * the generated client), proving the consumer contract can fail.
 * Returns the closed { ok, tooling, diagnostics } shape.
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

  // Compile both script-setup blocks to virtual .ts siblings. Writing
  // them beside the SFC keeps the SFC's relative import specifiers
  // ("../generated/planner.client") resolving to the generated client.
  const virtualFiles = [];
  const compileOne = (name, id) => {
    const sfcPath = join(fixtureRoot, "vue-consumer", "src", name);
    const parsed = compiler.parse(readFileSync(sfcPath, "utf8"), { filename: sfcPath });
    if (parsed.errors.length > 0) {
      return {
        name,
        virtual: null,
        parseErrors: parsed.errors.map((error) => ({
          where: name,
          file: String(error?.message ?? error).slice(0, 160),
        })),
      };
    }
    const compiled = compiler.compileScript(parsed.descriptor, { id });
    const virtualPath = join(consumerRoot, "src", name + ".ts");
    writeFileSync(virtualPath, compiled.content, "utf8");
    return { name, virtual: virtualPath, parseErrors: [] };
  };

  let positive;
  let negative;
  try {
    positive = compileOne("FocusTasks.vue", "fixture-focus-tasks");
    negative = compileOne("BrokenOperation.vue", "fixture-broken-operation");
  } catch (error) {
    return {
      ok: false,
      tooling: "@vue/compiler-sfc",
      diagnostics: [{ where: "(compileScript)", file: String(error?.message ?? error).slice(0, 160) }],
    };
  }

  try {
    // The strict program includes the compiled SFC scripts (the
    // tsconfig's src/**/*.ts include picks the virtual files up).
    const check = await typecheck();
    if (check.tooling === "typescript-unavailable") {
      return { ok: false, tooling: "typescript-unavailable", diagnostics: [] };
    }
    const byFile = (fragment) =>
      check.diagnostics.filter((diagnostic) => diagnostic.where.includes(fragment));

    // Positive: the FocusTasks script (the real client usage) must be clean.
    const positiveErrors = byFile("FocusTasks.vue.ts").concat(positive.parseErrors);
    if (positiveErrors.length > 0) {
      return { ok: false, tooling: check.tooling, diagnostics: positiveErrors };
    }

    // Negative: the broken script must produce a real type error —
    // destroyEverything does not exist on LekaloClient.
    const negativeErrors = byFile("BrokenOperation.vue.ts").concat(negative.parseErrors);
    const realNegative = negativeErrors.some((diagnostic) =>
      diagnostic.file.includes("destroyEverything"),
    );
    if (!realNegative) {
      return {
        ok: false,
        tooling: check.tooling,
        diagnostics: [
          {
            where: "BrokenOperation.vue",
            file: "expected-type-error-did-not-occur",
          },
          ...negativeErrors,
        ],
      };
    }
    return { ok: true, tooling: check.tooling + " + @vue/compiler-sfc", diagnostics: [] };
  } finally {
    for (const compiled of [positive, negative]) {
      if (compiled && compiled.virtual) {
        rmSync(compiled.virtual, { force: true });
      }
    }
  }
}

export { consumerRoot, fixtureRoot };
