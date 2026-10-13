// Strict JSON admission for the adapter SDK (issue #106): closed
// parsing with duplicate-key, trailing-document, explicit-null, and
// non-finite refusals, plus the canonical byte serializer the wire
// requires (byte-sorted keys, compact, no trailing LF).
package runtime

import (
	"fmt"
	"math"
	"sort"
	"strings"
	"unicode/utf8"
)

// Value is a parsed JSON value: nil, bool, int64, float64, string,
// []any, or map[string]any.
type Value = any

// ParseReason is the closed safe token of a strict parse refusal.
type ParseReason string

const (
	ReasonNotJSON          ParseReason = "not-json"
	ReasonTrailingDocument ParseReason = "trailing-document"
	ReasonDuplicateKey     ParseReason = "duplicate-key"
	ReasonOversize         ParseReason = "oversize"
)

type parser struct {
	bytes []byte
	index int
	depth int
	// allowDuplicate (issue #106 review r5 #9) receives the member-name
	// path of the CONTAINING object and decides whether a duplicate key
	// there keeps the last value (map-typed member positions) instead of
	// refusing — production's serde `BTreeMap` members take last-wins.
	allowDuplicate func([]string, Value, Value) bool
}

// The maximum container nesting depth, matching the host parser's
// recursion limit (issue #106 review r4 #5): deeper documents are a
// bounded ReasonNotJSON refusal — a growable goroutine stack must never
// substitute for an admission bound.
const maxNestingDepth = 128

// hex4 decodes one strict `[0-9a-fA-F]{4}` escape window. fmt.Sscanf
// ("%04x") is not a strict hex lexer — it accepts signs and prefixes —
// so the window is validated byte by byte exactly like the host's
// parser (issue #106 review r4 #2).
func hex4(window []byte) (int, bool) {
	value := 0
	for _, c := range window {
		var digit int
		switch {
		case c >= '0' && c <= '9':
			digit = int(c - '0')
		case c >= 'a' && c <= 'f':
			digit = int(c-'a') + 10
		case c >= 'A' && c <= 'F':
			digit = int(c-'A') + 10
		default:
			return 0, false
		}
		value = value*16 + digit
	}
	return value, true
}

// StrictParse parses one complete JSON document with closed admission.
func StrictParse(text string) (Value, ParseReason) {
	return StrictParseWith(text, nil)
}

// StrictParseWith is [`StrictParse`] with a duplicate-key policy:
// map-typed member positions keep last-wins; every struct position
// still refuses.
func StrictParseWith(text string, allowDuplicate func([]string, Value, Value) bool) (Value, ParseReason) {
	p := &parser{bytes: []byte(text), allowDuplicate: allowDuplicate}
	if len(p.bytes) >= 3 && p.bytes[0] == 0xEF && p.bytes[1] == 0xBB && p.bytes[2] == 0xBF {
		return nil, ReasonNotJSON
	}
	p.skipWhitespace()
	value, reason := p.parseValue(nil)
	if reason != "" {
		return nil, reason
	}
	p.skipWhitespace()
	if p.index != len(p.bytes) {
		return nil, ReasonTrailingDocument
	}
	return value, ""
}

// ReadSingleObject reads exactly one bounded JSON object document.
func ReadSingleObject(text string, maxBytes int) (Value, ParseReason) {
	return ReadSingleObjectWith(text, maxBytes, nil)
}

// ReadSingleObjectWith is [`ReadSingleObject`] with a duplicate-key
// policy (map-typed member positions keep last-wins).
func ReadSingleObjectWith(text string, maxBytes int, allowDuplicate func([]string, Value, Value) bool) (Value, ParseReason) {
	if len(text) > maxBytes {
		return nil, ReasonOversize
	}
	value, reason := StrictParseWith(text, allowDuplicate)
	if reason != "" {
		return nil, reason
	}
	if _, ok := value.(map[string]any); !ok {
		return nil, ReasonNotJSON
	}
	return value, ""
}

func (p *parser) skipWhitespace() {
	for p.index < len(p.bytes) {
		switch p.bytes[p.index] {
		case ' ', '\t', '\r', '\n':
			p.index++
		default:
			return
		}
	}
}

func (p *parser) peek() (byte, bool) {
	if p.index < len(p.bytes) {
		return p.bytes[p.index], true
	}
	return 0, false
}

