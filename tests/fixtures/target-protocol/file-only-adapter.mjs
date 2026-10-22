#!/usr/bin/env node
/**
 * The file-transport fixture adapter (issue #106 review r2 #8): declares
 * `transports: ["file"]`, so every SESSION exchange after the handshake is
 * delivered through the `--lekalo-request-file <PATH>` convention. The
 * host's describe handshake is stdin-delivered by design (empty scopes,
 * no session budget yet), so this adapter reads whichever channel the
 * host used: the file coordinate first, then piped stdin.
 */
import { readFileSync } from "node:fs";

const marker = process.argv.indexOf("--lekalo-request-file");
let raw = null;
if (marker !== -1 && process.argv[marker + 1]) {
  raw = readFileSync(process.argv[marker + 1], "utf8");
} else {
  raw = readFileSync(0, "utf8");
}
let request;
try {
  request = JSON.parse(raw);
} catch {
  process.stderr.write("file-exchange fixture: undecodable request document\n");
  process.exit(1);
}

const adapter = {
  digest: "sha256:5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f",
  id: "file-exchange-fixture",
  version: "0.1.0",
};
const shell = {
  evidence: { adapter },
  operation: request.operation,
  protocol: "lekalo.target/v1",
  protocol_version: request.protocol_version,
  request_id: request.request_id,
};
if (request.operation === "describe") {
  process.stdout.write(
    JSON.stringify({
      ...shell,
      status: "ok",
      capabilities: {
        adapter,
        capabilities: { "scan.symbols": "full" },
        ir_versions: [],
        operations: ["describe", "scan"],
        progress: false,
        protocol_versions: ["0.3.2"],
        read_scopes: ["src/**"],
        targets: ["file-exchange-fixture"],
        transports: ["file"],
        write_scopes: [],
      },
    }),
  );
} else {
  process.stdout.write(
    JSON.stringify({
      ...shell,
      status: "ok",
      result: { entries: [{ kind: "source", path: "src/main.ts" }], truncated: false },
    }),
  );
}
