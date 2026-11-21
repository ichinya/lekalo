const fs = require("node:fs");
const crypto = require("node:crypto");

const [data, requestedVersion] = process.argv.slice(2);
const wire = JSON.parse(data);
const request = JSON.parse(fs.readFileSync(0, "utf8"));
const digest = (bytes) => "sha256:" + crypto.createHash("sha256").update(bytes).digest("hex");
const response = {
  protocol: request.protocol,
  protocol_version: request.protocol_version,
  request_id: request.request_id,
  operation: request.operation,
  status: "ok",
  evidence: { adapter: wire.producer },
};
if (request.operation === "describe") {
  response.capabilities = {
    adapter: wire.producer,
    protocol_versions: ["0.3.2", requestedVersion],
    ir_versions: ["0.8.0", "0.2.16"],
    operations: ["describe", "scan"],
    transports: ["stdin"],
    targets: ["evidence-demo"],
    read_scopes: ["src/**"],
    capabilities: { "scan.symbols": "full" },
  };
} else {
  response.evidence.scan_profile_digest = digest("scan-profile");
  response.evidence.native_uncertainty = [];
  response.result = { entries: [
    ["src/tasks.ts", "task", "entity", "interface", "root"],
    ["src/ids.ts", "task_id", "scalar", "variable", "dependency"],
  ].map(([path, symbol, kind, family, native]) => {
    const source_digest = digest(fs.readFileSync(path));
    return {
      path, kind, detail: JSON.stringify({
        s: "taskboard." + symbol, n: native, l: 1, q: native === "root" ? "high" : "exact",
      }),
      evidence: {
        source_digest, signature: source_digest,
        references: native === "root" ? [
          { target: "dependency", role: "reference", confidence: "high" },
        ] : [],
        native_symbols: [{
          native, qualified_name: native, family, path, line: 1, end_line: 1,
          slot: "static", declaration_only: true, signature: source_digest,
        }],
      },
    };
  }) };
  if (requestedVersion === "1.4.0") response.result.evidence_groups = wire.groups;
}
process.stdout.write(JSON.stringify(response));
