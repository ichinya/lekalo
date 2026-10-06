// Response builders and the validate-before-write boundary (issue #106).
package runtime

import (
	"sort"

	"github.com/ichinya/lekalo-adapter-sdk-go/generated"
)

// BuildError is a builder refusal: the closed reason token.
type BuildError struct {
	Reason string
}

func (e *BuildError) Error() string { return "sdk-build-error: " + e.Reason }

func buildRefuse(reason string) *BuildError {
	return &BuildError{Reason: reason}
}

// BuildAdapterIdentity validates one adapter identity block.
func BuildAdapterIdentity(id, version, digest string) (Value, *BuildError) {
	if !IsToken(id) || !IsContractVersion(version) || !IsSha256Digest(digest) {
		return nil, buildRefuse("identity")
	}
	return map[string]any{"digest": digest, "id": id, "version": version}, nil
}

func boundedUnique(values []string, max int) ([]string, *BuildError) {
	if len(values) > max {
		return nil, buildRefuse("bounds")
	}
	seen := map[string]bool{}
	sorted := append([]string(nil), values...)
	for _, value := range sorted {
		seen[value] = true
	}
	sort.Strings(sorted)
	if len(seen) != len(values) {
		return nil, buildRefuse("bounds")
	}
	return sorted, nil
}

func stringValues(items []string) []any {
	result := make([]any, 0, len(items))
	for _, item := range items {
		result = append(result, item)
	}
	return result
}

// BuildCapabilities builds one describe capability map. Declared scopes
// and capability states are honest claims the host polices.
func BuildCapabilities(
	adapter Value,
	protocolVersions, operations, transports, targets, profiles, readScopes, writeScopes, irVersions []string,
	progress bool,
	capabilities map[string]string,
	constraints map[string]int64,
) (Value, *BuildError) {
	versions, err := boundedUnique(protocolVersions, int(generatedBounds.ProtocolVersionsMax))
	if err != nil {
		return nil, err
	}
	operationsSorted, err := boundedUnique(operations, int(generatedBounds.OperationsMaxLint))
	if err != nil {
		return nil, err
	}
	transportsSorted, err := boundedUnique(transports, int(generatedBounds.TransportsMax))
	if err != nil {
		return nil, err
	}
	targetsSorted, err := boundedUnique(targets, int(generatedBounds.TargetsMax))
	if err != nil {
		return nil, err
	}
	profilesSorted, err := boundedUnique(profiles, int(generatedBounds.ProfilesMax))
	if err != nil {
		return nil, err
	}
	readSorted, err := boundedUnique(readScopes, int(generatedBounds.MaxScopes))
	if err != nil {
		return nil, err
	}
	writeSorted, err := boundedUnique(writeScopes, int(generatedBounds.MaxScopes))
	if err != nil {
		return nil, err
	}
	irSorted, err := boundedUnique(irVersions, int(generatedBounds.MaxIrVersions))
	if err != nil {
		return nil, err
	}
	members := map[string]any{
		"adapter":           adapter,
		"ir_versions":       stringValues(irSorted),
		"operations":        stringValues(operationsSorted),
		"profiles":          stringValues(profilesSorted),
		"progress":          progress,
		"protocol_versions": stringValues(versions),
		"read_scopes":       stringValues(readSorted),
		"targets":           stringValues(targetsSorted),
		"transports":        stringValues(transportsSorted),
		"write_scopes":      stringValues(writeSorted),
	}
	if len(capabilities) > 0 {
		if len(capabilities) > int(generatedBounds.MaxDeclaredCapabilities) {
			return nil, buildRefuse("capabilities")
		}
		sorted := map[string]any{}
		keys := sortedKeys(capabilities)
		for _, id := range keys {
			state := capabilities[id]
			if !IsCapabilityID(id) || !containsString(GeneratedEnums("supportState"), state) {
				return nil, buildRefuse("capabilities")
			}
			sorted[id] = state
		}
		members["capabilities"] = sorted
	}
	if len(constraints) > 0 {
		members["constraints"] = constraints
	}
	return members, nil
}

