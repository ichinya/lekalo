/**
 * Logical path, scope, and protected-home helpers (issue #106).
 *
 * These are defense in depth for adapter authors: they mirror the host's
 * closed grammar exactly — portable lowercase segments joined by `/`,
 * optional trailing `/**` recursion on scopes, no absolute paths, no
 * backslashes, no drive letters, no traversal segment, no DOS devices or
 * short names, NFC-normalized segments — but they can never widen host
 * policy: only the host's private view and confinement enforce reality.
 */

const MAX_VALUE_BYTES = 512;
const MAX_SEGMENT_BYTES = 64;

const PROTECTED_HOMES = [
  ["lekalo-model", ["lekalo"]],
  ["lekalo-lockfile", ["lekalo.lock"]],
  ["ir", [".lekalo", "ir"]],
  ["cache", [".lekalo", "cache"]],
  ["import", [".lekalo", "import"]],
  ["privacy", [".lekalo", "privacy"]],
  ["consumer", [".lekalo", "consumer"]],
  ["openspec", ["openspec"]],
];

const DOS_DEVICES = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
]);

/** Whether a segment carries a DOS short-name `FOO~1` spelling. */
function containsShortName(segment) {
  const tilt = segment.indexOf("~");
  return tilt !== -1 && tilt >= 1 && tilt <= 6 && segment.length > tilt && /^\d/.test(segment.slice(tilt + 1));
}

const PERCENT_ESCAPE = /%[0-9a-fA-F]{2}/;

/** The uppercase DOS-reserved stem of one segment (before any extension). */
function dosStem(segment) {
  const dot = segment.indexOf(".");
  const stem = dot === -1 ? segment : segment.slice(0, dot);
  return stem.toUpperCase();
}

function isNfc(value) {
  return value.normalize("NFC") === value;
}

/** The closed host path-violation reasons, mirrored for local checks. */
export function pathViolation(path) {
  if (path === "") return "structure.path-empty";
  if (!isNfc(path)) return "structure.path-not-nfc";
  if (path.startsWith("/") || path.startsWith("\\\\") || path.startsWith("//")) return "structure.path-absolute";
  if (/^[a-zA-Z]:/.test(path) || /^[a-z][a-z0-9+.-]*:/i.test(path) || path.startsWith("~")) return "structure.path-absolute";
  if (path.includes("\\") || PERCENT_ESCAPE.test(path) || path.includes(":")) return "structure.path-escape";
  for (const segment of path.split("/")) {
    if (segment === "" || segment.length > 64) return "structure.path-segment";
    if (segment === "." || segment === ".." || segment.endsWith(".") || segment.endsWith(" ")) return "structure.path-traversal";
    if (DOS_DEVICES.has(dosStem(segment))) return "structure.path-device";
    if (containsShortName(segment)) return "structure.path-short-name";
    if (/[A-Z]/.test(segment)) return "structure.path-case";
  }
  return null;
}

function segmentOk(segment) {
  const portable = segment.startsWith(".") ? `x${segment.slice(1)}` : segment;
  if (segment === "." || segment === ".." || segment.endsWith(".")) return false;
  if (pathViolation(portable) !== null) return false;
  if (segment === "" || segment.length > MAX_SEGMENT_BYTES) return false;
  const first = segment[0];
  if (!(first >= "a" && first <= "z") && !(first >= "0" && first <= "9") && first !== ".") return false;
  return [...segment].every((c) => (c >= "a" && c <= "z") || (c >= "0" && c <= "9") || c === "." || c === "-" || c === "_");
}

function segments(value) {
  if (typeof value !== "string" || value === "" || value.length > MAX_VALUE_BYTES || value.startsWith("/")) return null;
  const parts = value.split("/");
  if (parts.some((p) => p === "")) return null;
  return parts;
}

/** Whether the value is a grammatical logical path (no recursion marker). */
export function isLogicalPath(value) {
  const parts = segments(value);
  if (parts === null) return false;
  return parts.every((p) => segmentOk(p));
}

