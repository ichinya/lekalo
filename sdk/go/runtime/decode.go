// Strict wire decoding and semantic validation (issue #106): the
// hand-maintained codec obligation over the generated vocabulary.
package runtime

import (
	"fmt"
	"strings"

	"github.com/ichinya/lekalo-adapter-sdk-go/generated"
)

// Refusal is one closed safe refusal: stage plus reason token.
type Refusal struct {
	Stage  string
	Reason string
}

func (r Refusal) Error() string { return r.Stage + "/" + r.Reason }

const (
	stageDecode   = "decode"
	stageValidate = "validate"
)

const lintVersion = "0.6.4"

func refuse(stage, reason string) *Refusal {
	return &Refusal{Stage: stage, Reason: reason}
}

func isClosedObject(value Value, allowed []string) bool {
	object, ok := value.(map[string]any)
	if !ok {
		return false
	}
	for key := range object {
		if !containsString(allowed, key) {
			return false
		}
	}
	return true
}

func boundedText(value Value, max int) bool {
	text, ok := value.(string)
	if !ok {
		return false
	}
	return len([]rune(text)) <= max
}

// The typed integer range test (issue #106 review r3 #3): only an
// integer JSON lexeme qualifies. Production decodes every wire number
// as serde `u64`, which refuses float spellings outright — an integral
// float (1.0, 1e2) is admission `shape`, never an accepted integer.
func numberInRange(value Value, min, max int64) bool {
	typed, ok := value.(int64)
	return ok && typed >= min && typed <= max
}

// AdmitRequest decodes and fully validates one request document.
func AdmitRequest(text string) (Value, *Refusal) {
	value, reason := ReadSingleObject(text, generated.MAX_REQUEST_BYTES)
	if reason != "" {
		return nil, refuse(stageDecode, string(reason))
	}
	value, refusal := AdmitRequestValue(value)
	if refusal != nil {
		return nil, refusal
	}
	if refusal := ValidateRequest(value); refusal != nil {
		return nil, refusal
	}
	return value, nil
}

// AdmitRequestValue runs only the structural admission pass.
func AdmitRequestValue(value Value) (Value, *Refusal) {
	allowed := []string{"dry_run", "ir_path", "limits", "lint_request", "native_request", "operation", "plan_id", "profile", "profile_capabilities", "profile_digest", "project_root", "protocol", "protocol_version", "request_id", "target"}
	// The declared field order (wire.rs RequestEnvelope) drives the
	// positional-seq mapping (issue #106 review r6 #1).
	fields := []string{"protocol", "protocol_version", "operation", "request_id", "project_root", "ir_path", "target", "profile", "profile_digest", "profile_capabilities", "dry_run", "limits", "plan_id", "native_request", "lint_request"}
	if !structValue(value, fields) {
		return nil, refuse(stageDecode, unknownOrShape(value, allowed))
	}
	// A null ELEMENT inside a positional envelope seq is serde's typed
	// invalid-type shape, never the map spelling's explicit-null token
	// (issue #106 review r8 #9).
	nullReason := "explicit-null"
	if _, isSeq := value.([]any); isSeq {
		nullReason = "shape"
	}
	value, _ = seqAsMembers(value, fields)
	for _, member := range []string{"protocol", "protocol_version", "operation", "request_id", "project_root"} {
		if !Has(value, member) {
			return nil, refuse(stageDecode, "shape")
		}
		if IsNull(value, member) {
			return nil, refuse(stageDecode, nullReason)
		}
	}
	if GetString(value, "protocol") != generated.WireToken {
		return nil, refuse(stageDecode, "token")
	}
	version := GetString(value, "protocol_version")
	if !containsString(generated.SupportedProtocolVersions, version) {
		return nil, refuse(stageDecode, "version")
	}
	if !containsString(generated.LatestOperations, GetString(value, "operation")) {
		return nil, refuse(stageDecode, "operation")
	}
	if !IsRequestID(GetString(value, "request_id")) {
		return nil, refuse(stageDecode, "request-id")
	}
	if GetString(value, "project_root") != "." {
		return nil, refuse(stageDecode, "grammar")
	}
	for _, member := range []string{"ir_path", "target", "profile"} {
		if IsNull(value, member) {
			return nil, refuse(stageDecode, nullReason)
		}
		if Has(value, member) {
			if member == "ir_path" {
				if !IsLogicalPath(GetString(value, member)) {
					return nil, refuse(stageDecode, "grammar")
				}
			} else if !IsToken(GetString(value, member)) {
				return nil, refuse(stageDecode, "grammar")
			}
		}
	}
	if IsNull(value, "profile_digest") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "profile_digest") && !IsSha256Digest(GetString(value, "profile_digest")) {
		return nil, refuse(stageDecode, "bounds")
	}
	if IsNull(value, "dry_run") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "dry_run") {
		if _, ok := GetBool(value, "dry_run"); !ok {
			return nil, refuse(stageDecode, "shape")
		}
	}
	if IsNull(value, "plan_id") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "plan_id") && !IsPlanID(GetString(value, "plan_id")) {
		return nil, refuse(stageDecode, "bounds")
	}
	if IsNull(value, "limits") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "limits") {
		if refusal := admitLimits(value.(map[string]any)["limits"]); refusal != nil {
			return nil, refusal
		}
	}
	if IsNull(value, "profile_capabilities") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "profile_capabilities") {
		if refusal := admitProfileCapabilities(value.(map[string]any)["profile_capabilities"]); refusal != nil {
			return nil, refusal
		}
	}
	if IsNull(value, "native_request") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "native_request") {
		nativeMapped, refusal := admitNativeRequest(valueOf(value, "native_request"))
		if refusal != nil {
			return nil, refusal
		}
		value.(map[string]any)["native_request"] = nativeMapped
	}
	if IsNull(value, "lint_request") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "lint_request") {
		lintMapped, refusal := admitLintRequest(valueOf(value, "lint_request"))
		if refusal != nil {
			return nil, refusal
		}
		value.(map[string]any)["lint_request"] = lintMapped
	}
	return value, nil
}

func unknownOrShape(value Value, allowed []string) string {
	object, ok := value.(map[string]any)
	if !ok {
		return "shape"
	}
	for key := range object {
		if !containsString(allowed, key) {
			return "unknown-member"
		}
	}
	return "shape"
}

func admitLimits(rawValue Value) *Refusal {
	if !structValue(rawValue, []string{"timeout_ms", "max_output_bytes"}) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"timeout_ms", "max_output_bytes"})
	if !isClosedObject(value, []string{"max_output_bytes", "timeout_ms"}) {
		return refuse(stageDecode, "shape")
	}
	// Production requires BOTH members whenever `limits` is present
	// (wire.rs: absent or out-of-range is admission `shape`); the SDK
	// used to admit partial limits objects. Member order mirrors
	// production's validate: timeout_ms first, so a zero timeout still
	// refuses `bounds` before the missing-member `shape` is reached.
	for _, bound := range []struct {
		member   string
		min, max int64
	}{
		{"timeout_ms", generated.LIMITS_TIMEOUT_MIN, generated.LIMITS_TIMEOUT_MAX},
		{"max_output_bytes", generated.LIMITS_OUTPUT_MIN, generated.LIMITS_OUTPUT_MAX},
	} {
		if IsNull(value, bound.member) {
			return refuse(stageDecode, "explicit-null")
		}
		if !Has(value, bound.member) {
			return refuse(stageDecode, "shape")
		}
		if _, isFloat := value.(map[string]any)[bound.member].(float64); isFloat {
			// A float lexeme where production types `u64` is `shape`.
			return refuse(stageDecode, "shape")
		}
		if !numberInRange(value.(map[string]any)[bound.member], bound.min, bound.max) {
			return refuse(stageDecode, "bounds")
		}
	}
	return nil
}

func admitProfileCapabilities(value Value) *Refusal {
	items, ok := value.([]any)
	// Production requires a present, non-empty, strictly-sorted list
	// (wire.rs profile-capabilities: `!capabilities.is_empty()` is part
	// of the sorted-unique verdict) — an empty list is admission
	// `bounds`, never an empty default (issue #106 review r5 #1).
	if !ok || len(items) == 0 || len(items) > generated.MAX_DECLARED_CAPABILITIES {
		return refuse(stageDecode, "bounds")
	}
	previous := ""
	for index, capability := range items {
		if !structValue(capability, []string{"id", "support"}) {
			return refuse(stageDecode, "shape")
		}
		capability, _ = seqAsMembers(capability, []string{"id", "support"})
		items[index] = capability
		if !isClosedObject(capability, []string{"id", "support"}) {
			return refuse(stageDecode, "shape")
		}
		id := GetString(capability, "id")
		if !IsCapabilityID(id) {
			return refuse(stageDecode, "bounds")
		}
		if !containsString(GeneratedEnums("supportState"), GetString(capability, "support")) {
			return refuse(stageDecode, "shape")
		}
		if previous != "" && id <= previous {
			return refuse(stageDecode, "bounds")
		}
		previous = id
	}
	return nil
}

func admitContentRef(rawValue Value, path string) *Refusal {
	if !structValue(rawValue, []string{"digest", "revision", "adapter"}) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"digest", "revision", "adapter"})
	if !isClosedObject(value, []string{"adapter", "digest", "revision"}) {
		return refuse(stageDecode, "shape")
	}
	if !IsSha256Digest(GetString(value, "digest")) {
		return refuse(stageDecode, "bounds")
	}
	// Revision is an unbounded wire string; the optional adapter identity carries the production adapter token
	// grammar (issue #106 review r4 #2, core wire.rs validate:
	// `!scopes::is_token(adapter)` refuses `native-request`): a present,
	// non-token identity must never reach dispatch.
	if IsNull(value, "adapter") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "adapter") && !IsToken(GetString(value, "adapter")) {
		return refuse(stageDecode, "bounds")
	}
	if IsNull(value, "revision") {
		return refuse(stageDecode, "explicit-null")
	}
	// Production types `revision` `Option<String>` with no bound beyond
	// the member type (issue #106 review r5 #9): only the type is
	// checked, never a length the wire does not declare.
	if Has(value, "revision") {
		if _, ok := value.(map[string]any)["revision"].(string); !ok {
			return refuse(stageDecode, "shape")
		}
	}
	return nil
}

