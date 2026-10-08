package runtime

import (
	"testing"
)

// The Go SDK smoke suite (issue #106 review r2 #17): the strict parser,
// admission, and numeric accessor semantics stay smoke-checked beside
// the vectors runner so `go test ./...` exercises the runtime directly.

func TestStrictParseRefusesDuplicateKeys(t *testing.T) {
	if _, reason := ReadSingleObject(`{"a":1,"a":2}`, 1024); reason != ReasonDuplicateKey {
		t.Fatalf("duplicate keys refused with %q, want duplicate-key", reason)
	}
}

func TestStrictParseAcceptsSingleObject(t *testing.T) {
	if _, reason := ReadSingleObject(`{"a":1}`, 1024); reason != "" {
		t.Fatalf("single object refused with %q", reason)
	}
}

func TestNumberInRangeAcceptsBothSpellings(t *testing.T) {
	// The integer and integral-float spellings of an integer JSON number
	// are both admitted (issue #106 review r2 #4): the strict parser
	// emits int64 for `1` and float64 for `1.0`, and admission accepts
	// both within the same closed range.
	if !numberInRange(int64(1), 1, 1073741824) {
		t.Fatal("integer spelling refused")
	}
	if !numberInRange(float64(1), 1, 1073741824) {
		t.Fatal("integral-float spelling refused")
	}
	if numberInRange(0, 1, 1073741824) {
		t.Fatal("below-range value admitted")
	}
}

func TestGetNumberResolvesBothSpellings(t *testing.T) {
	document := map[string]any{"i": int64(1), "f": float64(1)}
	if n, ok := GetNumber(document, "i"); !ok || n != 1 {
		t.Fatalf("GetNumber integer = %v,%v", n, ok)
	}
	if n, ok := GetNumber(document, "f"); !ok || n != 1 {
		t.Fatalf("GetNumber integral float = %v,%v", n, ok)
	}
}

func TestIsSha256DigestGrammar(t *testing.T) {
	if !IsSha256Digest("sha256:" + repeatString("a", 64)) {
		t.Fatal("valid digest refused")
	}
	if IsSha256Digest("sha256:zz") {
		t.Fatal("malformed digest admitted")
	}
}

func repeatString(s string, n int) string {
	out := make([]byte, 0, n*len(s))
	for i := 0; i < n; i++ {
		out = append(out, s...)
	}
	return string(out)
}
