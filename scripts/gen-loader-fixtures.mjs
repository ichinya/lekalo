#!/usr/bin/env node
// One-shot generator for the hermetic loader fixture suite
// (tests/fixtures/loader/**). Run from the workspace root:
//   node scripts/gen-loader-fixtures.mjs
// Fixtures are committed; this script is a development convenience.

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const base = join(root, "tests", "fixtures", "loader");

const golden = join(base, "golden-twin.json");
let preserved = null;
try {
  preserved = (await import("node:fs")).readFileSync(golden);
} catch {
  // generated separately from the implementation
}
rmSync(base, { recursive: true, force: true });
mkdirSync(base, { recursive: true });
if (preserved !== null) {
  writeFileSync(golden, preserved);
}

let count = 0;

function fixture(name, files, expect, extra = {}) {
  const dir = join(base, name);
  for (const [relative, content] of Object.entries(files)) {
    const target = join(dir, relative);
    mkdirSync(join(target, ".."), { recursive: true });
    writeFileSync(target, content);
  }
  writeFileSync(join(dir, "expect.json"), `${JSON.stringify(expect, null, 2)}\n`);
  for (const [relative, content] of Object.entries(extra)) {
    const target = join(dir, relative);
    mkdirSync(join(target, ".."), { recursive: true });
    writeFileSync(target, content);
  }
  count += 1;
}

const projectJson = (version, id = "alpha", extra = "") =>
  `{"schema_version":"${version}","definitions":[{"id":"${id}","kind":"project","version":1${extra}}]}`;

const moduleYaml = (version, id, imports = []) => {
  const lines = [`schema_version: "${version}"`, "definitions:", `  - id: ${id}`, "    kind: module", "    version: 1"];
  if (imports.length > 0) {
    lines.push("    imports:");
    for (const imported of imports) lines.push(`      - ${imported}`);
  }
  return `${lines.join("\n")}\n`;
};

const entitiesYaml = (version, body) => `schema_version: "${version}"\ndefinitions:\n${body}`;

// ---------------------------------------------------------------------------
// Valid projects
// ---------------------------------------------------------------------------

fixture(
  "valid-yaml-1-0-0",
  {
    "lekalo/project.yaml": `schema_version: "1.0.0"\ndefinitions:\n  - id: planner\n    kind: project\n    version: 1\n    description: Planner sample\n`,
    "lekalo/modules/planner/module.yaml": moduleYaml("1.0.0", "planner", []),
    "lekalo/modules/planner/entities.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: planner.task_id",
        "    kind: scalar",
        "    version: 1",
        "    base: uuid",
        "  - id: planner.task",
        "    kind: entity",
        "    version: 1",
        "    fields:",
        "      - name: id",
        "        type: task_id",
        "        required: true",
        "      - name: window",
        "        type: list<task_id?>",
        "        required: true",
        "    identity:",
        "      - id",
      ].join("\n"),
    ),
    "lekalo/modules/planner/commands.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: planner.focus_task",
        "    kind: command",
        "    version: 1",
        "    input:",
        "      - name: id",
        "        type:",
        "          ref: task_id",
        "        required: true",
        "    effects:",
        "      - planner.apply_focus",
      ].join("\n"),
    ),
    "lekalo/modules/planner/events.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: planner.event.focus_changed",
        "    kind: event",
        "    version: 1",
        "    payload:",
        "      - name: id",
        "        type: planner.task_id",
        "        required: true",
      ].join("\n"),
    ),
  },
  { status: "valid" },
);

fixture(
  "valid-json-0-1-0",
  {
    "lekalo/project.yaml": `${projectJson("0.1.0", "work")}\n`,
    "lekalo/modules/work-items/module.yaml": moduleYaml("0.1.0", "work-items", []),
    "lekalo/modules/work-items/entities.yaml":
      '{"schema_version":"0.1.0","definitions":[{"id":"work-items.item_id","kind":"scalar","version":1,"base":"uuid"}]}\n',
    "lekalo/modules/tracking/module.yaml": moduleYaml("0.1.0", "tracking", ["work-items"]),
    "lekalo/modules/tracking/queries.yaml":
      '{"schema_version":"0.1.0","definitions":[{"id":"tracking.recent_items","kind":"query","version":1,"reads":["work-items.item_id"]}]}\n',
  },
  { status: "valid" },
);