func admitNativeRequest(value Value) (Value, *Refusal) {
	allowed := []string{"capability_snapshot_digest", "changes", "execution_policy_ref", "input_manifest_digest", "observed_ref", "scan_ref", "tool_catalog_digest"}
	fields := []string{"changes", "scan_ref", "observed_ref", "execution_policy_ref", "input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"}
	if !structValue(value, fields) {
		return nil, refuse(stageDecode, "shape")
	}
	value, _ = seqAsMembers(value, fields)
	if !isClosedObject(value, allowed) {
		return nil, refuse(stageDecode, "shape")
	}
	for _, member := range []string{"input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"} {
		if IsNull(value, member) {
			return nil, refuse(stageDecode, "explicit-null")
		}
		if !IsSha256Digest(GetString(value, member)) {
			return nil, refuse(stageDecode, "bounds")
		}
	}
	changes := value.(map[string]any)["changes"]
	// All-default struct: production deserializes `changes` from a
	// positional seq too (`[]` decodes with both lists defaulted),
	// exactly like serde struct-from-seq (issue #106 review r5 #9).
	changes, refusal := seqAsMembers(changes, []string{"files", "symbols"})
	if refusal != nil {
		return nil, refusal
	}
	if !isClosedObject(changes, []string{"files", "symbols"}) {
		return nil, refuse(stageDecode, "shape")
	}
	files, refusal := optionalArray(changes, "files")
	if refusal != nil {
		return nil, refusal
	}
	symbols, refusal := optionalArray(changes, "symbols")
	if refusal != nil {
		return nil, refusal
	}
	if len(files) > 1024 || len(symbols) > 1024 {
		return nil, refuse(stageDecode, "bounds")
	}
	// Production validates the changed files without any ordering rule
	// (wire.rs native-request: member checks only) — the SDK's old
	// ascending-path requirement refused legal documents (review r5 #9).
	for index, change := range files {
		if !structValue(change, []string{"path", "change", "before_digest", "after_digest"}) {
			return nil, refuse(stageDecode, "shape")
		}
		mapped, _ := seqAsMembers(change, []string{"path", "change", "before_digest", "after_digest"})
		files[index] = mapped
		change = mapped
		if !isClosedObject(change, []string{"after_digest", "before_digest", "change", "path"}) {
			return nil, refuse(stageDecode, "shape")
		}
		if !IsLogicalPath(GetString(change, "path")) {
			return nil, refuse(stageDecode, "bounds")
		}
		if !containsString(GeneratedEnums("nativeChangeKind"), GetString(change, "change")) {
			return nil, refuse(stageDecode, "shape")
		}
		for _, member := range []string{"after_digest", "before_digest"} {
			if IsNull(change, member) {
				return nil, refuse(stageDecode, "explicit-null")
			}
			if Has(change, member) && !IsSha256Digest(GetString(change, member)) {
				return nil, refuse(stageDecode, "bounds")
			}
		}
	}
	// Symbols carry no per-item bound on the wire (only the count is
	// bounded) — empty and long strings are both legal (review r5 #9).
	for _, symbol := range symbols {
		if _, ok := symbol.(string); !ok {
			return nil, refuse(stageDecode, "shape")
		}
	}
	if refusal := admitContentRef(valueOf(value, "scan_ref"), "native_request.scan_ref"); refusal != nil {
		return nil, refusal
	}
	if refusal := admitContentRef(valueOf(value, "execution_policy_ref"), "native_request.execution_policy_ref"); refusal != nil {
		return nil, refusal
	}
	if IsNull(value, "observed_ref") {
		return nil, refuse(stageDecode, "explicit-null")
	}
	if Has(value, "observed_ref") {
		if refusal := admitContentRef(valueOf(value, "observed_ref"), "native_request.observed_ref"); refusal != nil {
			return nil, refusal
		}
	}
	return value, nil
}

func changesOf(value Value, key string) Value {
	return value.(map[string]any)[key]
}

// optionalArray admits one optional wire array (issue #106 review r4
// #3): absent means the empty default; a present member must be a JSON
// array — an object, string, number, or explicit null refuses instead
// of collapsing to the empty default like GetArray did.
// All-default struct members accepted from a positional JSON sequence
// (serde `visit_seq` parity, issue #106 review r5 #9): production
// deserializes every `#[serde(default)]` struct from a seq positionally,
// so `[]` (and any shorter prefix) decodes with the defaults filling
// the tail; a longer seq refuses `shape` exactly like serde's
// invalid-length error.
func seqAsMembers(value Value, fields []string) (Value, *Refusal) {
	items, ok := value.([]any)
	if !ok {
		return value, nil
	}
	if len(items) > len(fields) {
		return nil, refuse(stageDecode, "shape")
	}
	members := map[string]any{}
	for index, item := range items {
		members[fields[index]] = item
	}
	return members, nil
}

// structValue reports the struct-position admission shape: a member map
// (closed) or a positional sequence prefix of the declared fields —
// never a scalar (issue #106 review r6 #1).
func structValue(value Value, fields []string) bool {
	switch typed := value.(type) {
	case []any:
		return len(typed) <= len(fields)
	case map[string]any:
		for key := range typed {
			if !containsString(fields, key) {
				return false
			}
		}
		return true
	}
	return false
}

func optionalArray(object Value, member string) ([]any, *Refusal) {
	if !Has(object, member) {
		return nil, nil
	}
	if IsNull(object, member) {
		return nil, refuse(stageDecode, "explicit-null")
	}
	items, ok := object.(map[string]any)[member].([]any)
	if !ok {
		return nil, refuse(stageDecode, "bounds")
	}
	return items, nil
}

func admitLintRequest(value Value) (Value, *Refusal) {
	allowed := []string{"bindings", "files", "pins", "scope"}
	fields := []string{"scope", "pins", "bindings", "files"}
	if !structValue(value, fields) {
		return nil, refuse(stageDecode, "shape")
	}
	value, _ = seqAsMembers(value, fields)
	if !isClosedObject(value, allowed) {
		return nil, refuse(stageDecode, "shape")
	}
	// Every lint member is required on the wire (production's LintRequest
	// has no serde defaults): a missing member is `shape` exactly like
	// serde's missing-field error — only an explicit null gets its own
	// token (issue #106 review r5 #8).
	for _, member := range []string{"scope", "files", "bindings", "pins"} {
		if !Has(value, member) {
			return nil, refuse(stageDecode, "shape")
		}
		// pins is a REQUIRED struct (not an Option): an explicit null is
		// serde's wrong-type `shape`, never the Option null token
		// (issue #106 review r6 #7).
		if member != "pins" && IsNull(value, member) {
			return nil, refuse(stageDecode, "explicit-null")
		}
	}
	scope, refusal := optionalArray(value, "scope")
	if refusal != nil {
		return nil, refusal
	}
	files, refusal := optionalArray(value, "files")
	if refusal != nil {
		return nil, refusal
	}
	bindings, refusal := optionalArray(value, "bindings")
	if refusal != nil {
		return nil, refusal
	}
	if len(scope) == 0 || len(scope) > 10000 || len(files) > 4096 || len(bindings) > 10000 {
		return nil, refuse(stageDecode, "bounds")
	}
	for _, scopeEntry := range scope {
		if !boundedText(scopeEntry, int(generated.ERROR_DETAIL_MAX)) {
			return nil, refuse(stageDecode, "bounds")
		}
	}
	for _, file := range files {
		// Comma-ok at every element access (issue #106 review r5 #3): a
		// wrong-typed list element is a refusal, never a panic — the same
		// `bounds` token the grammar check would emit for a string (the
		// cross-language codec vocabulary must agree byte-for-byte).
		text, ok := file.(string)
		if !ok {
			return nil, refuse(stageDecode, "bounds")
		}
		if !IsLogicalPath(text) {
			return nil, refuse(stageDecode, "bounds")
		}
	}
	for index, binding := range bindings {
		if !structValue(binding, []string{"path", "nativeId", "symbol", "kind", "fingerprint"}) {
			return nil, refuse(stageDecode, "shape")
		}
		mapped, _ := seqAsMembers(binding, []string{"path", "nativeId", "symbol", "kind", "fingerprint"})
		bindings[index] = mapped
		binding = mapped
		if !isClosedObject(binding, []string{"fingerprint", "kind", "nativeId", "path", "symbol"}) {
			return nil, refuse(stageDecode, "shape")
		}
		if !IsLogicalPath(GetString(binding, "path")) {
			return nil, refuse(stageDecode, "bounds")
		}
		for _, member := range []string{"nativeId", "symbol"} {
			if !boundedText(binding.(map[string]any)[member], 128) {
				return nil, refuse(stageDecode, "bounds")
			}
		}
		if !containsString([]string{"command", "entity"}, GetString(binding, "kind")) {
			return nil, refuse(stageDecode, "shape")
		}
		if !IsSha256Digest(GetString(binding, "fingerprint")) {
			return nil, refuse(stageDecode, "bounds")
		}
	}
	pins := valueOf(value, "pins")
	if !structValue(pins, []string{"capabilities", "ir", "model", "observed", "profile", "revision"}) {
		return nil, refuse(stageDecode, "shape")
	}
	pins, _ = seqAsMembers(pins, []string{"model", "ir", "observed", "revision", "profile", "capabilities"})
	value.(map[string]any)["pins"] = pins
	if !isClosedObject(pins, []string{"capabilities", "ir", "model", "observed", "profile", "revision"}) {
		return nil, refuse(stageDecode, "shape")
	}
	if IsNull(pins, "capabilities") || IsNull(pins, "ir") || IsNull(pins, "model") ||
		IsNull(pins, "observed") || IsNull(pins, "profile") || IsNull(pins, "revision") {
		return nil, refuse(stageDecode, "shape")
	}
	// The pinned 0.6.4 schema carries every pin as a State-tagged value
	// ({state:"known",value}|{state:"unknown"|"withheld"|"unsupported"}).
	for _, member := range []string{"capabilities", "ir", "model", "observed", "profile", "revision"} {
		if !Has(pins, member) {
			return nil, refuse(stageDecode, "shape")
		}
		// The state admission NORMALIZES the serde `visit_seq` pair onto
		// the member map in place (issue #106 review r8 #3), so the
		// semantic validate pass below always reads {state,value}.
		normalized, refusal := admitStateString(valueOf(pins, member))
		if refusal != nil {
			return nil, refusal
		}
		pins.(map[string]any)[member] = normalized
	}
	return value, nil
}

