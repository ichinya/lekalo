// Logical path, scope, and protected-home helpers (issue #106):
// defense in depth mirroring the host's closed grammar.
package runtime

import "strings"

const (
	maxValueBytes   = 512
	maxSegmentBytes = 64
)

var protectedHomes = []struct {
	Name  string
	Segments []string
}{
	{"lekalo-model", []string{"lekalo"}},
	{"lekalo-lockfile", []string{"lekalo.lock"}},
	{"ir", []string{".lekalo", "ir"}},
	{"cache", []string{".lekalo", "cache"}},
	{"import", []string{".lekalo", "import"}},
	{"privacy", []string{".lekalo", "privacy"}},
	{"consumer", []string{".lekalo", "consumer"}},
	{"openspec", []string{"openspec"}},
}

var dosDevices = map[string]bool{
	"CON": true, "PRN": true, "AUX": true, "NUL": true,
	"COM1": true, "COM2": true, "COM3": true, "COM4": true, "COM5": true,
	"COM6": true, "COM7": true, "COM8": true, "COM9": true,
	"LPT1": true, "LPT2": true, "LPT3": true, "LPT4": true, "LPT5": true,
	"LPT6": true, "LPT7": true, "LPT8": true, "LPT9": true,
}

// PathViolation returns the closed host path-violation reason, if any.
// The NFC table check stays host-owned: this runtime treats non-ASCII
// segments as satisfiable and the vector corpus marks those vectors
// requiresNfc, skipped explicitly for runtimes without normalization.
func PathViolation(path string) string {
	if path == "" {
		return "structure.path-empty"
	}
	if strings.HasPrefix(path, "/") || strings.HasPrefix(path, "\\\\") || strings.HasPrefix(path, "//") {
		return "structure.path-absolute"
	}
	if strings.HasPrefix(path, "~") || isDriveOrScheme(path) {
		return "structure.path-absolute"
	}
	if strings.Contains(path, "\\") || containsPercentEscape(path) || strings.Contains(path, ":") {
		return "structure.path-escape"
	}
	for _, segment := range strings.Split(path, "/") {
		if segment == "" || len(segment) > 64 {
			return "structure.path-segment"
		}
		if segment == "." || segment == ".." || strings.HasSuffix(segment, ".") || strings.HasSuffix(segment, " ") {
			return "structure.path-traversal"
		}
		if dosDevices[dosStem(segment)] {
			return "structure.path-device"
		}
		if containsShortName(segment) {
			return "structure.path-short-name"
		}
		for _, c := range segment {
			if c >= 'A' && c <= 'Z' {
				return "structure.path-case"
			}
		}
	}
	return ""
}

func isDriveOrScheme(path string) bool {
	if len(path) >= 2 && isASCIIAlpha(path[0]) && path[1] == ':' {
		return true
	}
	for index := 1; index < len(path); index++ {
		if path[index] == ':' {
			if index == 1 {
				return false
			}
			for _, c := range path[:index] {
				if !(isASCIIAlphanumeric(byte(c)) || c == '+' || c == '.' || c == '-') {
					return false
				}
			}
			return isASCIILower(path[0])
		}
		if !(isASCIIAlphanumeric(path[index]) || path[index] == '+' || path[index] == '.' || path[index] == '-') {
			return false
		}
	}
	return false
}