func sortedKeys(source map[string]string) []string {
	keys := make([]string, 0, len(source))
	for key := range source {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

// BuildSuccess builds and fully validates one success envelope.
func BuildSuccess(request, adapter Value, capabilities Value, payload Value, writes Value, progress Value, planID string) (Value, *BuildError) {
	evidence := map[string]any{"adapter": adapter}
	if planID != "" {
		evidence["plan_id"] = planID
	}
	members := envelopeBase(request, "ok")
	members["evidence"] = evidence
	if capabilities != nil && GetString(request, "operation") == "describe" {
		members["capabilities"] = capabilities
	}
	if payload != nil {
		members["result"] = payload
	}
	if writes != nil {
		members["writes"] = writes
	}
	if progress != nil {
		members["progress"] = progress
	}
	return finishEnvelope(members, request)
}

// BuildErrorEnvelope builds one closed in-envelope error envelope.
func BuildErrorEnvelope(request, adapter Value, class, code, message string, detail []string) (Value, *BuildError) {
	if !containsString(GeneratedEnums("errorClass"), class) {
		return nil, buildRefuse("error-class")
	}
	if code == "" || len([]rune(code)) > int(generatedBounds.ErrorCodeMax) {
		return nil, buildRefuse("error-code")
	}
	if message == "" || len([]rune(message)) > int(generatedBounds.ErrorMessageMax) {
		return nil, buildRefuse("error-message")
	}
	errorMembers := map[string]any{"class": class, "code": code, "message": message}
	if detail != nil {
		if len(detail) > int(generatedBounds.ErrorDetailsMax) {
			return nil, buildRefuse("error-detail")
		}
		for _, item := range detail {
			if len([]rune(item)) > int(generatedBounds.ErrorDetailMax) {
				return nil, buildRefuse("error-detail")
			}
		}
		errorMembers["detail"] = stringValues(detail)
	}
	members := envelopeBase(request, "error")
	members["evidence"] = map[string]any{"adapter": adapter}
	members["error"] = errorMembers
	return finishEnvelope(members, request)
}

func envelopeBase(request Value, status string) map[string]any {
	return map[string]any{
		"operation":        valueOf(request, "operation"),
		"protocol":         generated.WireToken,
		"protocol_version": valueOf(request, "protocol_version"),
		"request_id":       valueOf(request, "request_id"),
		"status":           status,
	}
}

func finishEnvelope(envelope Value, request Value) (Value, *BuildError) {
	if writes := valueOf(envelope, "writes"); writes != nil {
		sorted := SortWritePlan(writes)
		if sorted == nil {
			return nil, buildRefuse("writes")
		}
		envelope.(map[string]any)["writes"] = sorted
	}
	value, refusal := AdmitResponse(Canonical(envelope))
	if refusal != nil {
		return nil, buildRefuse(refusal.Reason)
	}
	if payloadRefusal := ValidateResponsePayload(request, value); payloadRefusal != nil {
		return nil, buildRefuse(payloadRefusal.Reason)
	}
	if plan := planIDOf(envelope); plan != "" && !IsPlanID(plan) {
		return nil, buildRefuse("plan-id")
	}
	return envelope, nil
}

// BuildFinding builds one wire Finding row.
func BuildFinding(code, path, detail string) (Value, *BuildError) {
	if code == "" || len([]rune(code)) > int(generatedBounds.ErrorCodeMax) {
		return nil, buildRefuse("finding-code")
	}
	member := map[string]any{"code": code, "path": path}
	if detail != "" {
		if len([]rune(detail)) > int(generatedBounds.ErrorDetailMax) {
			return nil, buildRefuse("finding-detail")
		}
		member["detail"] = detail
	}
	return member, nil
}

// BuildDiagnostic builds one bounded, registry-checked diagnostic record.
func BuildDiagnostic(codes [][2]string, code, message, rule string) (Value, *BuildError) {
	found := false
	for _, entry := range codes {
		if entry[1] == code {
			found = true
		}
	}
	if !found {
		return nil, buildRefuse("diagnostic-code")
	}
	if message == "" || len([]rune(message)) > int(generatedBounds.ErrorMessageMax) {
		return nil, buildRefuse("diagnostic-message")
	}
	if len([]rune(rule)) > int(generatedBounds.ErrorDetailMax) {
		return nil, buildRefuse("diagnostic-rule")
	}
	return map[string]any{"code": code, "message": message, "rule": rule}, nil
}

// ResponseBytes renders the exact stdout form of one envelope.
func ResponseBytes(envelope Value) string {
	return Canonical(envelope)
}