// admitStateString checks one State-tagged string value of the pinned
// 0.6.4 lint schema and returns the NORMALIZED map form.
func admitStateString(value Value) (Value, *Refusal) {
	if seq, ok := value.([]any); ok {
		// An EMPTY positional seq never reaches seq[0] (issue #106
		// review r8 #6): index-out-of-range is a process panic, not a
		// bounded refusal. A 1-element or 3+-element seq is serde's
		// invalid length; a non-null payload at a unit variant is its
		// invalid type — both `shape` (r8 #2).
		if len(seq) == 0 || len(seq) > 2 {
			return nil, refuse(stageDecode, "shape")
		}
		tag, _ := seq[0].(string)
		if tag == "known" {
			if len(seq) != 2 {
				return nil, refuse(stageDecode, "shape")
			}
			if text, isText := seq[1].(string); isText {
				// An empty payload is the typed-decode `shape` class; only
				// the over-length grammar is `bounds` (r8 #9).
				if text == "" {
					return nil, refuse(stageDecode, "shape")
				}
				if !boundedText(seq[1], 128) {
					return nil, refuse(stageDecode, "bounds")
				}
				return map[string]any{"state": "known", "value": text}, nil
			}
			// A non-string payload at `known` is serde's invalid-type
			// `shape`, never `bounds` (issue #106 review r8 #2/#9).
			return nil, refuse(stageDecode, "shape")
		}
		if containsString([]string{"unknown", "withheld", "unsupported"}, tag) {
			// The unit variants carry only the serde `None` payload: a
			// 1-element seq or a non-null payload is `shape` (r8 #2).
			if len(seq) != 2 || seq[1] != nil {
				return nil, refuse(stageDecode, "shape")
			}
			return map[string]any{"state": tag}, nil
		}
		return nil, refuse(stageDecode, "shape")
	}
	if value == nil || !isClosedObject(value, []string{"state", "value"}) {
		return nil, refuse(stageDecode, "shape")
	}
	switch state := GetString(value, "state"); state {
	case "known":
		keys := value.(map[string]any)
		if len(keys) != 2 {
			return nil, refuse(stageDecode, "shape")
		}
		if text, isText := keys["value"].(string); isText {
			if text == "" {
				return nil, refuse(stageDecode, "shape")
			}
			if !boundedText(keys["value"], 128) {
				return nil, refuse(stageDecode, "bounds")
			}
			return value, nil
		}
		return nil, refuse(stageDecode, "shape")
	case "unknown", "withheld", "unsupported":
		if len(value.(map[string]any)) != 1 {
			return nil, refuse(stageDecode, "shape")
		}
		return value, nil
	default:
		return nil, refuse(stageDecode, "shape")
	}
}

// AdmitResponse decodes one response document (structural admission).
// AdmitResponse decodes one response document (structural admission
// only). Duplicate keys are refused everywhere except map-typed member
// positions: production's `capabilities.capabilities` is a serde
// `BTreeMap` whose duplicates take last-wins (issue #106 review r5 #9).
func AdmitResponse(text string) (Value, *Refusal) {
	value, reason := ReadSingleObjectWith(text, generated.DEFAULT_MAX_OUTPUT_BYTES, func(path []string, previous, next Value) bool {
		before, okBefore := previous.(string)
		after, okAfter := next.(string)
		return len(path) == 2 && path[0] == "capabilities" && path[1] == "capabilities" && okBefore && okAfter && containsString(GeneratedEnums("supportState"), before) && containsString(GeneratedEnums("supportState"), after)
	})
	if reason != "" {
		return nil, refuse(stageDecode, string(reason))
	}
	value, refusal := AdmitResponseValue(value)
	if refusal != nil {
		return nil, refusal
	}
	return value, nil
}

func admitIdentity(rawValue Value, path string) *Refusal {
	if !structValue(rawValue, []string{"id", "version", "digest"}) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"id", "version", "digest"})
	if !isClosedObject(value, []string{"digest", "id", "version"}) {
		return refuse(stageDecode, "shape")
	}
	if !IsToken(GetString(value, "id")) || !IsContractVersion(GetString(value, "version")) || !IsSha256Digest(GetString(value, "digest")) {
		return refuse(stageDecode, "bounds")
	}
	return nil
}

func admitEvidence(rawValue Value, path string) *Refusal {
	if !structValue(rawValue, []string{"adapter", "plan_id"}) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"adapter", "plan_id"})
	if !isClosedObject(value, []string{"adapter", "plan_id"}) {
		return refuse(stageDecode, "shape")
	}
	if refusal := admitIdentity(valueOf(value, "adapter"), path+".adapter"); refusal != nil {
		return refusal
	}
	if IsNull(value, "plan_id") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "plan_id") && !IsPlanID(GetString(value, "plan_id")) {
		return refuse(stageDecode, "bounds")
	}
	return nil
}

func admitWriteEntry(rawValue Value, path string) *Refusal {
	if !structValue(rawValue, []string{"path", "action", "sha256"}) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"path", "action", "sha256"})
	if !isClosedObject(value, []string{"action", "path", "sha256"}) {
		return refuse(stageDecode, "shape")
	}
	if !IsLogicalPath(GetString(value, "path")) {
		return refuse(stageDecode, "bounds")
	}
	if !containsString(GeneratedEnums("writeAction"), GetString(value, "action")) {
		return refuse(stageDecode, "shape")
	}
	if IsNull(value, "sha256") {
		return refuse(stageDecode, "explicit-null")
	}
	// `sha256` is a free-form `Option<String>` on the wire (production
	// has no digest grammar here — the action/digest pairing is the
	// client's semantic layer): any string is admissible, only the type
	// and the explicit-null refusal remain (issue #106 review r5 #9).
	if Has(value, "sha256") {
		if _, ok := value.(map[string]any)["sha256"].(string); !ok {
			return refuse(stageDecode, "shape")
		}
	}
	return nil
}

func admitScanEntry(rawValue Value, path string) *Refusal {
	if !structValue(rawValue, []string{"path", "kind", "detail", "evidence"}) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"path", "kind", "detail", "evidence"})
	if !isClosedObject(value, []string{"detail", "evidence", "kind", "path"}) {
		return refuse(stageDecode, "shape")
	}
	if !IsLogicalPath(GetString(value, "path")) || !boundedText(value.(map[string]any)["kind"], 128) {
		return refuse(stageDecode, "bounds")
	}
	if IsNull(value, "detail") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "detail") && !boundedText(value.(map[string]any)["detail"], 128) {
		return refuse(stageDecode, "bounds")
	}
	if IsNull(value, "evidence") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "evidence") {
		// All-default evidence: `{}` and the positional `[]` both decode
		// (signature/references default) exactly like production's serde
		// struct-from-seq (issue #106 review r5 #9).
		evidence, refusal := seqAsMembers(value.(map[string]any)["evidence"], []string{"signature", "references"})
		if refusal != nil {
			return refusal
		}
		if !isClosedObject(evidence, []string{"references", "signature"}) {
			return refuse(stageDecode, "shape")
		}
		if IsNull(evidence, "signature") {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(evidence, "signature") && !IsSha256Digest(GetString(evidence, "signature")) {
			return refuse(stageDecode, "bounds")
		}
		// `references` is a `#[serde(default)] Vec`: absent defaults
		// empty; an explicit null is a typed refusal, never a default.
		if IsNull(evidence, "references") {
			return refuse(stageDecode, "explicit-null")
		}
		references, refusal := optionalArray(evidence, "references")
		if refusal != nil {
			return refusal
		}
		if len(references) > int(generated.REFERENCES_MAX) {
			return refuse(stageDecode, "bounds")
		}
		for index, reference := range references {
			if !structValue(reference, []string{"target", "role", "confidence"}) {
				return refuse(stageDecode, "shape")
			}
			mapped, _ := seqAsMembers(reference, []string{"target", "role", "confidence"})
			references[index] = mapped
			reference = mapped
			if !isClosedObject(reference, []string{"confidence", "role", "target"}) {
				return refuse(stageDecode, "shape")
			}
			// The target carries the manifest-local semantic-id grammar
			// (core trace::id::is_semantic_id, issue #106 review r3 #8):
			// production's response decoder refuses path-style slash
			// targets, so the SDK must too.
			if !IsSemanticId(GetString(reference, "target")) {
				return refuse(stageDecode, "bounds")
			}
			if !containsString(GeneratedEnums("scanEntryReferenceRole"), GetString(reference, "role")) {
				return refuse(stageDecode, "shape")
			}
			if !containsString(GeneratedEnums("scanEntryConfidence"), GetString(reference, "confidence")) {
				return refuse(stageDecode, "shape")
			}
		}
	}
	return nil
}

