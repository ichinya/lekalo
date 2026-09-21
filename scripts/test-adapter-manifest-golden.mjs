// Issue #32 step 12: the shipped adapter's committed manifest must
// recompute exactly against the committed artifact bytes — the golden
// manifest check. Dependency-free; run from the repo root.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const bytes = readFileSync(new URL("adapters/node-typescript/adapter.mjs", root));
const manifest = JSON.parse(
  readFileSync(new URL("adapters/node-typescript/adapter.manifest.json", root), "utf8"),
);

// 1. Identity and compatibility pins.
if (manifest.schemaVersion !== "lekalo/adapter-manifest/v0.3.2") {
  fail("schema-version", manifest.schemaVersion);
}
if (manifest.identity !== "dev.lekalo.adapter-manifest@0.3.2") {
  fail("identity", manifest.identity);
}
if (manifest.adapter.id !== "lekalo-target-node-typescript") {
  fail("adapter-id", manifest.adapter.id);
}
if (!manifest.compatibility.protocolVersions.includes("0.3.2")) {
  fail("protocol-compatibility", manifest.compatibility.protocolVersions.join(","));
}
if (!manifest.compatibility.irVersions.includes("0.2.16")) {
  fail("ir-compatibility", manifest.compatibility.irVersions.join(","));
}

// 2. The entry digest must match the committed artifact bytes exactly.
const entry = manifest.integrity.files.find((file) => file.path === "adapter.mjs");
if (!entry) fail("entry-missing", "adapter.mjs is not in integrity.files");
const actualEntryDigest = createHash("sha256").update(bytes).digest("hex");
if (entry.digest !== "sha256:" + actualEntryDigest) {
  fail("entry-digest", `manifest=${entry.digest} actual=sha256:${actualEntryDigest}`);
}
if (entry.bytes !== bytes.length) {
  fail("entry-length", `manifest=${entry.bytes} actual=${bytes.length}`);
}

// 3. The package digest recomputes over the framed per-file byte set.
const canonical = (value) => {
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (value !== null && typeof value === "object") {
    return (
      "{" +
      Object.keys(value)
        .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
        .map((k) => JSON.stringify(k) + ":" + canonical(value[k]))
        .join(",") +
      "}"
    );
  }
  return JSON.stringify(value);
};
const stripped = JSON.parse(JSON.stringify(manifest));
stripped.integrity.packageDigest = "sha256:" + "0".repeat(64);
delete stripped.manifestDigest;
const manifestCanonical = Buffer.from(canonical(stripped), "utf8");
const manifestStored = readFileSync(
  new URL("adapters/node-typescript/adapter.manifest.json", root),
);
// The manifest's declared digest covers its stored bytes minus the
// self-referential member — recomputed through the canonical form.
const framed = (path, bytes) => {
  const head = Buffer.concat([
    Buffer.from(path, "utf8"),
    Buffer.from([0]),
    (() => {
      const b = Buffer.alloc(8);
      b.writeBigUInt64BE(BigInt(bytes.length));
      return b;
    })(),
    Buffer.from([0]),
  ]);
  return Buffer.concat([head, bytes]);
};
const partsInput = Buffer.concat([
  framed("adapter.mjs", bytes),
  framed("adapter.manifest.json", manifestCanonical),
].map((p) => {
  const b = Buffer.alloc(8);
  b.writeBigUInt64BE(BigInt(p.length));
  return Buffer.concat([b, p]);
}));
const packageDigest = createHash("sha256").update(partsInput).digest("hex");
if (manifest.integrity.packageDigest !== "sha256:" + packageDigest) {
  fail(
    "package-digest",
    `manifest=${manifest.integrity.packageDigest} actual=sha256:${packageDigest}`,
  );
}

// 4. No scripts by default is structural.
if (manifest.hooks.length !== 0) fail("hooks", "hooks must be empty");
if (manifest.permissions.network.mode !== "denied") fail("network", manifest.permissions.network.mode);

process.stdout.write(
  JSON.stringify({ ok: true, gate: "adapter-manifest-golden", bytes: bytes.length }) + "\n",
);
