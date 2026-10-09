/**
 * Strict JSON admission for the adapter SDK (issue #106).
 *
 * The wire is closed: duplicate decoded keys (including escaped
 * spellings), trailing documents, BOMs, malformed numbers, and
 * non-finite numbers are refused before any value map can collapse
 * them. Number lexemes are preserved exactly (issue #106 review r3 #3):
 * an integer spelling parses to a JS number, anything else (fraction,
 * exponent, or `+` in the exponent) parses to a `JsonFloat`, so wire
 * decoders can enforce production's serde `u64` syntax boundary by
 * type instead of re-deriving integrality from a rounded value.
 * Canonical output is byte-sorted compact JSON with no trailing LF —
 * exactly the core's serializer contract.
 */

/** A number whose JSON lexeme is not an integer spelling. Production
 * types every wire number `u64` (serde refuses these lexemes with a
 * parse error → `shape`), so admission refuses a JsonFloat wherever an
 * integer member is required, regardless of the parsed value. */
export class JsonFloat {
  constructor(raw) {
    this.raw = raw;
    this.value = Number(raw);
  }
}

const SIMPLE_ESCAPES = { '"': '"', "\\": "\\", "/": "/", b: "\b", f: "\f", n: "\n", r: "\r", t: "\t" };

/** Lexeme-aware strict parse of one complete document: keeps the last
 * duplicate key's value (like JSON.parse) but reports `duplicate-key`,
 * and refuses malformed number spellings positionally. Refusal
 * precedence matches the old JSON.parse-based pass: any syntax error
 * anywhere is `not-json`, duplicates only surface for otherwise-valid
 * documents. */
export function parseDocument(text) {
  let index = 0;
  const length = text.length;
  let duplicated = false;
  const notJson = () => ({ ok: false, reason: "not-json" });
  const skipWhitespace = () => {
    while (index < length && " \t\r\n".includes(text[index])) index += 1;
  };
  const parseString = () => {
    // The opening quote is already consumed; returns the decoded string
    // or null on malformed input.
    let out = "";
    while (index < length) {
      const ch = text[index];
      if (ch === '"') {
        index += 1;
        return out;
      }
      if (ch === "\\") {
        const escape = text[index + 1];
        index += 2;
        if (escape === "u") {
          const hex = text.slice(index, index + 4);
          if (!/^[0-9a-fA-F]{4}$/.test(hex)) return null;
          index += 4;
          let code = parseInt(hex, 16);
          if (code >= 0xd800 && code <= 0xdbff) {
            // Surrogate pair required (JSON.parse accepts a lone high
            // surrogate, so the reference parity keeps it legal).
            if (text[index] === "\\" && text[index + 1] === "u") {
              const lowHex = text.slice(index + 2, index + 6);
              if (!/^[0-9a-fA-F]{4}$/.test(lowHex)) return null;
              const low = parseInt(lowHex, 16);
              if (!(low >= 0xdc00 && low <= 0xdfff)) return null;
              index += 6;
              code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
            }
          }
          out += String.fromCodePoint(code);
        } else if (escape in SIMPLE_ESCAPES) {
          out += SIMPLE_ESCAPES[escape];
        } else {
          return null;
        }
        continue;
      }
      if (text.charCodeAt(index) < 0x20) return null;
      out += ch;
      index += 1;
    }
    return null;
  };
  const parseNumber = () => {
    const start = index;
    if (text[index] === "-") index += 1;
    // Integer part: `0` or [1-9][0-9]* — leading zeros refuse.
    if (text[index] === "0") {
      index += 1;
    } else if (text[index] >= "1" && text[index] <= "9") {
      index += 1;
      while (text[index] >= "0" && text[index] <= "9") index += 1;
    } else {
      return notJson();
    }
    let integral = true;
    if (text[index] === ".") {
      integral = false;
      index += 1;
      if (!(text[index] >= "0" && text[index] <= "9")) return notJson();
      while (text[index] >= "0" && text[index] <= "9") index += 1;
    }
    if (text[index] === "e" || text[index] === "E") {
      integral = false;
      index += 1;
      if (text[index] === "+" || text[index] === "-") index += 1;
      if (!(text[index] >= "0" && text[index] <= "9")) return notJson();
      while (text[index] >= "0" && text[index] <= "9") index += 1;
    }
    const raw = text.slice(start, index);
    const value = Number(raw);
    if (!Number.isFinite(value)) return notJson();
    return { ok: true, value: integral ? value : new JsonFloat(raw) };
  };
  const parseValue = () => {
    skipWhitespace();
    const ch = text[index];
    if (ch === "{") return parseObject();
    if (ch === "[") return parseArray();
    if (ch === '"') {
      index += 1;
      const value = parseString();
      return value === null ? notJson() : { ok: true, value };
    }
    if (text.startsWith("true", index)) {
      index += 4;
      return { ok: true, value: true };
    }
    if (text.startsWith("false", index)) {
      index += 5;
      return { ok: true, value: false };
    }
    if (text.startsWith("null", index)) {
      index += 4;
      return { ok: true, value: null };
    }
    if (ch === "-" || (ch >= "0" && ch <= "9")) return parseNumber();
    return notJson();
  };
  const parseObject = () => {
    index += 1;
    const members = {};
    const seen = new Set();
    skipWhitespace();
    if (text[index] === "}") {
      index += 1;
      return { ok: true, value: members };
    }
    for (;;) {
      skipWhitespace();
      if (text[index] !== '"') return notJson();
      index += 1;
      const key = parseString();
      if (key === null) return notJson();
      skipWhitespace();
      if (text[index] !== ":") return notJson();
      index += 1;
      const member = parseValue();
      if (!member.ok) return member;
      duplicated = duplicated || seen.has(key);
      seen.add(key);
      // Keep the last duplicate's value (JSON.parse semantics); defineProperty
      // keeps `__proto__` an ordinary own member exactly like JSON.parse.
      Object.defineProperty(members, key, { value: member.value, writable: true, enumerable: true, configurable: true });
      skipWhitespace();
      if (text[index] === ",") {
        index += 1;
        continue;
      }
      if (text[index] === "}") {
        index += 1;
        return { ok: true, value: members };
      }
      return notJson();
    }
  };
  const parseArray = () => {
    index += 1;
    const items = [];
    skipWhitespace();
    if (text[index] === "]") {
      index += 1;
      return { ok: true, value: items };
    }
    for (;;) {
      const item = parseValue();
      if (!item.ok) return item;
      items.push(item.value);
      skipWhitespace();
      if (text[index] === ",") {
        index += 1;
        continue;
      }
      if (text[index] === "]") {
        index += 1;
        return { ok: true, value: items };
      }
      return notJson();
    }
  };
  const value = parseValue();
  if (!value.ok) return value;
  skipWhitespace();
  if (index !== length) return { ok: false, reason: "trailing-document" };
  return duplicated ? { ok: false, reason: "duplicate-key" } : { ok: true, value: value.value };
}

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
 * documents, duplicate keys (including escaped spellings), malformed
 * number spellings, and non-finite numbers.
 */
export function strictParse(text) {
  if (typeof text !== "string") return { ok: false, reason: "not-json" };
  if (text.startsWith("\uFEFF")) return { ok: false, reason: "not-json" };
  const first = text.replace(/^[\s]+/, "")[0];
  if (first !== "{" && first !== "[") return { ok: false, reason: "not-json" };
  return parseDocument(text);
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
  if (value instanceof JsonFloat) {
    return Number.isInteger(value.value) ? String(value.value) : JSON.stringify(value.value);
  }
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}
