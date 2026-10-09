// Strict wire decoding and semantic validation (issue #106): the
// hand-maintained codec obligation over the generated vocabulary.
package runtime

import (
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
	if refusal := AdmitRequestValue(value); refusal != nil {
		return nil, refusal
	}
	if refusal := ValidateRequest(value); refusal != nil {
		return nil, refusal
	}
	return value, nil
}

// AdmitRequestValue runs only the structural admission pass.
func AdmitRequestValue(value Value) *Refusal {
	allowed := []string{"dry_run", "ir_path", "limits", "lint_request", "native_request", "operation", "plan_id", "profile", "profile_capabilities", "profile_digest", "project_root", "protocol", "protocol_version", "request_id", "target"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, unknownOrShape(value, allowed))
	}
	for _, member := range []string{"protocol", "protocol_version", "operation", "request_id", "project_root"} {
		if !Has(value, member) {
			return refuse(stageDecode, "shape")
		}
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
	}
	if GetString(value, "protocol") != generated.WireToken {
		return refuse(stageDecode, "token")
	}
	version := GetString(value, "protocol_version")
	if !containsString(generated.SupportedProtocolVersions, version) {
		return refuse(stageDecode, "version")
	}
	if !containsString(generated.LatestOperations, GetString(value, "operation")) {
		return refuse(stageDecode, "operation")
	}
	if !IsRequestID(GetString(value, "request_id")) {
		return refuse(stageDecode, "request-id")
	}
	if GetString(value, "project_root") != "." {
		return refuse(stageDecode, "grammar")
	}
	for _, member := range []string{"ir_path", "target", "profile"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(value, member) {
			if member == "ir_path" {
				if !IsLogicalPath(GetString(value, member)) {
					return refuse(stageDecode, "grammar")
				}
			} else if !IsToken(GetString(value, member)) {
				return refuse(stageDecode, "grammar")
			}
		}
	}
	if IsNull(value, "profile_digest") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "profile_digest") && !IsSha256Digest(GetString(value, "profile_digest")) {
		return refuse(stageDecode, "bounds")
	}
	if IsNull(value, "dry_run") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "dry_run") {
		if _, ok := GetBool(value, "dry_run"); !ok {
			return refuse(stageDecode, "shape")
		}
	}
	if IsNull(value, "plan_id") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "plan_id") && !IsPlanID(GetString(value, "plan_id")) {
		return refuse(stageDecode, "bounds")
	}
	if IsNull(value, "limits") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "limits") {
		if refusal := admitLimits(value.(map[string]any)["limits"]); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "profile_capabilities") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "profile_capabilities") {
		if refusal := admitProfileCapabilities(value.(map[string]any)["profile_capabilities"]); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "native_request") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "native_request") {
		if refusal := admitNativeRequest(value.(map[string]any)["native_request"]); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "lint_request") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "lint_request") {
		if refusal := admitLintRequest(value.(map[string]any)["lint_request"]); refusal != nil {
			return refusal
		}
	}
	return nil
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

