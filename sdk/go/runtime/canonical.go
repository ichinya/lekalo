// Deterministic sorting, serialization, and result-building helpers
// (issue #106): canonical write plans, artifact-manifest candidates,
// and scenario result normalization.
package runtime

import "sort"

// SortWritePlan sorts one write plan by path and verifies the closed
// entry grammar; nil when the plan is malformed.
func SortWritePlan(entries Value) []Value {
	items, ok := entries.([]any)
	if !ok || len(items) > generatedBounds.MaxPlanEntries {
		return nil
	}
	sorted := make([]Value, 0, len(items))
	for _, entry := range items {
		if _, ok := entry.(map[string]any); !ok {
			return nil
		}
		path := GetString(entry, "path")
		if !IsLogicalPath(path) {
			return nil
		}
		action := GetString(entry, "action")
		if !containsString(GeneratedEnums("writeAction"), action) {
			return nil
		}
		digestValue, hasDigest := entry.(map[string]any)["sha256"]
		if action == "delete" {
			if hasDigest && digestValue != nil {
				return nil
			}
		} else {
			digest, ok := digestValue.(string)
			if !ok || !IsSha256Digest(digest) {
				return nil
			}
		}
		sorted = append(sorted, entry)
	}
	sort.SliceStable(sorted, func(i, j int) bool {
		return GetString(sorted[i], "path") < GetString(sorted[j], "path")
	})
	return sorted
}

// CanonicalPlanID computes the plan identifier over one sorted plan.
func CanonicalPlanID(sorted []Value) string {
	return PlanID(sorted)
}

// WriteEntryDigest digests the exact file content a write entry commits to.
func WriteEntryDigest(content string) string {
	return Sha256(content)
}

// BuildArtifactManifest builds one path-sorted, digest-addressed
// candidate; nil on any closed-rule violation.
func BuildArtifactManifest(entries [][2]string) Value {
	if len(entries) == 0 || len(entries) > generatedBounds.MaxPlanEntries {
		return nil
	}
	type row struct {
		path   string
		action string
		digest string
	}
	rows := make([]row, 0, len(entries))
	for _, entry := range entries {
		path, bytes := entry[0], entry[1]
		if !IsLogicalPath(path) || ProtectedHome(path) != "" {
			return nil
		}
		rows = append(rows, row{path, "create", Sha256(bytes)})
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].path < rows[j].path })
	rowsValue := make([]any, 0, len(rows))
	for _, entry := range rows {
		rowsValue = append(rowsValue, map[string]any{
			"action": entry.action,
			"path":   entry.path,
			"sha256": entry.digest,
		})
	}
	return map[string]any{
		"algorithm":     "sha256",
		"canonicalForm": Canonical(rowsValue),
		"entries":       rowsValue,
	}
}

var scenarioStatuses = []string{"pass", "fail", "unsupported", "infrastructure", "degraded"}

// NormalizeScenarioResult normalizes one scenario result: closed status
// set, sorted findings, bounded evidence.
func NormalizeScenarioResult(result Value) Value {
	if _, ok := result.(map[string]any); !ok {
		return nil
	}
	status := GetString(result, "status")
	if !containsString(scenarioStatuses, status) {
		return nil
	}
	findings := GetArray(result, "findings")
	if len(findings) > 10000 {
		return nil
	}
	type findingRow struct {
		path   string
		code   string
		detail string
		hasDetail bool
	}
	rows := make([]findingRow, 0, len(findings))
	for _, finding := range findings {
		if _, ok := finding.(map[string]any); !ok {
			return nil
		}
		code := GetString(finding, "code")
		if code == "" || len([]rune(code)) > 128 {
			return nil
		}
		path := GetString(finding, "path")
		if !IsLogicalPath(path) {
			return nil
		}
		row := findingRow{path: path, code: code}
		if Has(finding, "detail") && !IsNull(finding, "detail") {
			detail := GetString(finding, "detail")
			if len([]rune(detail)) > 128 {
				return nil
			}
			row.detail = detail
			row.hasDetail = true
		}
		rows = append(rows, row)
	}
	sort.SliceStable(rows, func(i, j int) bool {
		if rows[i].path != rows[j].path {
			return rows[i].path < rows[j].path
		}
		return rows[i].code < rows[j].code
	})
	findingsValue := make([]any, 0, len(rows))
	for _, row := range rows {
		member := map[string]any{"code": row.code, "path": row.path}
		if row.hasDetail {
			member["detail"] = row.detail
		}
		findingsValue = append(findingsValue, member)
	}
	evidenceDigest := Value(nil)
	if Has(result, "evidenceDigest") && !IsNull(result, "evidenceDigest") {
		digest := GetString(result, "evidenceDigest")
		if !IsSha256Digest(digest) {
			return nil
		}
		evidenceDigest = digest
	}
	return map[string]any{
		"evidenceDigest": evidenceDigest,
		"findings":       findingsValue,
		"status":         status,
	}
}

// WritePlanViolation returns the bounded violation token, or "".
func WritePlanViolation(path string, writeScopes []string) string {
	return PlanScopeViolation(path, writeScopes)
}

func containsString(values []string, candidate string) bool {
	for _, value := range values {
		if value == candidate {
			return true
		}
	}
	return false
}