fixture(
  "valid-zero-modules",
  {
    "lekalo/project.yaml": `schema_version: "1.0.0"\ndefinitions:\n  - id: solo\n    kind: project\n    version: 1\n`,
  },
  { status: "valid" },
);

fixture(
  "valid-crlf-multibyte",
  {
    "lekalo/project.yaml": "schema_version: \"1.0.0\"\r\ndefinitions:\r\n  - id: plan\r\n    kind: project\r\n    version: 1\r\n    description: Café planning — résumé\r\n",
    "lekalo/modules/plan/module.yaml": moduleYaml("1.0.0", "plan", []),
    "lekalo/modules/plan/entities.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: plan.élément",
        "    kind: scalar",
        "    version: 1",
        "    base: string",
      ].join("\n"),
    ),
  },
  { status: "valid" },
);

fixture(
  "valid-direct-visibility",
  {
    "lekalo/project.yaml": `${projectJson("1.0.0", "net")}\n`,
    "lekalo/modules/alpha/module.yaml": moduleYaml("1.0.0", "alpha", []),
    "lekalo/modules/alpha/entities.yaml": entitiesYaml(
      "1.0.0",
      ["  - id: alpha.widget", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
    "lekalo/modules/beta/module.yaml": moduleYaml("1.0.0", "beta", ["alpha"]),
    "lekalo/modules/beta/entities.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: beta.gadget",
        "    kind: entity",
        "    version: 1",
        "    fields:",
        "      - name: base",
        "        type: alpha.widget",
        "        required: true",
        "    identity:",
        "      - base",
      ].join("\n"),
    ),
    "lekalo/modules/gamma/module.yaml": moduleYaml("1.0.0", "gamma", ["alpha", "beta"]),
    "lekalo/modules/gamma/entities.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: gamma.tool",
        "    kind: value-object",
        "    version: 1",
        "    fields:",
        "      - name: part",
        "        type: beta.gadget?",
        "        required: true",
      ].join("\n"),
    ),
  },
  { status: "valid" },
);

fixture(
  "valid-rename-history",
  {
    "lekalo/project.yaml": `schema_version: "1.0.0"\ndefinitions:\n  - id: hist\n    kind: project\n    version: 2\n    id_registry:\n      rename_history:\n        - from: hist.legacy_name\n          to: hist.modern_name\n          definition_version: 2\n`,
    "lekalo/modules/hist/module.yaml": moduleYaml("1.0.0", "hist", []),
    "lekalo/modules/hist/entities.yaml": entitiesYaml(
      "1.0.0",
      [
        "  - id: hist.modern_name",
        "    kind: scalar",
        "    version: 2",
        "    base: string",
        "    renamed_from:",
        "      - hist.legacy_name",
      ].join("\n"),
    ),
  },
  { status: "valid" },
);

// Twin projects: identical semantics, JSON vs YAML source.
const twinSemantic = {
  project: { id: "twin", kind: "project", version: 1 },
  module: { id: "twin", kind: "module", version: 1 },
  scalar: { id: "twin.label", kind: "scalar", version: 1, base: "string" },
  entity: {
    id: "twin.box",
    kind: "entity",
    version: 1,
    fields: [
      { name: "label", type: { ref: "twin.label" }, required: true },
      { name: "tags", type: { list: { ref: "twin.label" } }, required: true },
      { name: "note", type: { optional: { ref: "twin.label" } }, required: true },
    ],
    identity: ["label"],
  },
};
fixture(
  "valid-twin-json",
  {
    "lekalo/project.yaml": `${JSON.stringify({ schema_version: "1.0.0", definitions: [twinSemantic.project] })}\n`,
    "lekalo/modules/twin/module.yaml": `${JSON.stringify({ schema_version: "1.0.0", definitions: [twinSemantic.module] })}\n`,
    "lekalo/modules/twin/entities.yaml": `${JSON.stringify({ schema_version: "1.0.0", definitions: [twinSemantic.scalar, twinSemantic.entity] })}\n`,
  },
  { status: "valid", modelGolden: "../golden-twin.json" },
);
fixture(
  "valid-twin-yaml",
  {
    "lekalo/project.yaml": `schema_version: "1.0.0"\ndefinitions:\n  - id: twin\n    kind: project\n    version: 1\n`,
    "lekalo/modules/twin/module.yaml": `schema_version: "1.0.0"\ndefinitions:\n  - id: twin\n    kind: module\n    version: 1\n`,
    "lekalo/modules/twin/entities.yaml": [
      'schema_version: "1.0.0"',
      "definitions:",
      "  - id: twin.label",
      "    kind: scalar",
      "    version: 1",
      "    base: string",
      "  - id: twin.box",
      "    kind: entity",
      "    version: 1",
      "    fields:",
      "      - name: label",
      "        type: label",
      "        required: true",
      "      - name: tags",
      "        type: list<label>",
      "        required: true",
      "      - name: note",
      "        type: label?",
      "        required: true",
      "    identity:",
      "      - label",
    ].join("\n") + "\n",
  },
  { status: "valid", modelGolden: "../golden-twin.json" },
);