func admitNativePlan(rawValue Value, path string) *Refusal {
	allowed := []string{"commands", "completeness", "digest", "entries", "kind", "packages", "run_eligibility", "units"}
	if !structValue(rawValue, allowed) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"kind", "digest", "commands", "packages", "completeness", "run_eligibility"})
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	if !containsString(GeneratedEnums("nativePlanKind"), GetString(value, "kind")) {
		return refuse(stageDecode, "shape")
	}
	if !IsSha256Digest(GetString(value, "digest")) {
		return refuse(stageDecode, "bounds")
	}
	for _, member := range []string{"commands", "entries", "packages", "units"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(value, member) && !numberInRange(value.(map[string]any)[member], 0, generated.MAX_CONSTRAINT_VALUE) {
			return refuse(stageDecode, "bounds")
		}
	}
	for _, member := range []string{"completeness", "run_eligibility"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(value, member) {
			vocabulary := GeneratedEnums("nativeCompleteness")
			if member == "run_eligibility" {
				vocabulary = GeneratedEnums("nativeRunEligibility")
			}
			if !containsString(vocabulary, GetString(value, member)) {
				return refuse(stageDecode, "shape")
			}
		}
	}
	return nil
}

func admitResult(memberValue Value, path string) *Refusal {
	// All-default operation result: production deserializes it from a
	// positional seq too (`result:[]` is the all-default value), exactly
	// like serde struct-from-seq (issue #106 review r5 #9).
	value, refusal := seqAsMembers(memberValue, []string{"ok", "truncated", "entries", "findings", "bindings", "native_plan", "lint_evidence"})
	if refusal != nil {
		return refusal
	}
	allowed := []string{"bindings", "entries", "findings", "lint_evidence", "native_plan", "ok", "truncated"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	for _, member := range []string{"ok", "truncated"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(value, member) {
			if _, ok := GetBool(value, member); !ok {
				return refuse(stageDecode, "shape")
			}
		}
	}
	if IsNull(value, "entries") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "entries") {
		entries := value.(map[string]any)["entries"]
		items, ok := entries.([]any)
		if !ok || len(items) > int(generated.SCAN_ENTRIES_MAX) {
			return refuse(stageDecode, "bounds")
		}
		for index, entry := range items {
			if refusal := admitScanEntry(entry, fmt.Sprintf("result.entries.%d", index)); refusal != nil {
				return refusal
			}
		}
	}
	if IsNull(value, "findings") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "findings") {
		findings := value.(map[string]any)["findings"]
		items, ok := findings.([]any)
		if !ok || len(items) > int(generated.FINDINGS_MAX) {
			return refuse(stageDecode, "bounds")
		}
		for index, finding := range items {
			if !structValue(finding, []string{"path", "code", "detail"}) {
				return refuse(stageDecode, "shape")
			}
			mapped, _ := seqAsMembers(finding, []string{"path", "code", "detail"})
			items[index] = mapped
			finding = mapped
			if !isClosedObject(finding, []string{"code", "detail", "path"}) {
				return refuse(stageDecode, "shape")
			}
			if !IsLogicalPath(GetString(finding, "path")) || !boundedText(finding.(map[string]any)["code"], int(generated.ERROR_CODE_MAX)) {
				return refuse(stageDecode, "bounds")
			}
			if IsNull(finding, "detail") {
				return refuse(stageDecode, "explicit-null")
			}
			if Has(finding, "detail") && !boundedText(finding.(map[string]any)["detail"], int(generated.ERROR_DETAIL_MAX)) {
				return refuse(stageDecode, "bounds")
			}
		}
	}
	if IsNull(value, "bindings") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "bindings") {
		bindings := value.(map[string]any)["bindings"]
		items, ok := bindings.([]any)
		if !ok || len(items) > int(generated.BINDINGS_MAX) {
			return refuse(stageDecode, "bounds")
		}
		for index, binding := range items {
			if !structValue(binding, []string{"module", "target", "profile"}) {
				return refuse(stageDecode, "shape")
			}
			mapped, _ := seqAsMembers(binding, []string{"module", "target", "profile"})
			items[index] = mapped
			binding = mapped
			if !isClosedObject(binding, []string{"module", "profile", "target"}) {
				return refuse(stageDecode, "shape")
			}
			for _, member := range []string{"module", "profile", "target"} {
				if !IsToken(GetString(binding, member)) {
					return refuse(stageDecode, "bounds")
				}
			}
		}
	}
	if IsNull(value, "native_plan") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "native_plan") {
		if refusal := admitNativePlan(valueOf(value, "native_plan"), "result.native_plan"); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "lint_evidence") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "lint_evidence") {
		// The closed LintEvidence subtree is validated before emission
		// (issue #106 review r2 #3): a shallow object check admitted `{}`
		// as `status:"ok"`; the full pinned schema subtree must hold.
		if refusal := admitLintEvidence(value.(map[string]any)["lint_evidence"]); refusal != nil {
			return refusal
		}
	}
	return nil
}

// The bounded LintEvidence string grammar: 1..=256 chars, first
// alphanumeric, then `[A-Za-z0-9_./:#@\\-]`.
func lintBounded256(value Value) bool {
	text, ok := value.(string)
	if !ok || text == "" || len(text) > 256 {
		return false
	}
	for index, char := range text {
		ok := char >= 'a' && char <= 'z' || char >= 'A' && char <= 'Z' || char >= '0' && char <= '9'
		if index > 0 {
			ok = ok || char == '_' || char == '.' || char == '/' || char == ':' || char == '#' || char == '@' || char == '\\' || char == '-'
		}
		if !ok {
			return false
		}
	}
	return true
}

func lintStringArray(value Value, max int, unique bool) bool {
	items, ok := value.([]any)
	if !ok || len(items) > max {
		return false
	}
	seen := map[string]bool{}
	for _, item := range items {
		if !lintBounded256(item) {
			return false
		}
		if unique {
			// Comma-ok at every element access (issue #106 review r5 #3):
			// lintBounded256 already refuses non-strings, but the key
			// extraction must not trust it — a panic here kills the process.
			key, ok := item.(string)
			if !ok {
				return false
			}
			if seen[key] {
				return false
			}
			seen[key] = true
		}
	}
	return true
}

// The lint-side State<T> seq spelling (issue #106 review r6 #1, r8
// #2): the normalized {state,value} map only for the STRICT serde pair
// — exactly two elements, and the unit variants carry only the null
// payload. An empty seq never reaches seq[0] (r8 #6: index-out-of-range
// is a process panic, not a bounded refusal).
func lintStateSeq(rawValue Value) (map[string]any, bool) {
	seq, ok := rawValue.([]any)
	if !ok || len(seq) != 2 {
		return nil, false
	}
	tag, _ := seq[0].(string)
	if !containsString([]string{"known", "unknown", "withheld", "unsupported"}, tag) {
		return nil, false
	}
	if tag != "known" && seq[1] != nil {
		return nil, false
	}
	return map[string]any{"state": tag, "value": seq[1]}, true
}

// The lint-side State<T> verdicts are tri-state (issue #106 review r8
// #9): "" means ok, "shape" is serde's typed decode failure class,
// "bounds" is the bounded known-value grammar.
func lintStateString(rawValue Value) string {
	if seq, ok := lintStateSeq(rawValue); ok {
		if seq["state"] != "known" {
			return ""
		}
		if _, isText := seq["value"].(string); isText {
			if lintBounded256(seq["value"]) {
				return ""
			}
			return "bounds"
		}
		return "shape"
	}
	// A State value that IS an array but not the strict pair (1 or 3+
	// elements, wrong tag, non-null unit payload) is `shape` — the
	// struct-remap below must never re-admit it as {state:<element0>}
	// (issue #106 review r8 #2).
	if _, isSeq := rawValue.([]any); isSeq {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, []string{"state", "value"})
	if !isClosedObject(value, []string{"state", "value"}) {
		return "shape"
	}
	switch state := GetString(value, "state"); state {
	case "known":
		if _, isText := value.(map[string]any)["value"].(string); !isText {
			return "shape"
		}
		if lintBounded256(value.(map[string]any)["value"]) {
			return ""
		}
		return "bounds"
	case "unknown", "withheld", "unsupported":
		if Has(value, "value") {
			return "shape"
		}
		return ""
	}
	return "shape"
}

func lintStateBool(rawValue Value) string {
	if seq, ok := lintStateSeq(rawValue); ok {
		if seq["state"] != "known" {
			return ""
		}
		if _, isBool := seq["value"].(bool); isBool {
			return ""
		}
		return "shape"
	}
	if _, isSeq := rawValue.([]any); isSeq {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, []string{"state", "value"})
	if !isClosedObject(value, []string{"state", "value"}) {
		return "shape"
	}
	switch state := GetString(value, "state"); state {
	case "known":
		if _, isBool := value.(map[string]any)["value"].(bool); isBool {
			return ""
		}
		return "shape"
	case "unknown", "withheld", "unsupported":
		if Has(value, "value") {
			return "shape"
		}
		return ""
	}
	return "shape"
}

