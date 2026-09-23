/**
 * Deterministic YAML emitter for the OpenAPI generator (issue #46,
 * plan §4/§8). Input is the canonical document tree of
 * `openapi-gen.mjs`; output is one block-style YAML document with:
 *
 * - byte-sorted keys at every level (the canonical JSON order);
 * - 2-space indentation, LF endings, no trailing whitespace, exactly
 *   one final newline;
 * - every string key and string scalar double-quoted (JSON spelling):
 *   unambiguous, and the quoted `openapi: "3.1.0"` version stays a
 *   string through the closed import frontend;
 * - the two empty flow literals `{}` and `[]`, which block-style YAML
 *   cannot spell, exactly matching the bounded import exception;
 * - no anchors, no aliases, no tags, no flow content, no
 *   multi-document streams.
 */

/**
 * Canonical compact JSON: object keys sorted by unsigned UTF-8 byte
 * order, no whitespace. Mirrors the core serializer byte for byte.
 */
export function canonicalJson(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) throw new TypeError("non-finite number");
      return Number.isInteger(value) && Math.abs(value) < 1e15
        ? String(value)
        : JSON.stringify(value);
    case "string":
      return JSON.stringify(value);
    case "object": {
      if (Array.isArray(value)) {
        return `[${value.map(canonicalJson).join(",")}]`;
      }
      const keys = Object.keys(value)
        .filter((key) => value[key] !== undefined)
        .sort(byUtf8Bytes);
      return `{${keys
        .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
        .join(",")}}`;
    }
    default:
      throw new TypeError("unserializable value");
  }
}

/** Unsigned UTF-8 byte order comparison (the canonical key order). */
export function byUtf8Bytes(left, right) {
  const leftBytes = Buffer.from(left, "utf8");
  const rightBytes = Buffer.from(right, "utf8");
  const length = Math.min(leftBytes.length, rightBytes.length);
  for (let index = 0; index < length; index += 1) {
    if (leftBytes[index] !== rightBytes[index]) {
      return leftBytes[index] - rightBytes[index];
    }
  }
  return leftBytes.length - rightBytes.length;
}

/**
 * Emit the exact YAML text of one document tree. Pure: the same tree
 * always produces the same bytes.
 */
export function toYaml(value) {
  const lines = [];
  emitValue(value, 0, lines, "$");
  return `${lines.join("\n")}\n`;
}

/** One nested emission step. */
function emitValue(value, depth, lines, at) {
  if (isEmptyContainer(value)) {
    lines.push(`${indent(depth)}${inlineEmpty(value)}`);
    return;
  }
  if (Array.isArray(value)) {
    for (const [index, item] of value.entries()) {
      emitSequenceItem(item, depth, lines, `${at}[${index}]`);
    }
    return;
  }
  if (value !== null && typeof value === "object") {
    for (const key of Object.keys(value).sort(byUtf8Bytes)) {
      emitMember(key, value[key], depth, lines, `${at}.${key}`);
    }
    return;
  }
  lines.push(`${indent(depth)}${scalar(value, at)}`);
}

/** One `key: value` or `key:` + nested block member. */
function emitMember(key, value, depth, lines, at) {
  const name = scalar(key, `${at}::key`);
  if (isEmptyContainer(value)) {
    lines.push(`${indent(depth)}${name}: ${inlineEmpty(value)}`);
    return;
  }
  if (isContainer(value)) {
    lines.push(`${indent(depth)}${name}:`);
    emitValue(value, depth + 1, lines, at);
    return;
  }
  lines.push(`${indent(depth)}${name}: ${scalar(value, at)}`);
}

/** One `- value` or `-` + nested block sequence item. */
function emitSequenceItem(value, depth, lines, at) {
  if (value !== null && typeof value === "object" && !isEmptyContainer(value)) {
    if (!Array.isArray(value)) {
      // The nested mapping of a sequence item: the first key shares
      // the `-` line, the remaining keys indent by one level.
      const keys = Object.keys(value).sort(byUtf8Bytes);
      const [first, ...rest] = keys;
      const name = scalar(first, `${at}::key`);
      const head = value[first];
      if (isContainer(head) && !isEmptyContainer(head)) {
        lines.push(`${indent(depth)}- ${name}:`);
        emitValue(head, depth + 2, lines, `${at}.${first}`);
      } else if (isEmptyContainer(head)) {
        lines.push(`${indent(depth)}- ${name}: ${inlineEmpty(head)}`);
      } else {
        lines.push(`${indent(depth)}- ${name}: ${scalar(head, `${at}.${first}`)}`);
      }
      for (const key of rest) {
        emitMember(key, value[key], depth + 1, lines, `${at}.${key}`);
      }
      return;
    }
    lines.push(`${indent(depth)}-`);
    emitValue(value, depth + 1, lines, at);
    return;
  }
  if (isEmptyContainer(value)) {
    lines.push(`${indent(depth)}- ${inlineEmpty(value)}`);
    return;
  }
  lines.push(`${indent(depth)}- ${scalar(value, at)}`);
}

/** Whether the value is `{}` or `[]`. */
function isEmptyContainer(value) {
  if (Array.isArray(value)) return value.length === 0;
  return value !== null && typeof value === "object"
    ? Object.keys(value).length === 0
    : false;
}