func (p *parser) parseValue(path []string) (Value, ParseReason) {
	c, ok := p.peek()
	if !ok {
		return nil, ReasonNotJSON
	}
	switch {
	case c == '{':
		return p.parseObject(path)
	case c == '[':
		return p.parseArray(path)
	case c == '"':
		text, reason := p.parseString()
		if reason != "" {
			return nil, reason
		}
		return text, ""
	case c == 't':
		return p.parseLiteral("true", true)
	case c == 'f':
		return p.parseLiteral("false", false)
	case c == 'n':
		return p.parseLiteral("null", nil)
	case c == '-' || (c >= '0' && c <= '9'):
		return p.parseNumber()
	default:
		return nil, ReasonNotJSON
	}
}

func (p *parser) parseLiteral(literal string, value Value) (Value, ParseReason) {
	if strings.HasPrefix(string(p.bytes[p.index:]), literal) {
		p.index += len(literal)
		return value, ""
	}
	return nil, ReasonNotJSON
}

func (p *parser) parseObject(path []string) (Value, ParseReason) {
	p.index++
	p.depth++
	if p.depth > maxNestingDepth {
		return nil, ReasonNotJSON
	}
	result, reason := p.parseObjectMembers(path)
	p.depth--
	return result, reason
}

func (p *parser) parseObjectMembers(path []string) (Value, ParseReason) {
	members := map[string]any{}
	p.skipWhitespace()
	if c, _ := p.peek(); c == '}' {
		p.index++
		return members, ""
	}
	for {
		p.skipWhitespace()
		if c, _ := p.peek(); c != '"' {
			return nil, ReasonNotJSON
		}
		key, reason := p.parseString()
		if reason != "" {
			return nil, reason
		}
		p.skipWhitespace()
		if c, _ := p.peek(); c != ':' {
			return nil, ReasonNotJSON
		}
		p.index++
		p.skipWhitespace()
		memberPath := append(append([]string{}, path...), key)
		value, reason := p.parseValue(memberPath)
		if reason != "" {
			return nil, reason
		}
		if _, duplicate := members[key]; duplicate {
			// Map-typed member positions take last-wins exactly like
			// production's serde `BTreeMap` (issue #106 review r5 #9);
			// every struct position keeps the hard refusal.
			if p.allowDuplicate == nil || !p.allowDuplicate(path, members[key], value) {
				return nil, ReasonDuplicateKey
			}
		}
		members[key] = value
		p.skipWhitespace()
		next, _ := p.peek()
		switch next {
		case ',':
			p.index++
		case '}':
			p.index++
			return members, ""
		default:
			return nil, ReasonNotJSON
		}
	}
}

func (p *parser) parseArray(path []string) (Value, ParseReason) {
	p.index++
	p.depth++
	if p.depth > maxNestingDepth {
		return nil, ReasonNotJSON
	}
	result, reason := p.parseArrayItems(path)
	p.depth--
	return result, reason
}

func (p *parser) parseArrayItems(path []string) (Value, ParseReason) {
	items := []any{}
	p.skipWhitespace()
	if c, _ := p.peek(); c == ']' {
		p.index++
		return items, ""
	}
	for {
		p.skipWhitespace()
		value, reason := p.parseValue(path)
		if reason != "" {
			return nil, reason
		}
		items = append(items, value)
		p.skipWhitespace()
		next, _ := p.peek()
		switch next {
		case ',':
			p.index++
		case ']':
			p.index++
			return items, ""
		default:
			return nil, ReasonNotJSON
		}
	}
}