func lintStateU64(rawValue Value) string {
	if seq, ok := lintStateSeq(rawValue); ok {
		if seq["state"] != "known" {
			return ""
		}
		number, isNum := seq["value"].(int64)
		if !isNum || number < 0 {
			return "shape"
		}
		if number > 10_000_000 {
			return "bounds"
		}
		return ""
	}
	if _, isSeq := rawValue.([]any); isSeq {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, []string{"state", "value"})
	if !isClosedObject(value, []string{"state", "value"}) {
		return "shape"
	}
	switch state := GetString(value, "state"); state {
	case "known":
		number, isNum := GetNumber(value, "value")
		if !isNum || number < 0 {
			return "shape"
		}
		if number > 10_000_000 {
			return "bounds"
		}
		return ""
	case "unknown", "withheld", "unsupported":
		if Has(value, "value") {
			return "shape"
		}
		return ""
	}
	return "shape"
}

func lintSha256(value Value) bool {
	text, ok := value.(string)
	return ok && text != "" && len(text) <= 71 && IsSha256Digest(text)
}

func lintPins(rawValue Value) string {
	allowed := []string{"capabilities", "ir", "model", "observed", "profile", "revision"}
	if !structValue(rawValue, allowed) {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, []string{"model", "ir", "observed", "revision", "profile", "capabilities"})
	if !isClosedObject(value, allowed) {
		return "shape"
	}
	for _, pin := range allowed {
		if verdict := lintStateString(valueOf(value, pin)); verdict != "" {
			return verdict
		}
	}
	return ""
}

func lintProducer(rawValue Value) string {
	allowed := []string{"artifactDigest", "compiler", "framework", "id", "recipe", "tool", "version"}
	if !structValue(rawValue, allowed) {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, []string{"id", "version", "artifactDigest", "tool", "compiler", "framework", "recipe"})
	if !isClosedObject(value, allowed) {
		return "shape"
	}
	for _, member := range []string{"id", "version", "tool", "recipe"} {
		if !lintBounded256(valueOf(value, member)) {
			if _, isText := valueOf(value, member).(string); isText {
				return "bounds"
			}
			return "shape"
		}
	}
	if digest, isText := valueOf(value, "artifactDigest").(string); isText {
		if !lintSha256(digest) {
			return "bounds"
		}
	} else {
		return "shape"
	}
	for _, member := range []string{"compiler", "framework"} {
		if verdict := lintStateString(valueOf(value, member)); verdict != "" {
			return verdict
		}
	}
	return ""
}

func lintSource(rawValue Value) bool {
	allowed := []string{"bytes", "fingerprint", "id", "path"}
	if !structValue(rawValue, allowed) {
		return false
	}
	value, _ := seqAsMembers(rawValue, []string{"id", "path", "fingerprint", "bytes"})
	if !isClosedObject(value, allowed) {
		return false
	}
	path := GetString(value, "path")
	return lintBounded256(value.(map[string]any)["id"]) &&
		path != "" && len(path) <= 512 && IsLogicalPath(path) &&
		lintSha256(value.(map[string]any)["fingerprint"]) &&
		func() bool {
			number, ok := GetNumber(value, "bytes")
			return ok && number >= 0 && number <= generatedBounds.MaxRequestBytes
		}()
}

func lintSpan(rawValue Value) bool {
	allowed := []string{"column", "end", "endColumn", "endLine", "id", "line", "source", "start"}
	if !structValue(rawValue, allowed) {
		return false
	}
	value, _ := seqAsMembers(rawValue, []string{"id", "source", "start", "end", "line", "column", "endLine", "endColumn"})
	if !isClosedObject(value, allowed) {
		return false
	}
	// Every span member is an unbounded `u64` in production (0 is a
	// legal line/column; the SDK's old min-1 refused legal evidence —
	// review r5 #9).
	bounded := func(member string) bool {
		number, ok := GetNumber(value, member)
		return ok && number >= 0 && number <= generatedBounds.MaxRequestBytes
	}
	return lintBounded256(value.(map[string]any)["id"]) &&
		lintBounded256(value.(map[string]any)["source"]) &&
		bounded("start") && bounded("end") &&
		bounded("line") && bounded("column") &&
		bounded("endLine") && bounded("endColumn")
}

func lintCoverage(rawValue Value) string {
	allowed := []string{"eligible", "examined", "limitations", "rule", "scope", "state", "target"}
	if !structValue(rawValue, allowed) {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, []string{"rule", "target", "scope", "state", "eligible", "examined", "limitations"})
	if !isClosedObject(value, allowed) {
		return "shape"
	}
	state := GetString(value, "state")
	switch state {
	case "complete", "partial", "unknown", "unsupported", "withheld", "disabled":
	default:
		return "shape"
	}
	for _, member := range []string{"rule", "target", "scope"} {
		if !lintBounded256(value.(map[string]any)[member]) {
			if _, isText := value.(map[string]any)[member].(string); isText {
				return "bounds"
			}
			return "shape"
		}
	}
	for _, member := range []string{"eligible", "examined"} {
		if verdict := lintStateU64(value.(map[string]any)[member]); verdict != "" {
			return verdict
		}
	}
	if !lintStringArray(value.(map[string]any)["limitations"], 10_000, true) {
		if _, isArray := value.(map[string]any)["limitations"].([]any); isArray {
			return "bounds"
		}
		return "shape"
	}
	return ""
}

func lintRoute(rawValue Value) bool {
	if !structValue(rawValue, []string{"identity", "confidence", "currency", "origin"}) {
		return false
	}
	value, _ := seqAsMembers(rawValue, []string{"identity", "confidence", "currency", "origin"})
	if !isClosedObject(value, []string{"confidence", "currency", "identity", "origin"}) {
		return false
	}
	switch GetString(value, "confidence") {
	case "unknown", "low", "medium", "high", "exact":
	default:
		return false
	}
	switch GetString(value, "currency") {
	case "current", "stale", "unknown":
	default:
		return false
	}
	switch GetString(value, "origin") {
	case "explicit", "extracted", "inferred":
	default:
		return false
	}
	return lintBounded256(value.(map[string]any)["identity"])
}

func lintStep(rawValue Value) bool {
	if !structValue(rawValue, []string{"role", "identity", "locations", "guards", "confidence"}) {
		return false
	}
	value, _ := seqAsMembers(rawValue, []string{"role", "identity", "locations", "guards", "confidence"})
	if !isClosedObject(value, []string{"confidence", "guards", "identity", "locations", "role"}) {
		return false
	}
	switch GetString(value, "role") {
	case "binding", "trigger", "registration", "callback", "effect":
	default:
		return false
	}
	switch GetString(value, "confidence") {
	case "unknown", "low", "medium", "high", "exact":
	default:
		return false
	}
	return lintBounded256(value.(map[string]any)["identity"]) &&
		lintStringArray(value.(map[string]any)["locations"], 32, true) &&
		lintStringArray(value.(map[string]any)["guards"], 32, true)
}

func lintRecord(rawValue Value) string {
	allowed := []string{
		"activation", "binding", "candidates", "claim", "confidence", "configuration",
		"currency", "field", "guards", "id", "key", "kind", "locations", "mechanism",
		"nativeId", "operation", "origin", "ownership", "resource", "semanticSymbol",
		"subject", "trace", "value",
	}
	recordFields := []string{"id", "kind", "mechanism", "subject", "semanticSymbol", "operation", "resource", "field", "nativeId", "confidence", "currency", "origin", "claim", "binding", "configuration", "ownership", "trace", "value", "key", "candidates", "activation", "locations", "guards"}
	if !structValue(rawValue, recordFields) {
		return "shape"
	}
	value, _ := seqAsMembers(rawValue, recordFields)
	if !isClosedObject(value, allowed) {
		return "shape"
	}
	// Required members and enums are serde's typed-decode `shape` class;
	// the bounded string grammars stay `bounds` (issue #106 review r8
	// #9).
	switch GetString(value, "kind") {
	case "binding-candidates", "dispatch", "convention", "effect", "path", "reflection", "string-reference", "default", "native-edge", "field-write":
	default:
		return "shape"
	}
	switch GetString(value, "mechanism") {
	case "direct", "observer", "hook", "magic":
	default:
		return "shape"
	}
	switch GetString(value, "confidence") {
	case "unknown", "low", "medium", "high", "exact":
	default:
		return "shape"
	}
	switch GetString(value, "currency") {
	case "current", "stale", "unknown":
	default:
		return "shape"
	}
	switch GetString(value, "origin") {
	case "explicit", "extracted", "inferred":
	default:
		return "shape"
	}
	switch GetString(value, "claim") {
	case "structural", "possible-behavior", "verified-behavior":
	default:
		return "shape"
	}
	for _, member := range []string{"id", "subject", "nativeId"} {
		if !lintBounded256(value.(map[string]any)[member]) {
			if _, isText := value.(map[string]any)[member].(string); isText {
				return "bounds"
			}
			return "shape"
		}
	}
	for _, member := range []string{"semanticSymbol", "operation", "resource", "field", "value", "key"} {
		if verdict := lintStateString(value.(map[string]any)[member]); verdict != "" {
			return verdict
		}
	}
	for _, member := range []string{"binding", "configuration", "ownership", "trace"} {
		if verdict := lintStateBool(value.(map[string]any)[member]); verdict != "" {
			return verdict
		}
	}
	for _, member := range []string{"candidates", "activation"} {
		items, isArray := value.(map[string]any)[member].([]any)
		if !isArray {
			return "shape"
		}
		if len(items) > 32 {
			return "bounds"
		}
	}
	for _, item := range value.(map[string]any)["candidates"].([]any) {
		if !lintRoute(item) {
			return "bounds"
		}
	}
	for _, item := range value.(map[string]any)["activation"].([]any) {
		if !lintStep(item) {
			return "bounds"
		}
	}
	for _, member := range []string{"locations", "guards"} {
		if !lintStringArray(value.(map[string]any)[member], 32, true) {
			if _, isArray := value.(map[string]any)[member].([]any); isArray {
				return "bounds"
			}
			return "shape"
		}
	}
	return ""
}