// ---------------------------------------------------------------------------
// Invalid projects: one focused defect per stable code
// ---------------------------------------------------------------------------

const V1 = "1.0.0";

const simple = (name, expect, mutated, options = {}) => {
  const files = {
    "lekalo/project.yaml": options.project ?? `${projectJson(options.version ?? V1, "core")}\n`,
    "lekalo/modules/core/module.yaml": options.module ?? moduleYaml(options.version ?? V1, "core", options.imports ?? []),
    "lekalo/modules/core/entities.yaml": options.entities ?? entitiesYaml(
      options.version ?? V1,
      ["  - id: core.thing", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
    ...mutated,
  };
  fixture(name, files, expect, options.extra ?? {});
};

simple("invalid-encoding-bom", { status: "invalid", exit: 1, stream: "stderr", code: "loader.encoding" }, {}, {
  project: "\uFEFF" + `${projectJson(V1, "core")}\n`,
});

simple("invalid-json-comment", { status: "invalid", exit: 1, stream: "stderr", code: "loader.json-parse" }, {}, {
  project: `{"schema_version":"${V1}" /* note */,"definitions":[{"id":"core","kind":"project","version":1}]}\n`,
});

simple("invalid-json-trailing-comma", { status: "invalid", exit: 1, stream: "stderr", code: "loader.json-parse" }, {}, {
  project: `{"schema_version":"${V1}","definitions":[{"id":"core","kind":"project","version":1},]}\n`,
});

simple("invalid-json-duplicate-key", {
  status: "invalid", exit: 1, stream: "stderr", code: "loader.duplicate-key",
}, {}, {
  project: `{"schema_version":"${V1}","schema_version":"${V1}","definitions":[{"id":"core","kind":"project","version":1}]}\n`,
});

simple("invalid-json-number-overflow", { status: "invalid", exit: 1, stream: "stderr", code: "loader.json-parse" }, {}, {
  project: `{"schema_version":"${V1}","definitions":[{"id":"core","kind":"project","version":1,"overflow":123456789012345678901234567890123456789012345678901234}]}\n`,
});

simple("invalid-yaml-anchor", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `schema_version: "${V1}"\ndefinitions:\n  - id: &anchor core\n    kind: project\n    version: 1\n`,
});

simple("invalid-yaml-alias", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `base: &value x\nschema_version: *value\n`,
});

simple("invalid-yaml-tag", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `schema_version: !!str "${V1}"\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-yaml-flow", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `schema_version: "${V1}"\ndefinitions: [{id: core, kind: project, version: 1}]\n`,
});

simple("invalid-yaml-multi-doc", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `schema_version: "${V1}"\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n---\nschema_version: "${V1}"\ndefinitions:\n  - id: other\n`,
});

simple("invalid-yaml-doc-marker", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `---\nschema_version: "${V1}"\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-yaml-duplicate-key", { status: "invalid", exit: 1, stream: "stderr", code: "loader.duplicate-key" }, {}, {
  project: `schema_version: "${V1}"\nschema_version: "${V1}"\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-yaml-nonstring-key", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `schema_version: "${V1}"\n2024: year\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-yaml-merge-key", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-unsupported" }, {}, {
  project: `defaults:\n  <<: {version: 1}\nschema_version: "${V1}"\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-yaml-parse", { status: "invalid", exit: 1, stream: "stderr", code: "loader.yaml-parse" }, {}, {
  project: `schema_version: "${V1}"\ndefinitions:\n  - id: core\n   kind: project\n    version: 1\n`,
});

simple("invalid-version-missing", { status: "invalid", exit: 1, stream: "stderr", code: "loader.document-shape" }, {}, {
  project: `definitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-version-nonstring", { status: "invalid", exit: 1, stream: "stderr", code: "loader.document-shape" }, {}, {
  project: `schema_version: 2024\ndefinitions:\n  - id: core\n    kind: project\n    version: 1\n`,
});

