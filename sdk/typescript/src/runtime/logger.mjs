/**
 * The bounded, redacted stderr logger (issue #106).
 *
 * Logs never touch stdout (the response channel), never carry raw
 * request, source, or environment content, and stay under the same
 * bounded stream the host captures for diagnostics evidence.
 */
import { utf8Length } from "./json.mjs";

const MAX_MESSAGE_CHARS = 256;
const MAX_STDERR_BYTES = 65_536;

let emittedBytes = 0;

/**
 * Create a tagged logger. Levels are fixed; the message must already be
 * redacted — pass bounded tokens and counts, never payloads.
 */
export function stderrLogger(tag) {
  const safeTag = typeof tag === "string" && tag.length > 0 && tag.length <= 64 ? tag : "adapter";
  return {
    debug(message) {
      emit(`debug[${safeTag}]`, message);
    },
    error(message) {
      emit(`error[${safeTag}]`, message);
    },
    info(message) {
      emit(`info[${safeTag}]`, message);
    },
    warn(message) {
      emit(`warn[${safeTag}]`, message);
    },
  };
}

function emit(level, message) {
  if (typeof message !== "string" || message === "") return;
  const text = message.length > MAX_MESSAGE_CHARS ? `${message.slice(0, MAX_MESSAGE_CHARS)}…` : message;
  if (/[\r\n]/.test(text)) return;
  const line = `${new Date().toISOString()} ${level}: ${text}\n`;
  const bytes = utf8Length(line);
  if (emittedBytes + bytes > MAX_STDERR_BYTES) return;
  emittedBytes += bytes;
  process.stderr.write(line);
}

/** Whether a redaction candidate is safe to log: bounded and single-line. */
export function isLoggableToken(value) {
  return typeof value === "string" && value.length > 0 && value.length <= 128 && !/[\r\n]/.test(value);
}