// admitLintEvidence validates the closed `LintEvidence` subtree of a lint
// result (issue #106 review r2 #3): all twelve members required, const
// identity/version, closed nested objects — shallow or malformed evidence
// is refused instead of being emitted as success.
func admitLintEvidence(rawValue Value) *Refusal {
	allowed := []string{
		"coverage", "identity", "inputManifestDigest", "limitations", "locations",
		"pins", "producer", "records", "schemaVersion", "scope", "sources", "target",
	}
	if !structValue(rawValue, allowed) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"schemaVersion", "identity", "target", "scope", "producer", "pins", "inputManifestDigest", "sources", "locations", "coverage", "records", "limitations"})
	object, ok := value.(map[string]any)
	if !ok || !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	for _, member := range allowed {
		if !Has(value, member) {
			return refuse(stageDecode, "shape")
		}
	}
	if GetString(value, "schemaVersion") != "lekalo/ai-lint-evidence/v0.6.4" ||
		GetString(value, "identity") != "dev.lekalo.ai-lint-evidence@0.6.4" {
		return refuse(stageDecode, "shape")
	}
	if _, isText := object["target"].(string); !isText {
		return refuse(stageDecode, "shape")
	}
	if !lintBounded256(object["target"]) {
		return refuse(stageDecode, "bounds")
	}
	if _, isArray := object["scope"].([]any); !isArray {
		return refuse(stageDecode, "shape")
	}
	if !lintStringArray(object["scope"], 10_000, true) {
		return refuse(stageDecode, "bounds")
	}
	// The lint-subtree helpers are tri-state (issue #106 review r8 #9):
	// a wrong spelling is the typed-decode `shape` class, the bounded
	// grammars are `bounds`.
	if verdict := lintProducer(object["producer"]); verdict != "" {
		return refuse(stageDecode, verdict)
	}
	if verdict := lintPins(object["pins"]); verdict != "" {
		return refuse(stageDecode, verdict)
	}
	if digest, isText := object["inputManifestDigest"].(string); !isText {
		return refuse(stageDecode, "shape")
	} else if !lintSha256(digest) {
		return refuse(stageDecode, "bounds")
	}
	sources, ok := object["sources"].([]any)
	if !ok || len(sources) > 10_000 {
		return refuse(stageDecode, "bounds")
	}
	for _, source := range sources {
		if !lintSource(source) {
			return refuse(stageDecode, "bounds")
		}
	}
	spans, ok := object["locations"].([]any)
	if !ok || len(spans) > 32 {
		return refuse(stageDecode, "bounds")
	}
	spanKeys := map[string]bool{}
	for _, span := range spans {
		if !lintSpan(span) {
			return refuse(stageDecode, "bounds")
		}
		key := Canonical(span)
		if spanKeys[key] {
			return refuse(stageDecode, "bounds")
		}
		spanKeys[key] = true
	}
	coverages, ok := object["coverage"].([]any)
	if !ok || len(coverages) > 10_000 {
		return refuse(stageDecode, "bounds")
	}
	for _, coverage := range coverages {
		if verdict := lintCoverage(coverage); verdict != "" {
			return refuse(stageDecode, verdict)
		}
	}
	records, ok := object["records"].([]any)
	if !ok || len(records) > 10_000 {
		return refuse(stageDecode, "bounds")
	}
	for _, record := range records {
		if verdict := lintRecord(record); verdict != "" {
			return refuse(stageDecode, verdict)
		}
	}
	if !lintStringArray(object["limitations"], 10_000, true) {
		return refuse(stageDecode, "bounds")
	}
	return nil
}

func admitProgress(value Value, path string) *Refusal {
	steps, ok := value.([]any)
	if !ok || len(steps) > int(generated.PROGRESS_STEPS_MAX) {
		return refuse(stageDecode, "bounds")
	}
	for index, step := range steps {
		if !structValue(step, []string{"step", "state", "detail"}) {
			return refuse(stageDecode, "shape")
		}
		mapped, _ := seqAsMembers(step, []string{"step", "state", "detail"})
		steps[index] = mapped
		step = mapped
		if !isClosedObject(step, []string{"detail", "state", "step"}) {
			return refuse(stageDecode, "shape")
		}
		if !boundedText(step.(map[string]any)["step"], int(generated.PROGRESS_TEXT_MAX)) {
			return refuse(stageDecode, "bounds")
		}
		if !containsString(GeneratedEnums("progressState"), GetString(step, "state")) {
			return refuse(stageDecode, "shape")
		}
		if IsNull(step, "detail") {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(step, "detail") && !boundedText(step.(map[string]any)["detail"], int(generated.PROGRESS_TEXT_MAX)) {
			return refuse(stageDecode, "bounds")
		}
	}
	return nil
}

func admitError(rawValue Value, path string) *Refusal {
	allowed := []string{"class", "code", "detail", "message", "partial", "retryable"}
	if !structValue(rawValue, allowed) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"class", "code", "message", "retryable", "partial", "detail"})
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	if !containsString(GeneratedEnums("errorClass"), GetString(value, "class")) {
		return refuse(stageDecode, "shape")
	}
	// Production's response validation refuses empty error strings after
	// the bounded check (wire.rs `error.code.is_empty()`), so empty or
	// wrong-typed strings are the typed-decode `shape` class, never
	// `bounds` (issue #106 review r6 #3, r8 #7/#9).
	if _, isText := value.(map[string]any)["code"].(string); !isText {
		return refuse(stageDecode, "shape")
	}
	if _, isText := value.(map[string]any)["message"].(string); !isText {
		return refuse(stageDecode, "shape")
	}
	if value.(map[string]any)["code"] == "" || value.(map[string]any)["message"] == "" {
		return refuse(stageDecode, "shape")
	}
	if !boundedText(value.(map[string]any)["code"], int(generated.ERROR_CODE_MAX)) || !boundedText(value.(map[string]any)["message"], int(generated.ERROR_MESSAGE_MAX)) {
		return refuse(stageDecode, "bounds")
	}
	for _, member := range []string{"partial", "retryable"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(value, member) {
			if _, ok := GetBool(value, member); !ok {
				return refuse(stageDecode, "shape")
			}
		}
	}
	if IsNull(value, "detail") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "detail") {
		details, ok := value.(map[string]any)["detail"].([]any)
		if !ok || len(details) > int(generated.ERROR_DETAILS_MAX) {
			return refuse(stageDecode, "bounds")
		}
		for _, detail := range details {
			if !boundedText(detail, int(generated.ERROR_DETAIL_MAX)) {
				return refuse(stageDecode, "bounds")
			}
		}
	}
	return nil
}

func uniqueBounded(items []any, max int) bool {
	if len(items) > max {
		return false
	}
	seen := map[string]bool{}
	for _, item := range items {
		key := Canonical(item)
		if seen[key] {
			return false
		}
		seen[key] = true
	}
	return true
}