simple("invalid-version-unsupported", {
  status: "unsupported-version", exit: 5, stream: "stderr", code: "versioning.unsupported-version",
}, {}, {
  version: "2.0.0",
  project: `${projectJson("2.0.0", "core")}\n`,
});

simple("invalid-version-mixed", { status: "invalid", exit: 1, stream: "stderr", code: "versioning.mixed-versions" }, {}, {
  entities: entitiesYaml("0.1.0", ["  - id: core.thing", "    kind: scalar", "    version: 1", "    base: string"].join("\n")),
});

simple("invalid-definitions-empty", { status: "invalid", exit: 1, stream: "stderr", code: "loader.document-shape" }, {}, {
  project: `{"schema_version":"${V1}","definitions":[]}\n`,
});

simple("invalid-module-two-definitions", { status: "invalid", exit: 1, stream: "stderr", code: "loader.document-shape" }, {}, {
  module: entitiesYaml(V1, ["  - id: core", "    kind: module", "    version: 1", "  - id: core-twin", "    kind: module", "    version: 1"].join("\n")),
});

fixture(
  "invalid-duplicate-module-id",
  {
    "lekalo/project.yaml": `${projectJson(V1, "dup")}\n`,
    "lekalo/modules/one/module.yaml": moduleYaml(V1, "same-id", []),
    "lekalo/modules/two/module.yaml": moduleYaml(V1, "same-id", []),
  },
  { status: "invalid", exit: 1, stream: "stderr", code: "loader.duplicate-module-id" },
);

simple("invalid-duplicate-definition", { status: "invalid", exit: 1, stream: "stderr", code: "loader.duplicate-definition" }, {}, {
  entities: entitiesYaml(
    V1,
    [
      "  - id: core.thing",
      "    kind: scalar",
      "    version: 1",
      "    base: string",
      "  - id: core.thing",
      "    kind: scalar",
      "    version: 1",
      "    base: text",
    ].join("\n"),
  ),
});