/** Whether the value is a grammatical scope (final segment may be `**`). */
export function isScope(value) {
  const parts = segments(value);
  if (parts === null) return false;
  const last = parts[parts.length - 1];
  const head = parts.slice(0, -1);
  if (last === "**") return head.length > 0 && head.every((p) => segmentOk(p));
  return parts.every((p) => segmentOk(p));
}

/** Prefix-exact or recursive coverage of one path by one scope. */
export function scopeCovers(scope, path) {
  if (!isScope(scope) || !isLogicalPath(path)) return false;
  const scopeParts = segments(scope);
  const pathParts = segments(path);
  const recursive = scopeParts[scopeParts.length - 1] === "**";
  const prefix = recursive ? scopeParts.slice(0, -1) : scopeParts;
  if (recursive) {
    return pathParts.length > prefix.length && prefix.every((p, index) => pathParts[index] === p);
  }
  return pathParts.length === prefix.length && prefix.every((p, index) => pathParts[index] === p);
}

/** The protected home a concrete path sits in, if any. */
export function protectedHome(path) {
  const parts = segments(path);
  if (parts === null) return null;
  for (const [name, home] of PROTECTED_HOMES) {
    if (parts.length >= home.length && home.every((h, index) => parts[index] === h)) return name;
  }
  return null;
}

/** The protected home a declared write scope would reach, if any. */
export function scopeTouchesProtectedHome(scope) {
  const parts = segments(scope);
  if (parts === null) return null;
  const recursive = parts[parts.length - 1] === "**";
  const prefix = recursive ? parts.slice(0, -1) : parts;
  for (const [name, home] of PROTECTED_HOMES) {
    const inside = prefix.length >= home.length && home.every((h, index) => prefix[index] === h);
    const above = prefix.length <= home.length && prefix.every((p, index) => home[index] === p);
    if (inside || above) return name;
  }
  return null;
}

/** Whether one adapter identity id/target/profile token is grammatical. */
export function isToken(value) {
  if (typeof value !== "string" || value === "") return false;
  if (value.length > 64) return false;
  const first = value[0];
  if (!(first >= "a" && first <= "z")) return false;
  return [...value].every((c) => (c >= "a" && c <= "z") || (c >= "0" && c <= "9") || c === "-");
}

/** The production ai-lint identifier grammar (issue #106 review r3 #5):
 * core `ai_lint::input::token` gates the lint scope items, binding
 * nativeId/symbol, and the revision pin's known value — non-empty, at
 * most 256 UTF-8 bytes, ASCII alphanumeric or `_.:/#@\\-`, starting
 * alphanumeric. The SDKs used to enforce a length bound only. */
export function isWireToken(value) {
  if (typeof value !== "string" || value === "") return false;
  if (Buffer.byteLength(value, "utf8") > 256) return false;
  if (!/^[A-Za-z0-9]/.test(value)) return false;
  return /^[A-Za-z0-9_.:\/#@\\-]*$/.test(value);
}

/** The closed capability-id grammar (lowercase dotted segments). */
export function isCapabilityId(value) {
  if (typeof value !== "string" || value === "" || value.length > 128) return false;
  return value.split(".").every((segment) => {
    if (segment === "") return false;
    const first = segment[0];
    if (!(first >= "a" && first <= "z") && !(first >= "0" && first <= "9")) return false;
    return [...segment].every((c) => (c >= "a" && c <= "z") || (c >= "0" && c <= "9") || c === "-" || c === "_");
  });
}

/** The bounded contract-version spelling `x.y.z` (all-numeric parts). */
export function isContractVersion(value) {
  if (typeof value !== "string" || value === "" || value.length > 32) return false;
  const parts = value.split(".");
  if (parts.length !== 3) return false;
  return parts.every((part) => part !== "" && [...part].every((c) => c >= "0" && c <= "9"));
}

/** One write-scope violation of a plan entry, or null. */
export function planScopeViolation(entry, writeScopes) {
  const home = protectedHome(entry.path);
  if (home !== null) return `protected-path:${home}`;
  if (writeScopes.some((scope) => scopeCovers(scope, entry.path))) return null;
  return "scope:uncovered";
}