func admitCapabilities(rawValue Value, protocolVersion string, path string) *Refusal {
	allowed := []string{"adapter", "capabilities", "constraints", "ir_versions", "operations", "profiles", "progress", "protocol_versions", "read_scopes", "targets", "transports", "write_scopes"}
	if !structValue(rawValue, allowed) {
		return refuse(stageDecode, "shape")
	}
	value, _ := seqAsMembers(rawValue, []string{"adapter", "protocol_versions", "operations", "transports", "targets", "profiles", "read_scopes", "write_scopes", "progress", "ir_versions", "capabilities", "constraints"})
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	if refusal := admitIdentity(valueOf(value, "adapter"), path+".adapter"); refusal != nil {
		return refusal
	}
	// min-1 is part of admission for the required declared lists
	// (production `unique(…, 1, …)`, issue #106 review r5 #1): an empty
	// declaration is `bounds`, never an empty default.
	requiredList := func(member string, max int, check func(string) bool) *Refusal {
		items, ok := value.(map[string]any)[member].([]any)
		if !ok || len(items) == 0 || !uniqueBounded(items, max) {
			return refuse(stageDecode, "bounds")
		}
		for _, item := range items {
			text, ok := item.(string)
			if !ok || !check(text) {
				return refuse(stageDecode, "bounds")
			}
		}
		return nil
	}
	optionalList := func(member string, max int, check func(string) bool) *Refusal {
		if IsNull(value, member) {
			return refuse(stageDecode, "bounds")
		}
		if !Has(value, member) {
			return nil
		}
		items, ok := value.(map[string]any)[member].([]any)
		if !ok || !uniqueBounded(items, max) {
			return refuse(stageDecode, "bounds")
		}
		for _, item := range items {
			text, ok := item.(string)
			if !ok || !check(text) {
				return refuse(stageDecode, "bounds")
			}
		}
		return nil
	}
	isContract := IsContractVersion
	if refusal := requiredList("protocol_versions", int(generated.PROTOCOL_VERSIONS_MAX), isContract); refusal != nil {
		return refusal
	}
	// Operation set gating mirrors production validate_bounds exactly
	// (issue #106 review r5 #1): `lint` is legal only on a 0.6.4
	// describe, and the closed operation ceiling is 10 there, 9 on
	// every other version.
	operationsMax := int(generated.OPERATIONS_MAX_LINT)
	if protocolVersion != "0.6.4" {
		operationsMax = int(generated.OPERATIONS_MAX_BASE)
	}
	if refusal := requiredList("operations", operationsMax, func(op string) bool {
		return containsString(generated.LatestOperations, op) && (protocolVersion == "0.6.4" || op != "lint")
	}); refusal != nil {
		return refusal
	}
	if refusal := requiredList("transports", int(generated.TRANSPORTS_MAX), func(t string) bool {
		return containsString(GeneratedEnums("transport"), t)
	}); refusal != nil {
		return refusal
	}
	if refusal := optionalList("targets", int(generated.TARGETS_MAX), IsToken); refusal != nil {
		return refusal
	}
	if refusal := optionalList("profiles", int(generated.PROFILES_MAX), IsToken); refusal != nil {
		return refusal
	}
	// Production checks only uniqueness and the count ceiling on the
	// scope lists (wire.rs `unique(&c.read_scopes, 0, 64)`); item content
	// is unconstrained text, so the SDK's extra non-empty / length
	// grammar is an over-strict divergence (issue #106 review r6 #4, r8
	// #8) and stays out of admission — only the item type remains.
	scopeText := func(string) bool { return true }
	if refusal := optionalList("read_scopes", int(generated.MAX_SCOPES), scopeText); refusal != nil {
		return refusal
	}
	if refusal := optionalList("write_scopes", int(generated.MAX_SCOPES), scopeText); refusal != nil {
		return refusal
	}
	if refusal := optionalList("ir_versions", int(generated.MAX_IR_VERSIONS), isContract); refusal != nil {
		return refusal
	}
	// `progress` is a `#[serde(default)] bool` in production: absent is
	// legal (defaults false); a present non-boolean is a typed refusal
	// (review r5 #9).
	if Has(value, "progress") {
		if _, ok := GetBool(value, "progress"); !ok {
			return refuse(stageDecode, "shape")
		}
	}
	if IsNull(value, "capabilities") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "capabilities") {
		capabilityMap, ok := value.(map[string]any)["capabilities"].(map[string]any)
		if !ok {
			return refuse(stageDecode, "shape")
		}
		if len(capabilityMap) > int(generated.MAX_DECLARED_CAPABILITIES) {
			return refuse(stageDecode, "bounds")
		}
		for id, support := range capabilityMap {
			if !IsCapabilityID(id) {
				return refuse(stageDecode, "bounds")
			}
			// Registry membership is part of admission (production
			// consults `capability::definition(id)`), not a
			// discovery-time nicety (issue #106 review r5 #1).
			known := false
			for _, definition := range generated.CapabilityDefinitions {
				if definition.ID == id {
					known = true
				}
			}
			if !known {
				return refuse(stageDecode, "bounds")
			}
			text, ok := support.(string)
			if !ok || !containsString(GeneratedEnums("supportState"), text) {
				return refuse(stageDecode, "shape")
			}
		}
	}
	// `constraints` is a plain `Option` in production: absent and
	// explicit null are both legal, the members are `1..=MAX` when
	// present, and the all-default struct deserializes from `{}` or a
	// positional `[]` (issue #106 review r5 #1/#9).
	if Has(value, "constraints") && !IsNull(value, "constraints") {
		constraints, refusal := seqAsMembers(value.(map[string]any)["constraints"], []string{"max_entries", "max_writes"})
		if refusal != nil {
			return refusal
		}
		if !isClosedObject(constraints, []string{"max_entries", "max_writes"}) {
			return refuse(stageDecode, "shape")
		}
		for _, member := range []string{"max_entries", "max_writes"} {
			if Has(constraints, member) && !IsNull(constraints, member) && !numberInRange(constraints.(map[string]any)[member], 1, generated.MAX_CONSTRAINT_VALUE) {
				return refuse(stageDecode, "bounds")
			}
		}
	}
	return nil
}

// AdmitResponseValue runs the structural admission pass on one response.
func AdmitResponseValue(value Value) (Value, *Refusal) {
	allowed := []string{"capabilities", "error", "evidence", "operation", "progress", "protocol", "protocol_version", "request_id", "result", "status", "writes"}
	fields := []string{"protocol", "protocol_version", "operation", "request_id", "status", "evidence", "capabilities", "result", "writes", "progress", "error"}
	if !structValue(value, fields) {
		return nil, refuse(stageDecode, unknownOrShape(value, allowed))
	}
	// A null ELEMENT inside a positional envelope seq is serde's typed
	// invalid-type shape, never the map spelling's explicit-null token
	// (issue #106 review r8 #9).
	nullReason := "explicit-null"
	if _, isSeq := value.([]any); isSeq {
		nullReason = "shape"
	}
	value, _ = seqAsMembers(value, fields)
	if !isClosedObject(value, allowed) {
		return nil, refuse(stageDecode, unknownOrShape(value, allowed))
	}
	for _, member := range []string{"protocol", "protocol_version", "operation", "request_id", "status", "evidence"} {
		if !Has(value, member) {
			return nil, refuse(stageDecode, "shape")
		}
		if IsNull(value, member) {
			return nil, refuse(stageDecode, nullReason)
		}
	}
	if GetString(value, "protocol") != generated.WireToken {
		return nil, refuse(stageDecode, "token")
	}
	if !IsContractVersion(GetString(value, "protocol_version")) {
		return nil, refuse(stageDecode, "version")
	}
	if !containsString(generated.LatestOperations, GetString(value, "operation")) {
		return nil, refuse(stageDecode, "operation")
	}
	if !IsRequestID(GetString(value, "request_id")) {
		return nil, refuse(stageDecode, "request-id")
	}
	if !containsString(GeneratedEnums("responseStatus"), GetString(value, "status")) {
		return nil, refuse(stageDecode, "shape")
	}
	if refusal := admitEvidence(valueOf(value, "evidence"), "evidence"); refusal != nil {
		return nil, refusal
	}
	protocolVersion := GetString(value, "protocol_version")
	if IsNull(value, "capabilities") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "capabilities") {
		if refusal := admitCapabilities(valueOf(value, "capabilities"), protocolVersion, "capabilities"); refusal != nil {
			return nil, refusal
		}
	}
	if IsNull(value, "result") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "result") {
		if refusal := admitResult(valueOf(value, "result"), "result"); refusal != nil {
			return nil, refusal
		}
		// Lint evidence is version-gated exactly like production
		// validate_bounds (issue #106 review r5 #1): the member is legal
		// only on a 0.6.4 `lint` response.
		resultMember := value.(map[string]any)["result"]
		carriesLintEvidence := false
		if object, ok := resultMember.(map[string]any); ok {
			_, carriesLintEvidence = object["lint_evidence"]
		} else if items, ok := resultMember.([]any); ok {
			carriesLintEvidence = len(items) >= 7
		}
		if carriesLintEvidence && (protocolVersion != "0.6.4" || GetString(value, "operation") != "lint") {
			return nil, refuse(stageDecode, "shape")
		}
	}
	if IsNull(value, "writes") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "writes") {
		writes := value.(map[string]any)["writes"]
		items, ok := writes.([]any)
		if !ok || len(items) > int(generated.MAX_PLAN_ENTRIES) {
			return nil, refuse(stageDecode, "bounds")
		}
		for index, entry := range items {
			if !structValue(entry, []string{"path", "action", "sha256"}) {
				return nil, refuse(stageDecode, "shape")
			}
			mapped, _ := seqAsMembers(entry, []string{"path", "action", "sha256"})
			items[index] = mapped
			if refusal := admitWriteEntry(mapped, fmt.Sprintf("writes.%d", index)); refusal != nil {
				return nil, refusal
			}
		}
	}
	if IsNull(value, "progress") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "progress") {
		if refusal := admitProgress(valueOf(value, "progress"), "progress"); refusal != nil {
			return nil, refusal
		}
	}
	if IsNull(value, "error") {
		return nil, refuse(stageDecode, nullReason)
	}
	if Has(value, "error") {
		if refusal := admitError(valueOf(value, "error"), "error"); refusal != nil {
			return nil, refusal
		}
	}
	return value, nil
}