func (p *parser) parseString() (string, ParseReason) {
	p.index++
	var out strings.Builder
	for {
		if p.index >= len(p.bytes) {
			return "", ReasonNotJSON
		}
		c := p.bytes[p.index]
		p.index++
		switch {
		case c == '"':
			return out.String(), ""
		case c == '\\':
			if p.index >= len(p.bytes) {
				return "", ReasonNotJSON
			}
			escape := p.bytes[p.index]
			p.index++
			switch escape {
			case '"':
				out.WriteByte('"')
			case '\\':
				out.WriteByte('\\')
			case '/':
				out.WriteByte('/')
			case 'b':
				out.WriteByte(0x08)
			case 'f':
				out.WriteByte(0x0C)
			case 'n':
				out.WriteByte('\n')
			case 'r':
				out.WriteByte('\r')
			case 't':
				out.WriteByte('\t')
			case 'u':
				if p.index+4 > len(p.bytes) {
					return "", ReasonNotJSON
				}
				high, ok := hex4(p.bytes[p.index : p.index+4])
				if !ok {
					return "", ReasonNotJSON
				}
				p.index += 4
				if high >= 0xD800 && high < 0xDC00 {
					// A high surrogate must be followed by one REAL low
					// surrogate escape (issue #106 review r4 #2): a missing
					// pair or a low half outside DC00..DFFF is exactly what
					// the host's parser refuses — never fabricated
					// arithmetic on an out-of-range value.
					if p.index+6 > len(p.bytes) || p.bytes[p.index] != '\\' || p.bytes[p.index+1] != 'u' {
						return "", ReasonNotJSON
					}
					low, ok := hex4(p.bytes[p.index+2 : p.index+6])
					if !ok {
						return "", ReasonNotJSON
					}
					if low < 0xDC00 || low > 0xDFFF {
						return "", ReasonNotJSON
					}
					p.index += 6
					combined := rune(0x10000 + (rune(high)-0xD800)<<10 + (rune(low) - 0xDC00))
					out.WriteRune(combined)
				} else if high >= 0xDC00 && high <= 0xDFFF {
					// A lone low surrogate escape (issue #106 review r4 #2c):
					// the host refuses it; WriteRune would silently repair
					// it to U+FFFD and admit the document.
					return "", ReasonNotJSON
				} else {
					out.WriteRune(rune(high))
				}
			default:
				return "", ReasonNotJSON
			}
		case c <= 0x1F:
			return "", ReasonNotJSON
		default:
			width := utf8Width(c)
			if width == 0 || p.index-1+width > len(p.bytes) {
				return "", ReasonNotJSON
			}
			// Full UTF-8 sequence validation (issue #106 review r4 #2c):
			// overlong encodings, UTF-8-encoded surrogates, and truncated
			// continuations are refused exactly like the host's parser —
			// but a DECODED literal U+FFFD is a legal character the host
			// accepts, so the old ContainsRune(0xFFFD) refusal was a
			// reverse parity hole and is gone.
			if !utf8.Valid(p.bytes[p.index-1 : p.index-1+width]) {
				return "", ReasonNotJSON
			}
			out.Write(p.bytes[p.index-1 : p.index-1+width])
			p.index += width - 1
		}
	}
}

func utf8Width(byte byte) int {
	switch {
	case byte < 0x80:
		return 1
	case byte >= 0xC2 && byte < 0xE0:
		return 2
	case byte >= 0xE0 && byte < 0xF0:
		return 3
	case byte >= 0xF0 && byte < 0xF5:
		return 4
	default:
		return 0
	}
}

// The complete JSON number grammar (issue #106 review r3 #4): an
// optional minus, then `0` or [1-9][0-9]* (leading zeros refused), then
// an optional fraction ("." plus at least one digit) and an optional
// exponent (e/E, optional sign, at least one digit). Malformed
// spellings like "010000" or "1." are ReasonNotJSON, exactly as the
// host's serde parser refuses them.
func (p *parser) parseNumber() (Value, ParseReason) {
	start := p.index
	if c, _ := p.peek(); c == '-' {
		p.index++
	}
	c, ok := p.peek()
	if !ok || c < '0' || c > '9' {
		return nil, ReasonNotJSON
	}
	if c == '0' {
		p.index++
	} else {
		for {
			if c, ok := p.peek(); ok && c >= '0' && c <= '9' {
				p.index++
				continue
			}
			break
		}
	}
	isFloat := false
	if c, _ := p.peek(); c == '.' {
		isFloat = true
		p.index++
		c, ok := p.peek()
		if !ok || c < '0' || c > '9' {
			return nil, ReasonNotJSON
		}
		for {
			if c, ok := p.peek(); ok && c >= '0' && c <= '9' {
				p.index++
				continue
			}
			break
		}
	}
	if c, _ := p.peek(); c == 'e' || c == 'E' {
		isFloat = true
		p.index++
		if c, _ := p.peek(); c == '+' || c == '-' {
			p.index++
		}
		c, ok := p.peek()
		if !ok || c < '0' || c > '9' {
			return nil, ReasonNotJSON
		}
		for {
			if c, ok := p.peek(); ok && c >= '0' && c <= '9' {
				p.index++
				continue
			}
			break
		}
	}
	text := string(p.bytes[start:p.index])
	if isFloat {
		var value float64
		if _, err := fmt.Sscanf(text, "%g", &value); err != nil {
			return nil, ReasonNotJSON
		}
		return value, ""
	}
	if text == "-0" {
		// The negative-zero integer lexeme (issue #106 review r4 #3):
		// production types every wire number serde `u64`, whose visitor
		// refuses the signed spelling outright. The lexeme is kept out of
		// the integer value space so every u64 member refuses exactly
		// like the host (shape at `limits`, bounds elsewhere) instead of
		// collapsing to an accepted zero.
		return math.Copysign(0, -1), ""
	}
	var value int64
	if _, err := fmt.Sscanf(text, "%d", &value); err != nil {
		var fallback float64
		if _, err := fmt.Sscanf(text, "%g", &fallback); err != nil {
			return nil, ReasonNotJSON
		}
		return fallback, ""
	}
	return value, ""
}