/** Whether the value is a non-empty array or object. */
function isContainer(value) {
  if (Array.isArray(value)) return value.length > 0;
  return value !== null && typeof value === "object"
    ? Object.keys(value).length > 0
    : false;
}

/** The inline spelling of an empty container. */
function inlineEmpty(value) {
  return Array.isArray(value) ? "[]" : "{}";
}

/** The indentation of one depth level. */
function indent(depth) {
  return "  ".repeat(depth);
}

/** The exact scalar spelling: JSON quoting for every string. */
function scalar(value, at) {
  if (typeof value === "string") return JSON.stringify(value);
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new TypeError(`non-finite number at ${at}`);
    return String(value);
  }
  throw new TypeError(`unserializable scalar at ${at}: ${typeof value}`);
}

/**
 * The closed reader for the emitted dialect: the exact inverse of
 * [`toYaml`] over the bounded block-style subset (2-space indents,
 * JSON-quoted keys and strings, bare numbers/booleans/null, the two
 * inline empty flow literals, `- ` sequence items whose nested mapping
 * shares the dash line). Anything else — comments, block scalars,
 * anchors, flow content, tabs, bare scalars — throws `YamlReadError`,
 * so a document the generator cannot own is refused, never re-written
 * from a guess (issue #46 r1 F-7/cline F-2). The round-trip law:
 * `fromYaml(toYaml(value))` deep-equals `value`.
 */
export function fromYaml(text) {
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  for (const line of lines) {
    if (line.includes("\t")) throw new YamlReadError("tab-indentation");
    if (/^\s*#/.test(line)) throw new YamlReadError("comment");
  }
  const [value, next] = parseNode(lines, 0, 0);
  if (next !== lines.length) throw new YamlReadError("trailing-content");
  return value;
}

/** The refusal of one out-of-dialect document. */
export class YamlReadError extends Error {
  constructor(reason) {
    super(reason);
    this.name = "YamlReadError";
    this.reason = reason;
  }
}

/** The leading-space indent of one line. */
function indentOf(line) {
  const match = /^ */.exec(line);
  return match[0].length;
}

/** Parse one block node at `minimum` indent; returns `[value, nextIndex]`. */
function parseNode(lines, index, minimum) {
  if (index >= lines.length) throw new YamlReadError("unexpected-end");
  const line = lines[index];
  const indent = indentOf(line);
  if (indent < minimum) throw new YamlReadError("unexpected-dedent");
  const content = line.slice(indent);
  if (content === "-" || content.startsWith("- ")) {
    return parseSequence(lines, index, indent);
  }
  if (/^"(?:[^"\\]|\\.)*":(?: |$)/.test(content)) {
    return parseMapping(lines, index, indent);
  }
  throw new YamlReadError("unexpected-line");
}

/** Parse one block mapping whose members sit at `indent`. */
function parseMapping(lines, index, indent) {
  const object = {};
  let at = index;
  while (at < lines.length) {
    const line = lines[at];
    const here = indentOf(line);
    if (here < indent) break;
    if (here > indent) throw new YamlReadError("bad-indent");
    const content = line.slice(indent);
    const match = /^("(?:[^"\\]|\\.)*"):(?: (.*))?$/.exec(content);
    if (!match) throw new YamlReadError("key-shape");
    const key = JSON.parse(match[1]);
    const rest = match[2];
    at += 1;
    if (rest === undefined || rest === "") {
      // The nested block member (the emitter never writes a bare key
      // without one: empties stay inline).
      const [value, next] = parseNode(lines, at, indent + 1);
      object[key] = value;
      at = next;
      continue;
    }
    object[key] = parseInline(rest);
  }
  return [object, at];
}

/** Parse one block sequence whose `- ` items sit at `indent`. */
function parseSequence(lines, index, indent) {
  const array = [];
  let at = index;
  while (at < lines.length) {
    const line = lines[at];
    const here = indentOf(line);
    if (here !== indent || !(line.slice(indent) === "-" || line.slice(indent).startsWith("- "))) {
      break;
    }
    const after = line.slice(indent + 2);
    at += 1;
    if (after === "") {
      const [value, next] = parseNode(lines, at, indent + 1);
      array.push(value);
      at = next;
      continue;
    }
    if (after === "{}" || after === "[]") {
      array.push(parseInline(after));
      continue;
    }
    if (/^"(?:[^"\\]|\\.)*":/.test(after)) {
      // The nested mapping of one item: its first member shares the
      // dash line; the remaining members sit at dash indent + 2.
      lines[at - 1] = " ".repeat(indent + 2) + after;
      const [value, next] = parseMapping(lines, at - 1, indent + 2);
      array.push(value);
      at = next;
      continue;
    }
    array.push(parseInline(after));
  }
  return [array, at];
}

/** One inline scalar or empty-flow literal at a value position. */
function parseInline(token) {
  if (token === "{}" || token === "[]") {
    return JSON.parse(token);
  }
  if (token === "null") return null;
  if (token === "true") return true;
  if (token === "false") return false;
  if (/^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?$/.test(token)) return Number(token);
  if (/^"(?:[^"\\]|\\.)*"$/.test(token)) return JSON.parse(token);
  throw new YamlReadError("scalar-spelling");
}
