/**
 * Policy resolution for the OpenAPI generator (issue #46, plan §3).
 *
 * Policies live in the adapter-owned opaque target document
 * `lekalo/targets/node-typescript.yaml`, which the operator maintains
 * inside the project root and the resolved profile exposes as a read
 * root — the same document the Zod policy block lives in (the #45
 * precedent). The closed grammar is exactly:
 *
 *   zod:                      (owned by the Zod policy, tolerated)
 *     ...
 *   openapi:
 *     version: "3.1" | "3.0"
 *     mode: full | fragments
 *     path: <bounded token>
 *
 * Comments (`#`) and blank lines are ignored. Anything else — unknown
 * keys, unknown values, deeper nesting, tabs, duplicate keys, unknown
 * top-level sections — is a refusal, never a silent fallback to
 * defaults: an absent file (or an absent `openapi` block) resolves the
 * documented defaults, but a malformed present file fails the
 * operation in-envelope.
 */

/** The read-root logical path of the policy document. */
export const POLICY_PATH = "lekalo/targets/node-typescript.yaml";

/** The maximum policy document size; anything larger is a refusal. */
export const MAX_POLICY_BYTES = 16 * 1024;

/** The closed version vocabulary. */
export const VERSIONS = ["3.1", "3.0"];
/** The closed mode vocabulary. */
export const MODES = ["full", "fragments"];
/** The default policy of an absent document or absent block. */
export const DEFAULT_POLICY = Object.freeze({
  version: "3.1",
  mode: "full",
  path: "docs/openapi.yaml",
});

/**
 * Resolve one policy document's text (or `null` when the file is
 * absent) into a closed `{version, mode, path}` policy. Returns
 * `{ policy, source }` on success or `{ refusal }` with a bounded
 * reason token on any violation.
 */
export function resolvePolicy(text) {
  if (text === null || text === undefined) {
    return { policy: { ...DEFAULT_POLICY }, source: "defaults" };
  }
  if (typeof text !== "string") {
    return { refusal: "not-text" };
  }
  if (Buffer.byteLength(text, "utf8") > MAX_POLICY_BYTES) {
    return { refusal: "overbound" };
  }
  const parsed = parsePolicyYaml(text);
  if (parsed.refusal) {
    return { refusal: parsed.refusal };
  }
  return { policy: parsed.policy, source: "document" };
}

/**
 * Parse the closed grammar. Returns `{policy}` or `{refusal}`. The
 * `zod` section belongs to the Zod generator (issue #45) and is
 * tolerated but never interpreted here; a malformed `zod` block is
 * the Zod policy's refusal, not this one's.
 */
export function parsePolicyYaml(text) {
  const lines = text.split(/\r?\n/);
  let section = null;
  const seen = new Set();
  const policy = {};
  for (const raw of lines) {
    const stripped = raw.replace(/(^|\s)#.*$/, "");
    if (stripped.trim() === "") {
      continue;
    }
    if (stripped.includes("\t")) {
      return { refusal: "tab-indentation" };
    }
    if (!stripped.startsWith(" ") && !stripped.startsWith("-")) {
      const match = stripped.match(/^([a-z][a-z0-9_-]*):\s*(\S.*)?$/);
      if (!match) {
        return { refusal: "top-level-key" };
      }
      if (match[1] !== "zod" && match[1] !== "openapi") {
        // A different top-level section is foreign to this document.
        return { refusal: "unknown-section" };
      }
      if (match[2] !== undefined) {
        return { refusal: "key-shape" };
      }
      if (section === match[1]) {
        return { refusal: "duplicate-section" };
      }
      section = match[1];
      continue;
    }
    if (section === null) {
      return { refusal: "orphan-key" };
    }
    // The zod block belongs to the other generator; its members are
    // skipped here (its own policy parser owns the closed grammar).
    if (section === "zod") {
      continue;
    }
    const keyMatch = stripped.match(/^ {2}([a-z][a-z0-9_-]*):\s*(\S.*)?$/);
    if (!keyMatch || stripped.startsWith("    ")) {
      return { refusal: "key-shape" };
    }
    const key = keyMatch[1];
    const value = keyMatch[2]?.trim();
    if (seen.has(key)) {
      return { refusal: "duplicate-key" };
    }
    seen.add(key);
    if (key === "version") {
      if (!VERSIONS.includes(unquote(value))) {
        return { refusal: "version-value" };
      }
      policy.version = unquote(value);
      continue;
    }
    if (key === "mode") {
      if (!MODES.includes(value)) {
        return { refusal: "mode-value" };
      }
      policy.mode = value;
      continue;
    }
    if (key === "path") {
      const path = unquote(value);
      if (!/^[a-z][a-z0-9._/-]*\.yaml$/.test(path) || path.includes("..")) {
        return { refusal: "path-value" };
      }
      policy.path = path;
      continue;
    }
    return { refusal: "unknown-key" };
  }
  return { policy: { ...DEFAULT_POLICY, ...policy } };
}

/** Strip one layer of matching double quotes (the value spelling). */
function unquote(value) {
  if (value === undefined) {
    return "";
  }
  if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) {
    return value.slice(1, -1);
  }
  return value;
}
