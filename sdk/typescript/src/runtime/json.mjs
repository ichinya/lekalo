/**
 * Strict JSON admission for the adapter SDK (issue #106).
 *
 * The wire is closed: duplicate decoded keys (including escaped
 * spellings), trailing documents, BOMs, and non-finite numbers are
 * refused before any value map can collapse them. Canonical output is
 * byte-sorted compact JSON with no trailing LF — exactly the core's
 * serializer contract.
 */

/** Raw lexer pass: returns the duplicated key spellings, if any. */
export function duplicateKeys(text) {
  const offenders = [];
  const stack = [{ keys: null }];
  let index = 0;
  const skipWhitespace = () => {
    while (index < text.length && " \t\r\n".includes(text[index])) index += 1;
  };
  while (index < text.length) {
    const ch = text[index];
    if (ch === '"') {
      const start = index;
      index += 1;
      while (index < text.length) {
        const c = text[index];
        if (c === "\\") {
          index += 2;
          continue;
        }
        if (c === '"') {
          index += 1;
          break;
        }
        index += 1;
      }
      let key;
      try {
        key = JSON.parse(text.slice(start, index));
      } catch {
        return ["<malformed-string>"];
      }
      skipWhitespace();
      if (text[index] === ":") {
        index += 1;
        const frame = stack[stack.length - 1];
        if (frame.keys !== null) {
          if (frame.keys.has(key)) offenders.push(key);
          frame.keys.add(key);
        }
      }
      continue;
    }
    if (ch === "{") {
      stack.push({ keys: new Set() });
      index += 1;
      continue;
    }
    if (ch === "}") {
      if (stack.length > 1) stack.pop();
      index += 1;
      continue;
    }
    index += 1;
  }
  return offenders;
}

/**
 * Strict parse of one complete JSON document: refuses BOM, trailing
 * documents, duplicate keys (including escaped spellings), and
 * non-finite numbers.
 */
export function strictParse(text) {
  if (typeof text !== "string") return { ok: false, reason: "not-json" };
  if (text.startsWith("\uFEFF")) return { ok: false, reason: "not-json" };
  const first = text.replace(/^[\s]+/, "")[0];
  if (first !== "{" && first !== "[") return { ok: false, reason: "not-json" };
  const end = documentEnd(text);
  if (end === -1) return { ok: false, reason: "not-json" };
  if (text.slice(end + 1).trim().length > 0) return { ok: false, reason: "trailing-document" };
  const document = text.slice(0, end + 1);
  let value;
  try {
    value = JSON.parse(document);
  } catch {
    return { ok: false, reason: "not-json" };
  }
  if (duplicateKeys(document).length > 0) return { ok: false, reason: "duplicate-key" };
  if (/(?<!\\)(NaN|-?Infinity)(?!["'])/.test(document)) return { ok: false, reason: "not-json" };
  return { ok: true, value };
}

/** The index of the closing bracket of the first document, or -1. */
function documentEnd(text) {
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let index = 0; index < text.length; index += 1) {
    const ch = text[index];
    if (inString) {
      if (escaped) escaped = false;
      else if (ch === "\\") escaped = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (ch === '"') inString = true;
    else if (ch === "{" || ch === "[") depth += 1;
    else if (ch === "}" || ch === "]") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

/** Byte length of a UTF-8 encoding of the text. */
export function utf8Length(text) {
  return Buffer.byteLength(text, "utf8");
}

/**
 * Read exactly one bounded UTF-8 JSON object document; oversize input is
 * refused before parsing.
 */
export function readSingleObject(text, maxBytes) {
  if (utf8Length(text) > maxBytes) return { ok: false, reason: "oversize" };
  const parsed = strictParse(text);
  if (!parsed.ok) return parsed;
  if (parsed.value === null || typeof parsed.value !== "object" || Array.isArray(parsed.value)) {
    return { ok: false, reason: "shape" };
  }
  return parsed;
}

/** Canonical compact JSON: sorted keys, no whitespace, no trailing LF. */
export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}
