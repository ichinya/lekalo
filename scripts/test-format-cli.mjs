#!/usr/bin/env node
// Issue #94 live CLI gate: the real binary drives every documented
// disposition of `lekalo format` — the 0/3/1/5 check classes, apply
// idempotence against the reviewed canonical golden, the empty semantic
// diff, the no-partial-write refusal, scope selectors, comment-loss
// block/lossy behavior, the editor --stdin flow, and the report
// invariants JSON Schema cannot express.
//
// Requires the built binary (target/debug/lekalo) exactly like the other
// binary-dependent gates; a missing binary fails the gate.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const fixtures = join(root, "tests/fixtures/format");
if (!existsSync(binary)) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "binary-missing", binary }, null, 2)}
`);
  process.exit(1);
}

// The report schema check runs against FRESH binary envelopes (Ajv is the
// same pinned provisioning every other contract gate uses).
const ajvRequire = createRequire(
  (process.env.LEKALO_AJV_NODE_PATH || join(root, "scripts/lib")) + "/"
);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = ajvRequire("ajv/dist/2020").default;
  ajvVersion = ajvRequire("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: error?.message }, null, 2)}
`);
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}
`);
  process.exit(1);
}
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateReport = ajv.compile(
  JSON.parse(readFileSync(join(root, "contracts/format-report.schema.v0.8.0.json"), "utf8"))
);
const assertReport = (report, label) => {
  assert.ok(validateReport(report), `${label}: ${JSON.stringify(validateReport.errors)}`);
};

const sha256 = (data) => createHash("sha256").update(data).digest("hex");
const workspace = mkdtempSync(join(tmpdir(), "lekalo-format-cli-"));
let passed = 0;
try {
  const seed = (name, family) => {
    const destination = join(workspace, `${name}-${process.pid}`);
    rmSync(destination, { recursive: true, force: true });
    cpSync(join(fixtures, family), destination, { recursive: true });
    return destination;
  };
  const run = (cwd, args, input) => {
    const child = spawnSync(binary, args, {
      cwd,
      encoding: "utf8",
      input,
      maxBuffer: 64 * 1024 * 1024,
      timeout: 120000,
    });
    assert.ok(!child.error, `binary ran: ${child.error}`);
    return {
      status: child.status,
      stdout: child.stdout ?? "",
      stderr: child.stderr ?? "",
      json() {
        return JSON.parse((child.stdout || child.stderr).trim());
      },
    };
  };
  const treeHash = (base) => {
    const digest = createHash("sha256");
    const walk = (relative) => {
      const entries = readdirSync(join(base, relative), { withFileTypes: true })
        .map((entry) => entry.name)
        .sort();
      for (const name of entries) {
        const next = relative ? `${relative}/${name}` : name;
        const stats = statSync(join(base, next));
        if (stats.isDirectory()) {
          assert.ok(!name.startsWith(".lekalo"), "no runtime tree exists");
          walk(next);
        } else {
          digest.update(`${next}:${sha256(readFileSync(join(base, next)))}\n`);
        }
      }
    };
    walk("");
    return digest.digest("hex");
  };

  // 1. Check dispositions: 3 (diff), 0 (canonical), 1 (parse), 5 (version).
  {
    const project = seed("gate-check", "seeds/noncanonical");
    const denied = run(project, ["--json", "format", "--check"]);
    assert.equal(denied.status, 3, "check on noncanonical source denies");
    assert.equal(denied.stderr, "", "denied writes stdout");
    const envelope = denied.json();
    assert.equal(envelope.status, "denied");
    assert.ok(envelope.reasonCodes.includes("format.diff"), "format.diff is the gate disposition");
    // Report invariants against a fresh envelope.
    const report = envelope.payload;
    assert.equal(report.mode, "check");
    assertReport(report, "denied envelope validates against the report schema");
    assert.equal(report.summary.applied, 0, "check applies nothing");
    assert.equal(report.summary.selected, report.summary.changed + report.files.filter((row) => row.action === "unchanged").length);
    const paths = report.files.map((row) => row.path);
    assert.deepEqual(paths, [...paths].sort(), "rows are byte-sorted by path");
    assert.equal(new Set(paths).size, paths.length, "no duplicate rows");
    assert.equal(report.policy.digest, JSON.parse(readFileSync(join(root, "contracts/format-policy.v0.8.0.json"), "utf8")).policyDigest);
    for (const row of report.files) {
      assert.ok(!("replacement" in row), "disk rows never carry replacement text");
      assert.equal(row.equal, true, "every planned row is semantically verified");
    }
    // Read-only: source and (absent) runtime inventory unchanged.
    const beforeTree = treeHash(project);
    run(project, ["format", "--check"]);
    assert.equal(treeHash(project), beforeTree, "check never mutates the project");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }
  {
    const project = seed("gate-canonical", "planner-canonical");
    const valid = run(project, ["--json", "format", "--check"]);
    assert.equal(valid.status, 0);
    assert.equal(valid.json().format.summary.changed, 0);
    assertReport(valid.json().format, "valid envelope validates against the report schema");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }
  {
    const project = seed("gate-parse", "seeds/invalid-parse");
    const invalid = run(project, ["format", "--check"]);
    assert.equal(invalid.status, 1, "a parse error stays invalid");
    assert.match(invalid.stderr, /loader\.yaml-unsupported/, "never relabeled as format.diff");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }
  {
    const project = seed("gate-version", "seeds/unsupported-version");
    const unsupported = run(project, ["format", "--check"]);
    assert.equal(unsupported.status, 5, "unknown versions refuse");
    assert.match(unsupported.stderr, /versioning\.unsupported-version/);
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 2. Apply idempotence + the empty semantic diff + golden bytes.
  {
    const beforeProject = seed("diff-before", "seeds/noncanonical");
    const afterProject = seed("diff-after", "seeds/noncanonical");
    const applied = run(afterProject, ["format"]);
    assert.equal(applied.status, 0, applied.stdout + applied.stderr);
    // Idempotence: F(F(x)) == F(x) and a no-op apply touches nothing.
    const entities = join(afterProject, "lekalo/modules/planner/entities.yaml");
    const firstBytes = readFileSync(entities);
    const firstMtime = statSync(entities).mtimeMs;
    const again = run(afterProject, ["format"]);
    assert.equal(again.status, 0);
    assert.ok(readFileSync(entities).equals(firstBytes), "F(F(x)) == F(x)");
    assert.equal(statSync(entities).mtimeMs, firstMtime, "no-op apply does not touch files");
    assert.equal(run(afterProject, ["format", "--check"]).status, 0);
    // The canonical golden bytes are reproduced exactly.
    const golden = join(fixtures, "planner-canonical");
    for (const relative of [
      "lekalo/project.yaml",
      "lekalo/modules/planner/module.yaml",
      "lekalo/modules/planner/entities.yaml",
      "lekalo/modules/planner/scenarios.yaml",
      "lekalo/modules/scheduling/module.yaml",
    ]) {
      assert.ok(
        readFileSync(join(afterProject, relative)).equals(readFileSync(join(golden, relative))),
        `canonical bytes drift for ${relative}`,
      );
    }
    // The semantic diff of the formatting-only change is empty.
    const parent = dirname(afterProject);
    const diff = run(parent, [
      "diff",
      basename(beforeProject),
      basename(afterProject),
      "--format",
      "json",
      "--json",
    ]);
    assert.equal(diff.status, 0, diff.stdout + diff.stderr);
    assert.equal(diff.json().diff.changes.length, 0, "formatting produces an empty semantic diff");
    rmSync(beforeProject, { recursive: true, force: true });
    rmSync(afterProject, { recursive: true, force: true });
    passed += 1;
  }

  // 3. A late malformed file refuses the whole plan: no partial rewrite.
  {
    const project = seed("partial", "seeds/noncanonical");
    const entities = join(project, "lekalo/modules/planner/entities.yaml");
    const module = join(project, "lekalo/modules/planner/module.yaml");
    const entitiesBefore = readFileSync(entities);
    const moduleBefore = readFileSync(module);
    writeFileSync(join(project, "lekalo/modules/planner/scenarios.yaml"), Buffer.from("definitions: [\n  broken\n"));
    const refused = run(project, ["format"]);
    assert.equal(refused.status, 1, "a malformed source blocks the plan");
    assert.ok(readFileSync(entities).equals(entitiesBefore), "no earlier file rewritten");
    assert.ok(readFileSync(module).equals(moduleBefore), "no partial application");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 4. Scope selectors cover exactly their documents; unknowns refuse.
  {
    const project = seed("scope", "seeds/noncanonical");
    const scoped = run(project, ["format", "--check", "lekalo/modules/scheduling", "--json"]);
    assert.equal(scoped.status, 3);
    const rows = scoped.json().payload.files.map((row) => row.path);
    assert.deepEqual(rows, ["lekalo/modules/scheduling/module.yaml"]);
    const document = run(project, ["format", "--check", "lekalo/modules/scheduling/module.yaml"]);
    assert.equal(document.status, 3);
    const unknown = run(project, ["format", "--check", "lekalo/absent.yaml"]);
    assert.equal(unknown.status, 1);
    assert.match(unknown.stderr, /format\.selection-invalid/);
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 5. Comment loss blocks by default and applies under explicit opt-in.
  {
    const project = seed("loss", "seeds/comment-loss");
    const blocked = run(project, ["--json", "format"]);
    assert.equal(blocked.status, 3);
    assert.equal(blocked.json().reasonCodes.includes("format.comments-loss"), true);
    const lossy = run(project, ["--json", "format", "--allow-lossy-comments"]);
    assert.equal(lossy.status, 0);
    const losses = lossy.json().format.files.flatMap((row) => row.loss ?? []);
    assert.equal(losses.length, 1, "the loss row is enumerated in the report");
    assert.equal(run(project, ["format", "--check"]).status, 0, "lossy output stabilizes");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 6. The editor --stdin flow: replacement text, no writes, conflicts refuse.
  {
    const project = seed("stdin", "planner-canonical");
    const modulePath = join(project, "lekalo/modules/scheduling/module.yaml");
    const before = readFileSync(modulePath);
    for (const args of [
      ["format", "--stdin", "--stdin-filepath", "x", "--check"],
      ["format", "--stdin", "--stdin-filepath", "x", "lekalo/project.yaml"],
      ["format", "--stdin"],
    ]) {
      const conflict = run(project, args, "");
      assert.notEqual(conflict.status, 0, `usage conflict refuses: ${args}`);
    }
    const buffer = 'schema_version: "0.2.16"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling module\n';
    const editor = run(
      project,
      ["--json", "format", "--stdin", "--stdin-filepath", "lekalo/modules/scheduling/module.yaml"],
      buffer,
    );
    assert.equal(editor.status, 0, editor.stdout + editor.stderr);
    const editorReport = editor.json().format;
    assertReport(editorReport, "stdin envelope validates against the report schema");
    const row = editorReport.files[0];
    assert.ok(typeof row.replacement === "string", "the editor row carries replacement text");
    assert.match(row.replacement, /^"definitions":/, "canonical document text");
    assert.ok(before.equals(readFileSync(modulePath)), "stdin mode never writes the project");
    // The stdin-loss refusal carries NO report payload (finding r4 7):
    // the schema requires the row's replacement text, which a loss
    // refusal must withhold, so the only schema-valid stdin report is
    // the success one. The refusal is answered by format.stdin-loss on
    // the diagnostics channel alone.
    const lossyBuffer =
      "schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling\n    module # this trailing comment has no owner line\n";
    const refused = run(
      project,
      ["--json", "format", "--stdin", "--stdin-filepath", "lekalo/modules/scheduling/module.yaml"],
      lossyBuffer,
    );
    assert.equal(refused.status, 3, refused.stdout + refused.stderr);
    const refusedEnvelope = refused.json();
    assert.equal(refusedEnvelope.status, "denied", "the refusal envelope is denied");
    assert.ok(
      refusedEnvelope.format === undefined,
      "the refusal carries no report payload",
    );
    assert.ok(
      (refusedEnvelope.diagnostics ?? []).some((entry) => entry.id === "format.stdin-loss"),
      "the registered editor-mode loss identity rides the diagnostics",
    );
    assert.ok(before.equals(readFileSync(modulePath)), "the refusal never writes");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 6b. Producer/schema parity for the admitted module-directory grammar
  //     (finding r4 8): the loader admits portable segments that start
  //     with a digit and contain dots; the report pattern must accept
  //     exactly what the formatter emits for such a project.
  {
    const project = seed("digit-module", "planner-canonical");
    const renamed = join(project, "lekalo/modules/1scheduling");
    cpSync(join(project, "lekalo/modules/scheduling"), renamed, { recursive: true });
    rmSync(join(project, "lekalo/modules/scheduling"), { recursive: true, force: true });
    const output = run(project, ["format", "--check", "--json"]);
    assert.equal(output.status, 0, output.stdout + output.stderr);
    const report = output.json().format;
    assert.ok(
      report.files.some((entry) => entry.path === "lekalo/modules/1scheduling/module.yaml"),
      `the admitted digit-leading path is emitted: ${JSON.stringify(report.files.map((entry) => entry.path))}`,
    );
    assertReport(report, "digit-leading module directory report validates");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 7. The committed planner fixture is stable on this platform.
  {
    const project = seed("planner-golden", "planner-canonical");
    assert.equal(run(project, ["format", "--check"]).status, 0);
    assert.equal(run(project, ["format"]).status, 0);
    const golden = join(fixtures, "planner-canonical");
    const manifest = JSON.parse(readFileSync(join(fixtures, "portability/manifest.json"), "utf8"));
    for (const relative of [
      "lekalo/project.yaml",
      "lekalo/modules/planner/module.yaml",
      "lekalo/modules/planner/entities.yaml",
      "lekalo/modules/planner/scenarios.yaml",
      "lekalo/modules/scheduling/module.yaml",
    ]) {
      assert.ok(
        readFileSync(join(project, relative)).equals(readFileSync(join(golden, relative))),
        `planner golden is stable: ${relative}`,
      );
    }
    for (const file of manifest.files) {
      assert.equal(sha256(readFileSync(join(root, file.path))), file.sha256, `manifest pin: ${file.path}`);
    }
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  rmSync(workspace, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "format-cli", passed }, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "format-cli", error: String(error?.message ?? error) }, null, 2)}\n`);
  process.exitCode = 1;
} finally {
  if (existsSync(workspace)) rmSync(workspace, { recursive: true, force: true });
}
