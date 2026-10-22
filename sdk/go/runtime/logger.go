// The bounded, redacted stderr logger (issue #106): logs never touch
// stdout, never carry raw request, source, or environment content, and
// stay under the bounded stream the host captures.
package runtime

import (
	"fmt"
	"os"
	"strings"
	"sync/atomic"
	"time"
)

const (
	maxMessageChars = 256
	maxStderrBytes  = 65536
)

var emittedBytes atomic.Uint64

func emit(level, tag, message string) {
	if message == "" || strings.ContainsAny(message, "\n\r") {
		return
	}
	text := message
	if len([]rune(text)) > maxMessageChars {
		text = string([]rune(text)[:maxMessageChars])
	}
	line := fmt.Sprintf("%s %s[%s]: %s\n", time.Now().UTC().Format("06000102T150405Z"), level, tag, text)
	if emittedBytes.Add(uint64(len(line))) > maxStderrBytes {
		return
	}
	_, _ = os.Stderr.WriteString(line)
}

// Debug logs one bounded debug line.
func Debug(tag, message string) { emit("debug", tag, message) }

// Error logs one bounded error line.
func Error(tag, message string) { emit("error", tag, message) }

// Info logs one bounded info line.
func Info(tag, message string) { emit("info", tag, message) }

// Warn logs one bounded warning line.
func Warn(tag, message string) { emit("warn", tag, message) }

// IsLoggableToken reports whether a redaction candidate is safe to log.
func IsLoggableToken(value string) bool {
	return value != "" && len([]rune(value)) <= 128 && !strings.ContainsAny(value, "\n\r")
}
