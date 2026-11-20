const fs = require("node:fs");
const crypto = require("node:crypto");

const request = JSON.parse(fs.readFileSync(0, "utf8"));
const mode = process.argv[2];
const digest = bytes => "sha256:" + crypto.createHash("sha256").update(bytes).digest("hex");
const adapter = {id: "conformance-evidence", version: "1.0.0", digest: digest("conformance fixture")};
const response = {
  protocol: request.protocol,
  protocol_version: request.protocol_version,
  request_id: request.request_id,
  operation: request.operation,
  status: "ok",
  evidence: {adapter},
};

if (request.operation === "describe") {
  response.capabilities = {
    adapter,
    protocol_versions: ["0.3.2", "1.4.0"],
    ir_versions: ["0.2.16", "0.8.0"],
    operations: ["describe", "scan"],
    transports: ["stdin"],
    targets: mode === "targetless" ? [] : ["conformance-evidence"],
    read_scopes: ["src/**"],
    capabilities: {"scan.symbols": "full"},
  };
} else if (request.operation === "scan") {
  if (mode === "targetless" && request.target != null) {
    throw new Error("the fixture scan must omit its target");
  }
  const path = "src/conformance-evidence.txt";
  const sourceDigest = digest(fs.readFileSync(path));
  const native = "conformance-marker";
  const entry = {
    kind: "entity",
    path,
    detail: JSON.stringify({s: "planner.task", n: native, l: 1}),
    evidence: {
      source_digest: sourceDigest,
      signature: sourceDigest,
      native_symbols: [{
        native, qualified_name: native, family: "variable", path, line: 1, end_line: 1,
        slot: "static", declaration_only: true, signature: sourceDigest,
      }],
    },
  };
  const fact = {
    subject: mode === "bad-subject" ? "missing" : native,
    key: "primary",
    references: {},
    sources: [{path, source_digest: mode === "bad-digest" ? digest("changed source") : sourceDigest}],
    payload: {label: "fixture"},
  };
  response.evidence.scan_profile_digest = digest("fixture scan profile");
  response.evidence.native_uncertainty = [];
  response.result = {
    entries: [entry],
    evidence_groups: mode === "empty" ? [] : [{
      family: "fixture-label",
      package_digest: digest("fixture package"),
      schema_digest: digest("fixture schema"),
      coverage: "complete",
      facts: [fact],
    }],
  };
} else {
  throw new Error("unexpected conformance operation");
}
process.stdout.write(JSON.stringify(response));
