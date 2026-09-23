// Regenerates adapters/node-typescript/adapter.manifest.json against the
// committed artifact bytes (issue #32, step 12). Run from the repo root:
//   node scripts/regen-adapter-manifest.mjs
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const artifactPath = new URL("adapters/node-typescript/adapter.mjs", root);
const manifestPath = new URL("adapters/node-typescript/adapter.manifest.json", root);

const bytes = readFileSync(artifactPath);
const entryDigest = createHash("sha256").update(bytes).digest("hex");

// The package digest domain (issue #32): for every file in path-sorted
// order, path bytes, one NUL, the 8-byte big-endian byte length, one NUL,
// then the exact file bytes — each part framed by its 8-byte length. The
// manifest entry's contribution is its canonical bytes minus the
// self-referential manifestDigest member.
const framed = (path, bytes) => {
  const head = Buffer.concat([
    Buffer.from(path, "utf8"),
    Buffer.from([0]),
    (() => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(bytes.length)); return b; })(),
    Buffer.from([0]),
  ]);
  return Buffer.concat([head, bytes]);
};

const canonicalManifest = () => {
  const raw = JSON.parse(readFileSync(manifestPath, "utf8"));
  // Strip the self-referential member recursively — the same rule as the
  // Rust verifier's canonical::without_member (fix round 2, devin F-14).
  // A manifestDigest nested inside a Json-typed member must land outside
  // the digest domain on both sides, or JS-authored packages would fail
  // the Rust integrity gate.
  const stripMember = (value) => {
    if (Array.isArray(value)) return value.map(stripMember);
    if (value !== null && typeof value === "object") {
      const next = {};
      for (const [k, v] of Object.entries(value)) {
        if (k !== "manifestDigest") next[k] = stripMember(v);
      }
      return next;
    }
    return value;
  };
  const stripped = stripMember(raw);
  if (stripped.integrity) stripped.integrity.packageDigest = "sha256:" + "0".repeat(64);
  // canonical JSON: sorted keys, compact
  const canonical = (value) => {
    if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
    if (value !== null && typeof value === "object") {
      return "{" + Object.keys(value).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
        .map((k) => JSON.stringify(k) + ":" + canonical(value[k])).join(",") + "}";
    }
    return JSON.stringify(value);
  };
  return Buffer.from(canonical(raw), "utf8");
};

const parts = [
  framed("adapter.mjs", bytes),
  framed("adapter.manifest.json", canonicalManifest()),
];
const digestInput = Buffer.concat(parts.map((p) => {
  const b = Buffer.alloc(8);
  b.writeBigUInt64BE(BigInt(p.length));
  return Buffer.concat([b, p]);
}));
const packageDigest = createHash("sha256").update(digestInput).digest("hex");

const manifestBytes = readFileSync(manifestPath);
const manifest = JSON.parse(manifestBytes.toString("utf8"));
manifest.integrity.packageDigest = "sha256:" + packageDigest;
manifest.integrity.files = manifest.integrity.files.map((file) =>
  file.path === "adapter.mjs"
    ? { path: file.path, digest: "sha256:" + entryDigest, bytes: bytes.length }
    : file
);
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
console.log(JSON.stringify({
  ok: true,
  entryDigest: "sha256:" + entryDigest,
  packageDigest: "sha256:" + packageDigest,
  bytes: bytes.length,
}));
