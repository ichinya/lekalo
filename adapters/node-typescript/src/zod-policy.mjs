/**
 * Codegen policy resolution for the Zod generator (issue #45, plan §3.2).
 *
 * Policies live in the adapter-owned opaque target document
 * `lekalo/targets/node-typescript.yaml`, which the operator maintains
 * inside the project root and the resolved profile exposes as a read
 * root. The closed grammar is exactly:
 *
 *   zod:
 *     date: date-string | date-native
 *     unknown-keys: strict | strip
 *
 * Comments (`#`) and blank lines are ignored. Anything else — unknown
 * keys, unknown values, deeper nesting, tabs, duplicate keys — is a
 * refusal, never a silent fallback to defaults: an absent file resolves
 * the documented defaults, but a malformed present file fails the
 * operation in-envelope (plan §3.2, R4).
 */

import { DATE_POLICIES, DEFAULT_POLICY, UNKNOWN_KEY_POLICIES } from "./zod-map.mjs";

/** The read-root logical path of the policy document. */
export const POLICY_PATH = "lekalo/targets/node-typescript.yaml";

/** The maximum policy document size; anything larger is a refusal. */
export const MAX_POLICY_BYTES = 16 * 1024;

/**
 * Resolve one policy document's text (or `null` when the file is
 * absent) into a closed `{date, unknownKeys}` policy.
 * Returns `{ policy, source }` on success or `{ refusal }` with a
 * bounded reason token on any violation.
 */
export function resolvePolicy(text) {
  if (text === null || text === undefined) {
    return { policy: { ...DEFAULT_POLICY }, source: "defaults" };
  }
  if (typeof text !== "string") {
    return { refusal: "not-text" };
  }
  const bytes = Buffer.byteLength(text, "utf8");
  if (bytes > MAX_POLICY_BYTES) {
    return { refusal: "overbound" };
  }
  const parsed = parsePolicyYaml(text);
  if (parsed.refusal) {
    return { refusal: parsed.refusal };
  }
  return { policy: parsed.policy, source: "document" };
}

/**
 * Parse the closed two-key grammar. Returns `{policy}` or `{refusal}`.
 * Indentation is exactly two spaces for the key block; tabs are refused
 * (the closed YAML dialect of every target document in this repo).
 */
export function parsePolicyYaml(text) {
  const lines = text.split(/\r?\n/);
  let inZod = false;
  const seen = new Set();
  const policy = {};
  for (let index = 0; index < lines.length; index += 1) {
    const raw = lines[index];
    const stripped = raw.replace(/(^|\s)#.*$/, "");
    if (stripped.trim() === "") {
      continue;
    }
    if (stripped.includes("\t")) {
      return { refusal: "tab-indentation" };
    }
    if (!stripped.startsWith(" ") && !stripped.startsWith("-")) {
      const match = stripped.match(/^([a-z][a-z0-9_-]*):\s*$/);
      if (!match) {
        return { refusal: "top-level-key" };
      }
      if (match[1] !== "zod") {
        // A different top-level section is foreign to this document.
        return { refusal: "unknown-section" };
      }
      if (inZod) {
        return { refusal: "duplicate-section" };
      }
      inZod = true;
      continue;
    }
    if (!inZod) {
      return { refusal: "orphan-key" };
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
    if (key === "date") {
      if (!DATE_POLICIES.includes(value)) {
        return { refusal: "date-value" };
      }
      policy.date = value;
      continue;
    }
    if (key === "unknown-keys") {
      if (!UNKNOWN_KEY_POLICIES.includes(value)) {
        return { refusal: "unknown-keys-value" };
      }
      policy.unknownKeys = value;
      continue;
    }
    return { refusal: "unknown-key" };
  }
  return {
    policy: { ...DEFAULT_POLICY, ...policy },
  };
}
