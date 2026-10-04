// Issue #32 step 12: the shipped adapters' committed manifests must
// recompute exactly against the committed artifact bytes — the golden
// manifest check. Issue #54 extends the gate to the second adapter
// (`lekalo-target-php-laravel`), whose entry mirrors the same digest
// domain. Dependency-free; run from the repo root.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

/**
 * Verify one adapter's manifest against its committed entry bytes: the
 * identity/compatibility pins, the entry digest and length, and the
 * package digest recomputation over the framed per-file byte set.
 */
const verifyAdapter = (directory, adapterId, entryName, irVersion) => {
  const bytes = readFileSync(new URL(`${directory}/${entryName}`, root));
  const manifest = JSON.parse(
    readFileSync(new URL(`${directory}/adapter.manifest.json`, root), "utf8"),
  );

  // 1. Identity and compatibility pins.
  if (manifest.schemaVersion !== "lekalo/adapter-manifest/v0.6.4") {
    fail("schema-version", manifest.schemaVersion);
  }
  if (manifest.identity !== "dev.lekalo.adapter-manifest@0.6.4") {
    fail("identity", manifest.identity);
  }
  if (manifest.adapter.id !== adapterId) {
    fail("adapter-id", manifest.adapter.id);
  }
  if (!manifest.compatibility.protocolVersions.includes("0.3.2")) {
    fail("protocol-compatibility", manifest.compatibility.protocolVersions.join(","));
  }
  if (!manifest.compatibility.irVersions.includes(irVersion)) {
    fail("ir-compatibility", manifest.compatibility.irVersions.join(","));
  }

  if (!manifest.compatibility.protocolVersions.includes("0.6.4") || !manifest.capabilities.operations.includes("lint")) fail("lint-compatibility", adapterId);
  // 2. The entry digest must match the committed artifact bytes exactly.
  const entry = manifest.integrity.files.find((file) => file.path === entryName);
  if (!entry) fail("entry-missing", `${entryName} is not in integrity.files`);
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
    framed(entryName, bytes),
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
  return bytes.length;
};

const nodeBytes = verifyAdapter(
  "adapters/node-typescript",
  "lekalo-target-node-typescript",
  "adapter.mjs",
  "0.2.16",
);
const phpBytes = verifyAdapter(
  "adapters/php-laravel",
  "lekalo-target-php-laravel",
  "adapter.php",
  "0.2.16",
);

process.stdout.write(
  JSON.stringify({
    ok: true,
    gate: "adapter-manifest-golden",
    adapters: {
      "lekalo-target-node-typescript": { bytes: nodeBytes },
      "lekalo-target-php-laravel": { bytes: phpBytes },
    },
  }) + "\n",
);