// ValidateRequest runs the cross-member request semantics.
func ValidateRequest(value Value) *Refusal {
	if GetString(value, "protocol") != generated.WireToken {
		return refuse(stageValidate, "version")
	}
	operation := GetString(value, "operation")
	version := GetString(value, "protocol_version")
	hasLint := Has(value, "lint_request")
	if (operation == "lint") != hasLint {
		return refuse(stageValidate, "lint-request")
	}
	if lintValue := value.(map[string]any)["lint_request"]; hasLint {
		if version != lintVersion || !Has(value, "target") {
			return refuse(stageValidate, "lint-request")
		}
		for _, member := range []string{"profile", "profile_digest", "profile_capabilities", "ir_path", "native_request", "dry_run", "plan_id"} {
			if Has(value, member) {
				return refuse(stageValidate, "lint-request")
			}
		}
		scope := GetArray(lintValue, "scope")
		files := GetArray(lintValue, "files")
		if !isSortedStrings(scope) || !isSortedStrings(files) {
			return refuse(stageValidate, "lint-request")
		}
		// Production gates scope items with the ai-lint token grammar,
		// not a length bound (issue #106 review r3 #5).
		for _, item := range scope {
			text, _ := item.(string)
			if !IsWireToken(text) {
				return refuse(stageValidate, "lint-request")
			}
		}
		for _, file := range files {
			// Comma-ok at every element access (issue #106 review r5 #3):
			// a wrong-typed list element is a refusal, never a panic.
			text, ok := file.(string)
			if !ok {
				return refuse(stageValidate, "lint-request")
			}
			if !IsLogicalPath(text) {
				return refuse(stageValidate, "lint-request")
			}
		}
		keys := map[string]bool{}
		for _, binding := range GetArray(lintValue, "bindings") {
			path := GetString(binding, "path")
			if !IsLogicalPath(path) {
				return refuse(stageValidate, "lint-binding")
			}
			found := false
			for _, file := range files {
				if text, ok := file.(string); ok && text == path {
					found = true
				}
			}
			if !found {
				return refuse(stageValidate, "lint-binding")
			}
			if !IsSha256Digest(GetString(binding, "fingerprint")) {
				return refuse(stageValidate, "lint-binding")
			}
			// nativeId and symbol carry the ai-lint token grammar
			// (issue #106 review r3 #5), not a length bound.
			if !IsWireToken(GetString(binding, "nativeId")) || !IsWireToken(GetString(binding, "symbol")) {
				return refuse(stageValidate, "lint-binding")
			}
			key := path + "\x00" + GetString(binding, "nativeId") + "\x00" + GetString(binding, "kind")
			if keys[key] {
				return refuse(stageValidate, "lint-binding")
			}
			keys[key] = true
		}
		pins := lintValue.(map[string]any)["pins"]
		pinKnown := func(pin Value) (string, bool) {
			if GetString(pin, "state") == "known" {
				return GetString(pin, "value"), true
			}
			return "", false
		}
		for _, pin := range []string{"model", "ir"} {
			// Required known digests (issue #106 review r2 #2): production
			// refuses unknown and malformed spellings of the model/ir pins.
			if known, ok := pinKnown(pins.(map[string]any)[pin]); !ok || !IsSha256Digest(known) {
				return refuse(stageValidate, "lint-pin")
			}
		}
		for _, pin := range []string{"observed", "profile", "capabilities"} {
			if known, ok := pinKnown(pins.(map[string]any)[pin]); ok && !IsSha256Digest(known) {
				return refuse(stageValidate, "lint-pin")
			}
		}
		if revision, ok := pinKnown(pins.(map[string]any)["revision"]); ok && !IsWireToken(revision) {
			return refuse(stageValidate, "lint-pin")
		}
	}
	hasNative := Has(value, "native_request")
	if (operation == "plan-native") != hasNative {
		return refuse(stageValidate, "native-request")
	}
	dryRun, hasDryRun := GetBool(value, "dry_run")
	apply := operation == "clean" || (operation == "generate" && hasDryRun && !dryRun)
	hasPlan := Has(value, "plan_id")
	if apply != hasPlan {
		return refuse(stageValidate, "plan-id")
	}
	if hasPlan && !IsPlanID(GetString(value, "plan_id")) {
		return refuse(stageValidate, "plan-id")
	}
	if (operation == "generate") != hasDryRun {
		return refuse(stageValidate, "dry-run")
	}
	if operation == "plan-native" && (hasDryRun || hasPlan) {
		return refuse(stageValidate, "plan-id")
	}
	if requiresIr(operation) && !Has(value, "ir_path") {
		return refuse(stageValidate, "ir-path")
	}
	if (operation == "generate" || operation == "bind") && !Has(value, "target") {
		return refuse(stageValidate, "target")
	}
	if operation == "bind" && !Has(value, "profile") {
		return refuse(stageValidate, "profile")
	}
	if operation == "describe" {
		for _, member := range []string{"target", "profile", "profile_digest", "profile_capabilities", "native_request", "ir_path"} {
			if Has(value, member) {
				return refuse(stageValidate, "member")
			}
		}
	}
	profileResolution := Has(value, "profile_digest") || Has(value, "profile_capabilities")
	if profileResolution && (!Has(value, "profile_digest") || !Has(value, "profile_capabilities") || !Has(value, "profile")) {
		return refuse(stageValidate, "profile-capabilities")
	}
	if profileResolution && !containsString(generated.SupportedProtocolVersions, version) {
		return refuse(stageValidate, "member")
	}
	return nil
}

func requiresIr(operation string) bool {
	return operation == "validate" || operation == "generate" || operation == "verify"
}

func isSortedStrings(items []any) bool {
	for index := 1; index < len(items); index++ {
		previous, ok1 := items[index-1].(string)
		current, ok2 := items[index].(string)
		if !ok1 || !ok2 || previous >= current {
			return false
		}
	}
	return true
}

// ValidateResponseIdentity checks the response echo against the session.
func ValidateResponseIdentity(response Value, expectedProtocolVersion, expectedOperation, expectedRequestID string) *Refusal {
	if GetString(response, "protocol") != generated.WireToken {
		return refuse(stageValidate, "version")
	}
	if GetString(response, "protocol_version") != expectedProtocolVersion {
		return refuse(stageValidate, "echo-version")
	}
	if GetString(response, "operation") != expectedOperation {
		return refuse(stageValidate, "echo-operation")
	}
	if GetString(response, "request_id") != expectedRequestID {
		return refuse(stageValidate, "echo-request-id")
	}
	// The describe handshake binds the evidence identity to the declared
	// capabilities identity exactly like the production client
	// (`evidence.adapter != capabilities.adapter` → `Evidence`
	// invalidity, issue #106 review r5 #10) — an SDK consumer must never
	// accept a response a host session would refuse.
	if Has(response, "capabilities") {
		evidenceAdapter := valueOf(valueOf(response, "evidence"), "adapter")
		declaredAdapter := valueOf(valueOf(response, "capabilities"), "adapter")
		if Canonical(evidenceAdapter) != Canonical(declaredAdapter) {
			return refuse(stageValidate, "identity")
		}
	}
	return nil
}

// ValidateResponsePayload checks the operation payload pairing of one
// response (the validate-before-write obligation).
func ValidateResponsePayload(request, response Value) *Refusal {
	operation := GetString(request, "operation")
	if Has(response, "capabilities") && operation != "describe" {
		return refuse(stageValidate, "member")
	}
	if operation == "describe" && GetString(response, "status") == "ok" && !Has(response, "capabilities") {
		return refuse(stageValidate, "member")
	}
	if operation == "describe" && Has(response, "capabilities") {
		caps := valueOf(response, "capabilities")
		evidence := valueOf(response, "evidence")
		if Canonical(valueOf(evidence, "adapter")) != Canonical(valueOf(caps, "adapter")) {
			return refuse(stageValidate, "identity")
		}
		contains := func(key, expected string) bool {
			for _, item := range GetArray(caps, key) {
				if text, ok := item.(string); ok && text == expected {
					return true
				}
			}
			return false
		}
		if !contains("operations", "describe") || !contains("protocol_versions", GetString(request, "protocol_version")) || Has(response, "progress") {
			return refuse(stageValidate, "member")
		}
	}
	apply := operation == "clean" || (operation == "generate" && requestHasDryRun(request, false))
	evidencePlan := planIDOf(response)
	if apply && evidencePlan != GetString(request, "plan_id") {
		return refuse(stageValidate, "plan-id")
	}
	if GetString(response, "status") == "error" {
		if !Has(response, "error") {
			return refuse(stageValidate, "error-pairing")
		}
		if Has(response, "result") || Has(response, "writes") || Has(response, "progress") {
			return refuse(stageValidate, "error-pairing")
		}
		return nil
	}
	if Has(response, "error") {
		return refuse(stageValidate, "error-pairing")
	}
	if result := valueOf(response, "result"); result != nil {
		if Has(result, "lint_evidence") && operation != "lint" {
			return refuse(stageValidate, "member")
		}
		if Has(result, "entries") && operation != "scan" {
			return refuse(stageValidate, "member")
		}
		if Has(result, "truncated") && operation != "scan" {
			return refuse(stageValidate, "member")
		}
		if Has(result, "bindings") && operation != "bind" {
			return refuse(stageValidate, "member")
		}
		if Has(result, "ok") && !(operation == "validate" || operation == "verify") {
			return refuse(stageValidate, "member")
		}
		if Has(result, "findings") && !(operation == "validate" || operation == "verify") {
			return refuse(stageValidate, "member")
		}
		if Has(result, "native_plan") && operation != "plan-native" {
			return refuse(stageValidate, "member")
		}
		if operation == "plan-native" && Has(result, "entries") {
			return refuse(stageValidate, "member")
		}
	}
	complete := false
	result := valueOf(response, "result")
	switch operation {
	case "scan":
		complete = result != nil && Has(result, "entries")
	case "bind":
		complete = result != nil && Has(result, "bindings")
	case "validate", "verify":
		complete = result != nil && Has(result, "ok")
	case "plan-native":
		complete = result != nil && Has(result, "native_plan")
	case "lint":
		complete = result != nil && Has(result, "lint_evidence")
	default:
		complete = result == nil
	}
	if !complete {
		return refuse(stageValidate, "member")
	}
	if declaresWrites(operation) {
		writes := GetArray(response, "writes")
		if writes == nil {
			return refuse(stageValidate, "writes-missing")
		}
		for _, entry := range writes {
			action := GetString(entry, "action")
			if action == "delete" {
				if Has(entry, "sha256") {
					return refuse(stageValidate, "write-digest")
				}
			} else if !IsSha256Digest(GetString(entry, "sha256")) {
				return refuse(stageValidate, "write-digest")
			}
		}
	} else if Has(response, "writes") {
		return refuse(stageValidate, "member")
	}
	return nil
}

func valueOf(object Value, key string) Value {
	if object, ok := object.(map[string]any); ok {
		return object[key]
	}
	return nil
}

func planIDOf(response Value) string {
	if evidence := valueOf(response, "evidence"); evidence != nil {
		return GetString(evidence, "plan_id")
	}
	return ""
}

func requestHasDryRun(request Value, expected bool) bool {
	value, ok := GetBool(request, "dry_run")
	return ok && value == expected
}

func declaresWrites(operation string) bool {
	return operation == "generate" || operation == "clean" || operation == "plan-clean"
}

var _ = strings.HasPrefix