func admitLimits(value Value) *Refusal {
	if !isClosedObject(value, []string{"max_output_bytes", "timeout_ms"}) {
		return refuse(stageDecode, "shape")
	}
	// Production requires BOTH members whenever `limits` is present
	// (wire.rs: absent or out-of-range is admission `shape`); the SDK
	// used to admit partial limits objects. Member order mirrors
	// production's validate: timeout_ms first, so a zero timeout still
	// refuses `bounds` before the missing-member `shape` is reached.
	for _, bound := range []struct {
		member      string
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
	if !ok || len(items) > generated.MAX_DECLARED_CAPABILITIES {
		return refuse(stageDecode, "bounds")
	}
	previous := ""
	for _, capability := range items {
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

func admitContentRef(value Value) *Refusal {
	if !isClosedObject(value, []string{"adapter", "digest", "revision"}) {
		return refuse(stageDecode, "shape")
	}
	if !IsSha256Digest(GetString(value, "digest")) {
		return refuse(stageDecode, "bounds")
	}
	for _, member := range []string{"adapter", "revision"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(value, member) && !boundedText(value.(map[string]any)[member], 128) {
			return refuse(stageDecode, "bounds")
		}
	}
	return nil
}

func admitNativeRequest(value Value) *Refusal {
	allowed := []string{"capability_snapshot_digest", "changes", "execution_policy_ref", "input_manifest_digest", "observed_ref", "scan_ref", "tool_catalog_digest"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	for _, member := range []string{"input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if !IsSha256Digest(GetString(value, member)) {
			return refuse(stageDecode, "bounds")
		}
	}
	changes := value.(map[string]any)["changes"]
	if !isClosedObject(changes, []string{"files", "symbols"}) {
		return refuse(stageDecode, "shape")
	}
	files := GetArray(changes, "files")
	symbols := GetArray(changes, "symbols")
	if len(files) > 1024 || len(symbols) > 1024 {
		return refuse(stageDecode, "bounds")
	}
	previous := ""
	for _, change := range files {
		if !isClosedObject(change, []string{"after_digest", "before_digest", "change", "path"}) {
			return refuse(stageDecode, "shape")
		}
		if !IsLogicalPath(GetString(change, "path")) {
			return refuse(stageDecode, "bounds")
		}
		if !containsString(GeneratedEnums("nativeChangeKind"), GetString(change, "change")) {
			return refuse(stageDecode, "shape")
		}
		for _, member := range []string{"after_digest", "before_digest"} {
			if IsNull(change, member) {
				return refuse(stageDecode, "explicit-null")
			}
			if Has(change, member) && !IsSha256Digest(GetString(change, member)) {
				return refuse(stageDecode, "bounds")
			}
		}
		path := GetString(change, "path")
		if previous != "" && path <= previous {
			return refuse(stageDecode, "bounds")
		}
		previous = path
	}
	for _, symbol := range symbols {
		if !boundedText(symbol, int(generated.ERROR_DETAIL_MAX)) {
			return refuse(stageDecode, "bounds")
		}
	}
	if refusal := admitContentRef(changesOf(value, "scan_ref")); refusal != nil {
		return refusal
	}
	if refusal := admitContentRef(changesOf(value, "execution_policy_ref")); refusal != nil {
		return refusal
	}
	if IsNull(value, "observed_ref") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "observed_ref") {
		if refusal := admitContentRef(value.(map[string]any)["observed_ref"]); refusal != nil {
			return refusal
		}
	}
	return nil
}

func changesOf(value Value, key string) Value {
	return value.(map[string]any)[key]
}

func admitLintRequest(value Value) *Refusal {
	allowed := []string{"bindings", "files", "pins", "scope"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	for _, member := range []string{"scope", "files", "bindings"} {
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
	}
	scope := GetArray(value, "scope")
	files := GetArray(value, "files")
	bindings := GetArray(value, "bindings")
	if len(scope) == 0 || len(scope) > 10000 || len(files) > 4096 || len(bindings) > 10000 {
		return refuse(stageDecode, "bounds")
	}
	for _, scopeEntry := range scope {
		if !boundedText(scopeEntry, int(generated.ERROR_DETAIL_MAX)) {
			return refuse(stageDecode, "bounds")
		}
	}
	for _, file := range files {
		if !IsLogicalPath(file.(string)) {
			return refuse(stageDecode, "bounds")
		}
	}
	for _, binding := range bindings {
		if !isClosedObject(binding, []string{"fingerprint", "kind", "nativeId", "path", "symbol"}) {
			return refuse(stageDecode, "shape")
		}
		if !IsLogicalPath(GetString(binding, "path")) {
			return refuse(stageDecode, "bounds")
		}
		for _, member := range []string{"nativeId", "symbol"} {
			if !boundedText(binding.(map[string]any)[member], 128) {
				return refuse(stageDecode, "bounds")
			}
		}
		if !containsString([]string{"command", "entity"}, GetString(binding, "kind")) {
			return refuse(stageDecode, "shape")
		}
		if !IsSha256Digest(GetString(binding, "fingerprint")) {
			return refuse(stageDecode, "bounds")
		}
	}
	pins := value.(map[string]any)["pins"]
	if !isClosedObject(pins, []string{"capabilities", "ir", "model", "observed", "profile", "revision"}) {
		return refuse(stageDecode, "shape")
	}
	// The pinned 0.6.4 schema carries every pin as a State-tagged value
	// ({state:"known",value}|{state:"unknown"|"withheld"|"unsupported"}).
	for _, member := range []string{"capabilities", "ir", "model", "observed", "profile", "revision"} {
		if IsNull(pins, member) {
			return refuse(stageDecode, "explicit-null")
		}
		if refusal := admitStateString(pins.(map[string]any)[member]); refusal != nil {
			return refusal
		}
	}
	return nil
}

// admitStateString checks one State-tagged string value of the pinned
// 0.6.4 lint schema.
func admitStateString(value Value) *Refusal {
	if value == nil || !isClosedObject(value, []string{"state", "value"}) {
		return refuse(stageDecode, "shape")
	}
	switch GetString(value, "state") {
	case "known":
		keys := value.(map[string]any)
		if len(keys) != 2 {
			return refuse(stageDecode, "shape")
		}
		if !boundedText(keys["value"], 128) {
			return refuse(stageDecode, "bounds")
		}
		return nil
	case "unknown", "withheld", "unsupported":
		if len(value.(map[string]any)) != 1 {
			return refuse(stageDecode, "shape")
		}
		return nil
	default:
		return refuse(stageDecode, "shape")
	}
}

// AdmitResponse decodes one response document (structural admission).
func AdmitResponse(text string) (Value, *Refusal) {
	value, reason := ReadSingleObject(text, generated.DEFAULT_MAX_OUTPUT_BYTES)
	if reason != "" {
		return nil, refuse(stageDecode, string(reason))
	}
	if refusal := AdmitResponseValue(value); refusal != nil {
		return nil, refusal
	}
	return value, nil
}

func admitIdentity(value Value) *Refusal {
	if !isClosedObject(value, []string{"digest", "id", "version"}) {
		return refuse(stageDecode, "shape")
	}
	if !IsToken(GetString(value, "id")) || !IsContractVersion(GetString(value, "version")) || !IsSha256Digest(GetString(value, "digest")) {
		return refuse(stageDecode, "bounds")
	}
	return nil
}

func admitEvidence(value Value) *Refusal {
	if !isClosedObject(value, []string{"adapter", "plan_id"}) {
		return refuse(stageDecode, "shape")
	}
	if refusal := admitIdentity(value.(map[string]any)["adapter"]); refusal != nil {
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

func admitWriteEntry(value Value) *Refusal {
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
	if Has(value, "sha256") && !IsSha256Digest(GetString(value, "sha256")) {
		return refuse(stageDecode, "bounds")
	}
	return nil
}

func admitScanEntry(value Value) *Refusal {
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
		evidence := value.(map[string]any)["evidence"]
		if !isClosedObject(evidence, []string{"references", "signature"}) {
			return refuse(stageDecode, "shape")
		}
		if IsNull(evidence, "signature") {
			return refuse(stageDecode, "explicit-null")
		}
		if Has(evidence, "signature") && !IsSha256Digest(GetString(evidence, "signature")) {
			return refuse(stageDecode, "bounds")
		}
		references := GetArray(evidence, "references")
		if len(references) > int(generated.REFERENCES_MAX) {
			return refuse(stageDecode, "bounds")
		}
		for _, reference := range references {
			if !isClosedObject(reference, []string{"confidence", "role", "target"}) {
				return refuse(stageDecode, "shape")
			}
			if !boundedText(reference.(map[string]any)["target"], 192) {
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

func admitNativePlan(value Value) *Refusal {
	allowed := []string{"commands", "completeness", "digest", "entries", "kind", "packages", "run_eligibility", "units"}
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

func admitResult(value Value) *Refusal {
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
		for _, entry := range items {
			if refusal := admitScanEntry(entry); refusal != nil {
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
		for _, finding := range items {
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
		for _, binding := range items {
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
		if refusal := admitNativePlan(value.(map[string]any)["native_plan"]); refusal != nil {
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
			key := item.(string)
			if seen[key] {
				return false
			}
			seen[key] = true
		}
	}
	return true
}

func lintStateString(value Value) bool {
	if !isClosedObject(value, []string{"state", "value"}) {
		return false
	}
	switch GetString(value, "state") {
	case "known":
		return lintBounded256(value.(map[string]any)["value"])
	case "unknown", "withheld", "unsupported":
		return !Has(value, "value")
	}
	return false
}

func lintStateBool(value Value) bool {
	if !isClosedObject(value, []string{"state", "value"}) {
		return false
	}
	switch GetString(value, "state") {
	case "known":
		_, ok := value.(map[string]any)["value"].(bool)
		return ok
	case "unknown", "withheld", "unsupported":
		return !Has(value, "value")
	}
	return false
}

func lintStateU64(value Value) bool {
	if !isClosedObject(value, []string{"state", "value"}) {
		return false
	}
	switch GetString(value, "state") {
	case "known":
		number, ok := GetNumber(value, "value")
		return ok && number >= 0 && number <= 10_000_000
	case "unknown", "withheld", "unsupported":
		return !Has(value, "value")
	}
	return false
}

func lintSha256(value Value) bool {
	text, ok := value.(string)
	return ok && text != "" && len(text) <= 71 && IsSha256Digest(text)
}

func lintPins(value Value) bool {
	allowed := []string{"capabilities", "ir", "model", "observed", "profile", "revision"}
	if !isClosedObject(value, allowed) {
		return false
	}
	for _, pin := range allowed {
		if !lintStateString(value.(map[string]any)[pin]) {
			return false
		}
	}
	return true
}

func lintProducer(value Value) bool {
	allowed := []string{"artifactDigest", "compiler", "framework", "id", "recipe", "tool", "version"}
	if !isClosedObject(value, allowed) {
		return false
	}
	for _, member := range []string{"id", "version", "tool", "recipe"} {
		if !lintBounded256(value.(map[string]any)[member]) {
			return false
		}
	}
	return lintSha256(value.(map[string]any)["artifactDigest"]) &&
		lintStateString(value.(map[string]any)["compiler"]) &&
		lintStateString(value.(map[string]any)["framework"])
}

func lintSource(value Value) bool {
	allowed := []string{"bytes", "fingerprint", "id", "path"}
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

func lintSpan(value Value) bool {
	allowed := []string{"column", "end", "endColumn", "endLine", "id", "line", "source", "start"}
	if !isClosedObject(value, allowed) {
		return false
	}
	bounded := func(member string, min int64) bool {
		number, ok := GetNumber(value, member)
		return ok && number >= min && number <= generatedBounds.MaxRequestBytes
	}
	return lintBounded256(value.(map[string]any)["id"]) &&
		lintBounded256(value.(map[string]any)["source"]) &&
		bounded("start", 0) && bounded("end", 0) &&
		bounded("line", 1) && bounded("column", 1) &&
		bounded("endLine", 1) && bounded("endColumn", 1)
}

func lintCoverage(value Value) bool {
	allowed := []string{"eligible", "examined", "limitations", "rule", "scope", "state", "target"}
	if !isClosedObject(value, allowed) {
		return false
	}
	state := GetString(value, "state")
	switch state {
	case "complete", "partial", "unknown", "unsupported", "withheld", "disabled":
	default:
		return false
	}
	for _, member := range []string{"rule", "target", "scope"} {
		if !lintBounded256(value.(map[string]any)[member]) {
			return false
		}
	}
	return lintStateU64(value.(map[string]any)["eligible"]) &&
		lintStateU64(value.(map[string]any)["examined"]) &&
		lintStringArray(value.(map[string]any)["limitations"], 10_000, true)
}

func lintRoute(value Value) bool {
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

func lintStep(value Value) bool {
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

func lintRecord(value Value) bool {
	allowed := []string{
		"activation", "binding", "candidates", "claim", "confidence", "configuration",
		"currency", "field", "guards", "id", "key", "kind", "locations", "mechanism",
		"nativeId", "operation", "origin", "ownership", "resource", "semanticSymbol",
		"subject", "trace", "value",
	}
	if !isClosedObject(value, allowed) {
		return false
	}
	switch GetString(value, "kind") {
	case "binding-candidates", "dispatch", "convention", "effect", "path", "reflection", "string-reference", "default", "native-edge", "field-write":
	default:
		return false
	}
	switch GetString(value, "mechanism") {
	case "direct", "observer", "hook", "magic":
	default:
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
	switch GetString(value, "claim") {
	case "structural", "possible-behavior", "verified-behavior":
	default:
		return false
	}
	for _, member := range []string{"id", "subject", "nativeId"} {
		if !lintBounded256(value.(map[string]any)[member]) {
			return false
		}
	}
	for _, member := range []string{"semanticSymbol", "operation", "resource", "field", "value", "key"} {
		if !lintStateString(value.(map[string]any)[member]) {
			return false
		}
	}
	for _, member := range []string{"binding", "configuration", "ownership", "trace"} {
		if !lintStateBool(value.(map[string]any)[member]) {
			return false
		}
	}
	candidates, ok := value.(map[string]any)["candidates"].([]any)
	if !ok || len(candidates) > 32 {
		return false
	}
	for _, item := range candidates {
		if !lintRoute(item) {
			return false
		}
	}
	activation, ok := value.(map[string]any)["activation"].([]any)
	if !ok || len(activation) > 32 {
		return false
	}
	for _, item := range activation {
		if !lintStep(item) {
			return false
		}
	}
	return lintStringArray(value.(map[string]any)["locations"], 32, true) &&
		lintStringArray(value.(map[string]any)["guards"], 32, true)
}

// admitLintEvidence validates the closed `LintEvidence` subtree of a lint
// result (issue #106 review r2 #3): all twelve members required, const
// identity/version, closed nested objects — shallow or malformed evidence
// is refused instead of being emitted as success.
func admitLintEvidence(value Value) *Refusal {
	allowed := []string{
		"coverage", "identity", "inputManifestDigest", "limitations", "locations",
		"pins", "producer", "records", "schemaVersion", "scope", "sources", "target",
	}
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
	if !lintBounded256(object["target"]) ||
		!lintStringArray(object["scope"], 10_000, true) ||
		!lintProducer(object["producer"]) ||
		!lintPins(object["pins"]) ||
		!lintSha256(object["inputManifestDigest"]) {
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
		if !lintCoverage(coverage) {
			return refuse(stageDecode, "bounds")
		}
	}
	records, ok := object["records"].([]any)
	if !ok || len(records) > 10_000 {
		return refuse(stageDecode, "bounds")
	}
	for _, record := range records {
		if !lintRecord(record) {
			return refuse(stageDecode, "bounds")
		}
	}
	if !lintStringArray(object["limitations"], 10_000, true) {
		return refuse(stageDecode, "bounds")
	}
	return nil
}

func admitProgress(value Value) *Refusal {
	steps, ok := value.([]any)
	if !ok || len(steps) > int(generated.PROGRESS_STEPS_MAX) {
		return refuse(stageDecode, "bounds")
	}
	for _, step := range steps {
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

func admitError(value Value) *Refusal {
	allowed := []string{"class", "code", "detail", "message", "partial", "retryable"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	if !containsString(GeneratedEnums("errorClass"), GetString(value, "class")) {
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

func admitCapabilities(value Value) *Refusal {
	allowed := []string{"adapter", "capabilities", "constraints", "ir_versions", "operations", "profiles", "progress", "protocol_versions", "read_scopes", "targets", "transports", "write_scopes"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, "shape")
	}
	if refusal := admitIdentity(value.(map[string]any)["adapter"]); refusal != nil {
		return refusal
	}
	requiredList := func(member string, max int, check func(string) bool) *Refusal {
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
	if refusal := requiredList("operations", int(generated.OPERATIONS_MAX_LINT), func(op string) bool {
		return containsString(generated.LatestOperations, op)
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
	scopeText := func(scope string) bool { return scope != "" && len(scope) <= int(generated.PATH_MAX) }
	if refusal := optionalList("read_scopes", int(generated.MAX_SCOPES), scopeText); refusal != nil {
		return refusal
	}
	if refusal := optionalList("write_scopes", int(generated.MAX_SCOPES), scopeText); refusal != nil {
		return refusal
	}
	if refusal := optionalList("ir_versions", int(generated.MAX_IR_VERSIONS), isContract); refusal != nil {
		return refusal
	}
	if _, ok := GetBool(value, "progress"); !ok {
		return refuse(stageDecode, "shape")
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
			text, ok := support.(string)
			if !ok || !containsString(GeneratedEnums("supportState"), text) {
				return refuse(stageDecode, "shape")
			}
		}
	}
	if IsNull(value, "constraints") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "constraints") {
		constraints := value.(map[string]any)["constraints"]
		if !isClosedObject(constraints, []string{"max_entries", "max_writes"}) {
			return refuse(stageDecode, "shape")
		}
		for _, member := range []string{"max_entries", "max_writes"} {
			if IsNull(constraints, member) {
				return refuse(stageDecode, "explicit-null")
			}
			if Has(constraints, member) && !numberInRange(constraints.(map[string]any)[member], 0, generated.MAX_CONSTRAINT_VALUE) {
				return refuse(stageDecode, "bounds")
			}
		}
	}
	return nil
}

// AdmitResponseValue runs the structural admission pass on one response.
func AdmitResponseValue(value Value) *Refusal {
	allowed := []string{"capabilities", "error", "evidence", "operation", "progress", "protocol", "protocol_version", "request_id", "result", "status", "writes"}
	if !isClosedObject(value, allowed) {
		return refuse(stageDecode, unknownOrShape(value, allowed))
	}
	for _, member := range []string{"protocol", "protocol_version", "operation", "request_id", "status", "evidence"} {
		if !Has(value, member) {
			return refuse(stageDecode, "shape")
		}
		if IsNull(value, member) {
			return refuse(stageDecode, "explicit-null")
		}
	}
	if GetString(value, "protocol") != generated.WireToken {
		return refuse(stageDecode, "token")
	}
	if !IsContractVersion(GetString(value, "protocol_version")) {
		return refuse(stageDecode, "version")
	}
	if !containsString(generated.LatestOperations, GetString(value, "operation")) {
		return refuse(stageDecode, "operation")
	}
	if !IsRequestID(GetString(value, "request_id")) {
		return refuse(stageDecode, "request-id")
	}
	if !containsString(GeneratedEnums("responseStatus"), GetString(value, "status")) {
		return refuse(stageDecode, "shape")
	}
	if refusal := admitEvidence(value.(map[string]any)["evidence"]); refusal != nil {
		return refusal
	}
	if IsNull(value, "capabilities") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "capabilities") {
		if refusal := admitCapabilities(value.(map[string]any)["capabilities"]); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "result") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "result") {
		if refusal := admitResult(value.(map[string]any)["result"]); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "writes") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "writes") {
		writes := value.(map[string]any)["writes"]
		items, ok := writes.([]any)
		if !ok || len(items) > int(generated.MAX_PLAN_ENTRIES) {
			return refuse(stageDecode, "bounds")
		}
		for _, entry := range items {
			if refusal := admitWriteEntry(entry); refusal != nil {
				return refusal
			}
		}
	}
	if IsNull(value, "progress") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "progress") {
		if refusal := admitProgress(value.(map[string]any)["progress"]); refusal != nil {
			return refusal
		}
	}
	if IsNull(value, "error") {
		return refuse(stageDecode, "explicit-null")
	}
	if Has(value, "error") {
		if refusal := admitError(value.(map[string]any)["error"]); refusal != nil {
			return refusal
		}
	}
	return nil
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
			if !IsLogicalPath(file.(string)) {
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
				if file.(string) == path {
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
			} else if !Has(entry, "sha256") {
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