func isASCIIAlpha(c byte) bool      { return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') }
func isASCIILower(c byte) bool      { return c >= 'a' && c <= 'z' }
func isASCIIAlphanumeric(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
}

func dosStem(segment string) string {
	if dot := strings.IndexByte(segment, '.'); dot != -1 {
		return strings.ToUpper(segment[:dot])
	}
	return strings.ToUpper(segment)
}

func containsShortName(segment string) bool {
	tilt := strings.IndexByte(segment, '~')
	if tilt < 1 || tilt > 6 || tilt+1 >= len(segment) {
		return false
	}
	next := segment[tilt+1]
	return next >= '0' && next <= '9'
}

func containsPercentEscape(path string) bool {
	for index := 0; index+2 < len(path); index++ {
		if path[index] == '%' && isHexDigit(path[index+1]) && isHexDigit(path[index+2]) {
			return true
		}
	}
	return false
}

func isHexDigit(c byte) bool {
	return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')
}

func segmentOk(segment string) bool {
	if segment == "." || segment == ".." || strings.HasSuffix(segment, ".") {
		return false
	}
	portable := segment
	if strings.HasPrefix(segment, ".") {
		portable = "x" + segment[1:]
	}
	if PathViolation(portable) != "" {
		return false
	}
	if segment == "" || len(segment) > maxSegmentBytes {
		return false
	}
	first := segment[0]
	if !(isASCIILower(first) || (first >= '0' && first <= '9') || first == '.') {
		return false
	}
	for _, c := range segment {
		if !(isASCIILower(byte(c)) || (c >= '0' && c <= '9') || c == '.' || c == '-' || c == '_') {
			return false
		}
	}
	return true
}

func segments(value string) ([]string, bool) {
	if value == "" || len(value) > maxValueBytes || strings.HasPrefix(value, "/") {
		return nil, false
	}
	parts := strings.Split(value, "/")
	for _, part := range parts {
		if part == "" {
			return nil, false
		}
	}
	return parts, true
}

// IsLogicalPath reports the closed logical-path grammar.
func IsLogicalPath(value string) bool {
	parts, ok := segments(value)
	if !ok {
		return false
	}
	for _, part := range parts {
		if !segmentOk(part) {
			return false
		}
	}
	return true
}

// IsScope reports the closed scope grammar (final segment may be `**`).
func IsScope(value string) bool {
	parts, ok := segments(value)
	if !ok {
		return false
	}
	last := parts[len(parts)-1]
	head := parts[:len(parts)-1]
	if last == "**" {
		if len(head) == 0 {
			return false
		}
		for _, part := range head {
			if !segmentOk(part) {
				return false
			}
		}
		return true
	}
	for _, part := range parts {
		if !segmentOk(part) {
			return false
		}
	}
	return true
}

// ScopeCovers reports prefix-exact or recursive coverage.
func ScopeCovers(scope, path string) bool {
	if !IsScope(scope) || !IsLogicalPath(path) {
		return false
	}
	scopeParts, _ := segments(scope)
	pathParts, _ := segments(path)
	recursive := scopeParts[len(scopeParts)-1] == "**"
	prefix := scopeParts
	if recursive {
		prefix = scopeParts[:len(scopeParts)-1]
	}
	if len(pathParts) < len(prefix) {
		return false
	}
	for index, part := range prefix {
		if pathParts[index] != part {
			return false
		}
	}
	if recursive {
		return len(pathParts) > len(prefix)
	}
	return len(pathParts) == len(prefix)
}

// ProtectedHome returns the protected home a concrete path sits in.
func ProtectedHome(path string) string {
	parts, ok := segments(path)
	if !ok {
		return ""
	}
	for _, home := range protectedHomes {
		if len(parts) >= len(home.Segments) && equalPrefix(parts, home.Segments) {
			return home.Name
		}
	}
	return ""
}

// ScopeTouchesProtectedHome returns the home a write scope would reach.
func ScopeTouchesProtectedHome(scope string) string {
	parts, ok := segments(scope)
	if !ok {
		return ""
	}
	prefix := parts
	if parts[len(parts)-1] == "**" {
		prefix = parts[:len(parts)-1]
	}
	for _, home := range protectedHomes {
		inside := len(prefix) >= len(home.Segments) && equalPrefix(prefix, home.Segments)
		above := len(prefix) <= len(home.Segments) && equalPrefix(home.Segments, prefix)
		if inside || above {
			return home.Name
		}
	}
	return ""
}

func equalPrefix(parts []string, prefix []string) bool {
	if len(parts) < len(prefix) {
		return false
	}
	for index, part := range prefix {
		if parts[index] != part {
			return false
		}
	}
	return true
}

// IsToken reports the adapter token grammar.
func IsToken(value string) bool {
	if value == "" || len(value) > 64 {
		return false
	}
	if !isASCIILower(value[0]) {
		return false
	}
	for _, c := range value {
		if !(isASCIILower(byte(c)) || (c >= '0' && c <= '9') || c == '-') {
			return false
		}
	}
	return true
}

// IsCapabilityID reports the closed capability-id grammar.
func IsCapabilityID(value string) bool {
	if value == "" || len(value) > 128 {
		return false
	}
	for _, segment := range strings.Split(value, ".") {
		if segment == "" {
			return false
		}
		first := segment[0]
		if !(isASCIILower(first) || (first >= '0' && first <= '9')) {
			return false
		}
		for _, c := range segment {
			if !(isASCIILower(byte(c)) || (c >= '0' && c <= '9') || c == '-' || c == '_') {
				return false
			}
		}
	}
	return true
}

// IsContractVersion reports the bounded `x.y.z` spelling.
func IsContractVersion(value string) bool {
	if value == "" || len(value) > 32 {
		return false
	}
	parts := strings.Split(value, ".")
	if len(parts) != 3 {
		return false
	}
	for _, part := range parts {
		if part == "" {
			return false
		}
		for _, c := range part {
			if c < '0' || c > '9' {
				return false
			}
		}
	}
	return true
}

// PlanScopeViolation returns the bounded violation token, or "".
func PlanScopeViolation(path string, writeScopes []string) string {
	if home := ProtectedHome(path); home != "" {
		return "protected-path:" + home
	}
	for _, scope := range writeScopes {
		if ScopeCovers(scope, path) {
			return ""
		}
	}
	return "scope:uncovered"
}
