// Deterministic identifiers and digest grammar (issue #106).
package runtime

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"
)

// Sha256Hex is the lowercase hex SHA-256 of the UTF-8 bytes.
func Sha256Hex(text string) string {
	sum := sha256.Sum256([]byte(text))
	return hex.EncodeToString(sum[:])
}

// Sha256 is the `sha256:<64 hex>` digest spelling.
func Sha256(text string) string {
	return "sha256:" + Sha256Hex(text)
}

const (
	// RequestIDPrefix is the prefix of every deterministic request id.
	RequestIDPrefix = "req-"
	// PlanIDPrefix is the prefix of every deterministic plan id.
	PlanIDPrefix = "plan-"
)

func prefixedHex(value, prefix string) bool {
	rest, found := strings.CutPrefix(value, prefix)
	if !found || len(rest) != 64 {
		return false
	}
	for _, c := range rest {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			return false
		}
	}
	return true
}

// IsSha256Digest reports the canonical digest spelling.
func IsSha256Digest(value string) bool {
	return prefixedHex(value, "sha256:")
}

// IsRequestID reports the `req-` identifier grammar.
func IsRequestID(value string) bool {
	return prefixedHex(value, RequestIDPrefix)
}

// IsPlanID reports the `plan-` identifier grammar.
func IsPlanID(value string) bool {
	return prefixedHex(value, PlanIDPrefix)
}

// RequestID computes the deterministic request identifier over the
// canonical JSON of the envelope with its request_id member absent.
func RequestID(envelope Value) string {
	if object, ok := envelope.(map[string]any); ok {
		copy_ := make(map[string]any, len(object))
		for key, member := range object {
			if key != "request_id" {
				copy_[key] = member
			}
		}
		return RequestIDPrefix + Sha256Hex(Canonical(copy_))
	}
	return RequestIDPrefix + Sha256Hex(Canonical(envelope))
}

// PlanID computes the deterministic plan identifier over the canonical
// bytes of one sorted plan.
func PlanID(entries Value) string {
	return PlanIDPrefix + Sha256Hex(Canonical(entries))
}

var _ = fmt.Sprintf