// Canonical renders the byte-sorted compact JSON form (no trailing LF).
func Canonical(value Value) string {
	var out strings.Builder
	writeCanonical(&out, value)
	return out.String()
}

func writeCanonical(out *strings.Builder, value Value) {
	switch typed := value.(type) {
	case nil:
		out.WriteString("null")
	case bool:
		if typed {
			out.WriteString("true")
		} else {
			out.WriteString("false")
		}
	case int:
		out.WriteString(fmt.Sprintf("%d", typed))
	case int64:
		out.WriteString(fmt.Sprintf("%d", typed))
	case float64:
		if typed == float64(int64(typed)) {
			out.WriteString(fmt.Sprintf("%d", int64(typed)))
		} else {
			out.WriteString(fmt.Sprintf("%g", typed))
		}
	case string:
		out.WriteString(Quote(typed))
	case []any:
		out.WriteByte('[')
		for index, item := range typed {
			if index > 0 {
				out.WriteByte(',')
			}
			writeCanonical(out, item)
		}
		out.WriteByte(']')
	case map[string]any:
		keys := make([]string, 0, len(typed))
		for key := range typed {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		out.WriteByte('{')
		for index, key := range keys {
			if index > 0 {
				out.WriteByte(',')
			}
			out.WriteString(Quote(key))
			out.WriteByte(':')
			writeCanonical(out, typed[key])
		}
		out.WriteByte('}')
	default:
		out.WriteString("null")
	}
}

// Quote renders one JSON string literal with the closed escape set.
func Quote(text string) string {
	var out strings.Builder
	out.WriteByte('"')
	for _, c := range text {
		switch c {
		case '"':
			out.WriteString("\\\"")
		case '\\':
			out.WriteString("\\\\")
		case '\n':
			out.WriteString("\\n")
		case '\r':
			out.WriteString("\\r")
		case '\t':
			out.WriteString("\\t")
		case 0x08:
			out.WriteString("\\b")
		case 0x0C:
			out.WriteString("\\f")
		default:
			if c < 0x20 {
				out.WriteString(fmt.Sprintf("\\u%04x", c))
			} else {
				out.WriteRune(c)
			}
		}
	}
	out.WriteByte('"')
	return out.String()
}

// Members returns the object member keys (nil when not an object).
func Members(value Value) map[string]Value {
	if object, ok := value.(map[string]any); ok {
		result := make(map[string]Value, len(object))
		for key, member := range object {
			result[key] = member
		}
		return result
	}
	return nil
}

// GetString returns the string member or "".
func GetString(object Value, key string) string {
	if object, ok := object.(map[string]any); ok {
		if text, ok := object[key].(string); ok {
			return text
		}
	}
	return ""
}

// GetArray returns the array member or nil.
func GetArray(object Value, key string) []any {
	if object, ok := object.(map[string]any); ok {
		if items, ok := object[key].([]any); ok {
			return items
		}
	}
	return nil
}

// Has reports whether the member exists (including explicit null).
func Has(object Value, key string) bool {
	object_, ok := object.(map[string]any)
	if !ok {
		return false
	}
	_, exists := object_[key]
	return exists
}

// IsNull reports an explicit null member.
func IsNull(object Value, key string) bool {
	object_, ok := object.(map[string]any)
	if !ok {
		return false
	}
	value, exists := object_[key]
	return exists && value == nil
}

// GetNumber returns the non-negative integer member value. Only an
// integer JSON lexeme qualifies (issue #106 review r3 #3): production
// decodes every wire number as serde `u64`, which refuses float
// spellings outright — an integral float (1.0, 1e2) is admission
// `shape`, never an accepted integer.
func GetNumber(object Value, key string) (int64, bool) {
	if object, ok := object.(map[string]any); ok {
		if typed, ok := object[key].(int64); ok && typed >= 0 {
			return typed, true
		}
	}
	return 0, false
}

// GetBool returns the boolean member.
func GetBool(object Value, key string) (bool, bool) {
	if object, ok := object.(map[string]any); ok {
		if value, ok := object[key].(bool); ok {
			return value, true
		}
	}
	return false, false
}