fixture(
  "invalid-conflicting-declaration",
  {
    "lekalo/project.yaml": `${projectJson(V1, "clash")}\n`,
    "lekalo/modules/one/module.yaml": moduleYaml(V1, "one", []),
    "lekalo/modules/one/entities.yaml": entitiesYaml(
      V1,
      ["  - id: one.shared", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
    "lekalo/modules/two/module.yaml": moduleYaml(V1, "two", []),
    "lekalo/modules/two/entities.yaml": entitiesYaml(
      V1,
      ["  - id: one.shared", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
  },
  { status: "invalid", exit: 1, stream: "stderr", code: "loader.conflicting-declaration" },
);

simple("invalid-import-missing", { status: "invalid", exit: 1, stream: "stderr", code: "loader.import-missing" }, {}, {
  imports: ["ghost"],
});

simple("invalid-import-cycle", { status: "invalid", exit: 1, stream: "stderr", code: "loader.import-cycle" }, {}, {
  imports: ["other"],
  extra: {
    "lekalo/modules/other/module.yaml": moduleYaml(V1, "other", ["core"]),
    "lekalo/modules/other/entities.yaml": entitiesYaml(
      V1,
      ["  - id: other.prop", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
  },
});

simple("invalid-import-self", { status: "invalid", exit: 1, stream: "stderr", code: "loader.import-cycle" }, {}, {
  imports: ["core"],
});

simple("invalid-import-duplicate", { status: "invalid", exit: 1, stream: "stderr", code: "loader.import-invalid" }, {}, {
  imports: ["other", "other"],
  extra: {
    "lekalo/modules/other/module.yaml": moduleYaml(V1, "other", []),
  },
});

simple("invalid-import-grammar", { status: "invalid", exit: 1, stream: "stderr", code: "loader.import-invalid" }, {}, {
  imports: ["Other"],
});

simple("invalid-import-path-escape", { status: "denied", exit: 3, stream: "stdout", code: "loader.path-escape" }, {}, {
  imports: ["../core"],
});

fixture(
  "invalid-reference-without-import",
  {
    "lekalo/project.yaml": `${projectJson(V1, "net")}\n`,
    "lekalo/modules/alpha/module.yaml": moduleYaml(V1, "alpha", []),
    "lekalo/modules/alpha/entities.yaml": entitiesYaml(
      V1,
      ["  - id: alpha.widget", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
    "lekalo/modules/beta/module.yaml": moduleYaml(V1, "beta", []),
    "lekalo/modules/beta/entities.yaml": entitiesYaml(
      V1,
      [
        "  - id: beta.gadget",
        "    kind: entity",
        "    version: 1",
        "    fields:",
        "      - name: base",
        "        type: alpha.widget",
        "        required: true",
        "    identity:",
        "      - base",
      ].join("\n"),
    ),
  },
  { status: "invalid", exit: 1, stream: "stderr", code: "loader.reference-without-import" },
);

simple("invalid-short-reference-unresolved", { status: "invalid", exit: 1, stream: "stderr", code: "loader.short-reference-unresolved" }, {}, {
  entities: entitiesYaml(
    V1,
    [
      "  - id: core.thing",
      "    kind: entity",
      "    version: 1",
      "    fields:",
      "      - name: ghost_ref",
      "        type: no_such_thing",
      "        required: true",
      "    identity:",
      "      - ghost_ref",
    ].join("\n"),
  ),
});

fixture(
  "invalid-ambiguous-short-reference",
  {
    "lekalo/project.yaml": `${projectJson(V1, "amb")}\n`,
    "lekalo/modules/amb/module.yaml": moduleYaml(V1, "amb", []),
    "lekalo/modules/amb/entities.yaml": entitiesYaml(
      V1,
      [
        "  - id: amb.label",
        "    kind: scalar",
        "    version: 1",
        "    base: string",
        "  - id: amb.event.label",
        "    kind: scalar",
        "    version: 1",
        "    base: string",
        "  - id: amb.holder",
        "    kind: entity",
        "    version: 1",
        "    fields:",
        "      - name: pick",
        "        type: label",
        "        required: true",
        "    identity:",
        "      - pick",
      ].join("\n"),
    ),
  },
  { status: "invalid", exit: 1, stream: "stderr", code: "loader.ambiguous-short-reference" },
);

simple("invalid-type-syntax", { status: "invalid", exit: 1, stream: "stderr", code: "loader.type-syntax" }, {}, {
  entities: entitiesYaml(
    V1,
    [
      "  - id: core.thing",
      "    kind: entity",
      "    version: 1",
      "    fields:",
      "      - name: broken",
      "        type: list< core.thing",
      "        required: true",
      "    identity:",
      "      - broken",
    ].join("\n"),
  ),
});

simple("invalid-type-depth", { status: "invalid", exit: 1, stream: "stderr", code: "loader.type-depth" }, {}, {
  entities: entitiesYaml(
    V1,
    [
      "  - id: core.deep",
      "    kind: scalar",
      "    version: 1",
      "    base: string",
      "  - id: core.thing",
      "    kind: entity",
      "    version: 1",
      "    fields:",
      "      - name: deep",
      "        type: list<list<list<core.deep?>>>",
      "        required: true",
      "    identity:",
      "      - deep",
    ].join("\n"),
  ),
});

fixture(
  "invalid-structure-unexpected-entry",
  {
    "lekalo/project.yaml": `${projectJson(V1, "core")}\n`,
    "lekalo/rogue.yaml": "not: allowed\n",
  },
  { status: "denied", exit: 3, stream: "stdout", code: "structure.canonical-unexpected-entry" },
);

fixture(
  "invalid-structure-module-yaml-missing",
  {
    "lekalo/project.yaml": `${projectJson(V1, "core")}\n`,
    "lekalo/modules/core/entities.yaml": entitiesYaml(
      V1,
      ["  - id: core.thing", "    kind: scalar", "    version: 1", "    base: string"].join("\n"),
    ),
  },
  { status: "invalid", exit: 1, stream: "stderr", code: "structure.document-missing" },
);

console.log(`generated ${count} fixtures under ${base}`);
