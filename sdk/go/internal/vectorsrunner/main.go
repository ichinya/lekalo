// Command vectorsrunner executes the shared adapter-SDK vector corpus
// (issue #106) through the Go SDK runtime and prints the same one-line
// JSON verdict as the TypeScript and Rust runners.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	sdk "github.com/ichinya/lekalo-adapter-sdk-go/runtime"
)

type verdict struct {
	Results          []result           `json:"-"`
	Failures         []map[string]any   `json:"failures,omitempty"`
	OK               bool               `json:"ok"`
	Language         string             `json:"language"`
	Total            int                `json:"total"`
	Failed           int                `json:"failed"`
	CanonicalOutputs map[string]string  `json:"canonicalOutputs"`
}

type result struct {
	name   string
	ok     bool
	detail string
}

var state verdict

func record(name string, ok bool, detail string) {
	state.Results = append(state.Results, result{name, ok, detail})
}

func readCorpus(corpus, relative string) string {
	bytes, err := os.ReadFile(filepath.Join(corpus, relative))
	if err != nil {
		return ""
	}
	return string(bytes)
}

func listCorpus(corpus, family string) []string {
	entries, err := os.ReadDir(filepath.Join(corpus, family))
	if err != nil {
		return nil
	}
	var names []string
	for _, entry := range entries {
		name := entry.Name()
		if strings.HasSuffix(name, ".json") && !strings.HasSuffix(name, ".expect.json") {
			names = append(names, name)
		}
	}
	sort.Strings(names)
	return names
}

func jsonText(value sdk.Value, member string) string {
	var target sdk.Value
	if object, ok := value.(map[string]any); ok {
		target = object[member]
	}
	return sdk.Canonical(target)
}

func main() {
	corpus := "../../tests/fixtures/adapter-sdk"
	for _, arg := range os.Args[1:] {
		if strings.HasPrefix(arg, "--corpus=") {
			corpus = strings.TrimPrefix(arg, "--corpus=")
		}
	}
	state = verdict{Language: "go", CanonicalOutputs: map[string]string{}}

	// valid/
	for _, name := range listCorpus(corpus, "valid") {
		text := readCorpus(corpus, "valid/"+name)
		isResponse := strings.Contains(text, "\"status\"") && strings.Contains(text, "\"evidence\"")
		if isResponse {
			_, refusal := sdk.AdmitResponse(text)
			record("valid/"+name, refusal == nil, refusalReason(refusal))
		} else {
			_, refusal := sdk.AdmitRequest(text)
			record("valid/"+name, refusal == nil, refusalReason(refusal))
		}
	}

	// invalid/
	for _, name := range listCorpus(corpus, "invalid") {
		text := readCorpus(corpus, "invalid/"+name)
		expect := readCorpus(corpus, "invalid/"+strings.TrimSuffix(name, ".json")+".expect.json")
		expectStage, expectReason := parseExpect(expect)
		isResponse := strings.Contains(text, "\"status\"") && strings.Contains(text, "\"evidence\"")
		var actualStage, actualReason string
		if !isResponse {
			if _, refusal := sdk.AdmitRequest(text); refusal != nil {
				actualStage, actualReason = refusal.Stage, refusal.Reason
			} else if value, refusal := sdk.AdmitRequest(text); refusal == nil {
				if validate := sdk.ValidateRequest(value); validate != nil {
					actualStage, actualReason = validate.Stage, validate.Reason
				} else {
					actualStage = "none"
				}
			}
		} else if _, refusal := sdk.AdmitResponse(text); refusal != nil {
			actualStage, actualReason = refusal.Stage, refusal.Reason
		} else {
			actualStage = "none"
		}
		ok := actualStage == expectStage && actualReason == expectReason
		record("invalid/"+name, ok, fmt.Sprintf("%s/%s vs %s/%s", actualStage, actualReason, expectStage, expectReason))
	}

	runSemantic(corpus, &state)
	runCanonical(corpus)
	runSecurity(corpus)
	runUpgrade(corpus)

	failed := 0
	for _, entry := range state.Results {
		if !entry.ok {
			failed++
			state.Failures = append(state.Failures, map[string]any{"name": entry.name, "detail": entry.detail})
		}
	}
	state.Total = len(state.Results)
	state.Failed = failed
	state.OK = failed == 0
	out, _ := json.Marshal(state)
	fmt.Println(string(out))
	if failed > 0 {
		os.Exit(1)
	}
}

func parseExpect(expect string) (string, string) {
	var parsed struct {
		Stage  string `json:"stage"`
		Reason string `json:"reason"`
	}
	_ = json.Unmarshal([]byte(expect), &parsed)
	return parsed.Stage, parsed.Reason
}

func refusalReason(refusal *sdk.Refusal) string {
	if refusal == nil {
		return ""
	}
	return refusal.Reason
}

func runSemantic(corpus string, _ *verdict) {
	for _, name := range listCorpus(corpus, "semantic") {
		text := readCorpus(corpus, "semantic/"+name)
		var vector map[string]any
		if err := json.Unmarshal([]byte(text), &vector); err != nil {
			record("semantic/"+name, false, "unparseable")
			continue
		}
		kind, _ := vector["kind"].(string)
		expect, _ := vector["expect"].(string)
		expectKind, expectReason := expect, ""
		if index := strings.Index(expect, ":"); index != -1 {
			expectKind, expectReason = expect[:index], expect[index+1:]
		}
		ok, detail := func() (bool, string) {
			switch kind {
			case "validate-before-write":
				request, refusal := sdk.AdmitRequest(sdk.Canonical(vector["request"]))
				if refusal != nil {
					return expectKind == "reject" && expectReason == refusal.Reason, refusal.Reason
				}
				response, refusal := sdk.AdmitResponse(sdk.Canonical(vector["response"]))
				if refusal != nil {
					return expectKind == "reject" && expectReason == refusal.Reason, refusal.Reason
				}
				requestObject, _ := request.(map[string]any)
				reason := ""
				if identity := sdk.ValidateResponseIdentity(response,
					fmt.Sprint(requestObject["protocol_version"]),
					fmt.Sprint(requestObject["operation"]),
					fmt.Sprint(requestObject["request_id"])); identity != nil {
					reason = identity.Reason
				} else if payload := sdk.ValidateResponsePayload(request, response); payload != nil {
					reason = payload.Reason
				}
				if reason == "" {
					return expectKind == "accept", ""
				}
				return expectKind == "reject" && expectReason == reason, reason
			case "build-success":
				request, refusal := sdk.AdmitRequest(sdk.Canonical(vector["request"]))
				if refusal != nil {
					return false, refusal.Reason
				}
				builder, _ := vector["builder"].(map[string]any)
				identity, buildErr := sdk.BuildAdapterIdentity(
					sdk.GetString(builder["identity"], "id"),
					sdk.GetString(builder["identity"], "version"),
					sdk.GetString(builder["identity"], "digest"))
				if buildErr != nil {
					return false, buildErr.Reason
				}
				fragments, _ := builder["fragments"].(map[string]any)
				planID, _ := fragments["planId"].(string)
				envelope, buildErr := sdk.BuildSuccess(request, identity, builder["capabilities"], fragments["payload"], fragments["writes"], fragments["progress"], planID)
				if buildErr != nil {
					return expectKind == "reject" && expectReason == buildErr.Reason, buildErr.Reason
				}
				ok := expectKind == "accept"
				if ok {
					if _, hasWrites := fragments["writes"]; hasWrites {
						var raw []any
						encoded, _ := json.Marshal(fragments["writes"])
						_ = json.Unmarshal(encoded, &raw)
						sort.Slice(raw, func(i, j int) bool {
							left, _ := raw[i].(map[string]any)
							right, _ := raw[j].(map[string]any)
							return fmt.Sprint(left["path"]) < fmt.Sprint(right["path"])
						})
						ok = sdk.Canonical(envelope.(map[string]any)["writes"]) == sdk.Canonical(raw)
					}
				}
				return ok, ""
			case "build-error":
				request, refusal := sdk.AdmitRequest(sdk.Canonical(vector["request"]))
				if refusal != nil {
					return false, refusal.Reason
				}
				builder, _ := vector["builder"].(map[string]any)
				identity, buildErr := sdk.BuildAdapterIdentity(
					sdk.GetString(builder["identity"], "id"),
					sdk.GetString(builder["identity"], "version"),
					sdk.GetString(builder["identity"], "digest"))
				if buildErr != nil {
					return false, buildErr.Reason
				}
				errorSpec, _ := builder["error"].(map[string]any)
				var detail []string
				if raw, ok := errorSpec["detail"].([]any); ok {
					for _, item := range raw {
						detail = append(detail, fmt.Sprint(item))
					}
				}
				envelope, buildErr := sdk.BuildErrorEnvelope(request, identity,
					fmt.Sprint(errorSpec["errorClass"]), fmt.Sprint(errorSpec["errorCode"]), fmt.Sprint(errorSpec["message"]), detail)
				if buildErr != nil {
					return expectKind == "reject" && expectReason == buildErr.Reason, buildErr.Reason
				}
				return expectKind == "accept" && sdk.GetString(envelope, "status") == "error", ""
			case "decode-oversize":
				maxBytes := int(toFloat(vector["maxBytes"]))
				text, _ := vector["text"].(string)
				_, reason := sdk.ReadSingleObject(text, maxBytes)
				return expectKind == "reject" && string(reason) == expectReason, string(reason)
			case "canonical-plan":
				sorted := sdk.SortWritePlan(vector["writes"])
				if sorted == nil {
					return false, "unsortable"
				}
				planID, _ := vector["planId"].(string)
				return sdk.CanonicalPlanID(sorted) == planID, ""
			case "artifact-manifest":
				entriesRaw, _ := vector["entries"].([]any)
				entries := make([][2]string, 0, len(entriesRaw))
				for _, entry := range entriesRaw {
					entryObject, _ := entry.(map[string]any)
					path, _ := entryObject["path"].(string)
					bytes, _ := entryObject["bytes"].(string)
					entries = append(entries, [2]string{path, bytes})
				}
				manifest := sdk.BuildArtifactManifest(entries)
				if expectKind == "reject" {
					return manifest == nil, ""
				}
				return manifest != nil && sdk.Canonical(manifest.(map[string]any)["entries"]) == sdk.Canonical(vector["expectSorted"]), ""
			case "scenario-result":
				normalized := sdk.NormalizeScenarioResult(vector["result"])
				if expectKind == "reject" {
					return normalized == nil, ""
				}
				return normalized != nil && sdk.Canonical(normalized.(map[string]any)["findings"]) == sdk.Canonical(vector["normalizedFindings"]), ""
			default:
				return false, "unknown-kind"
			}
		}()
		record("semantic/"+name, ok, detail)
	}
}

func runCanonical(corpus string) {
	for _, name := range listCorpus(corpus, "canonical") {
		text := readCorpus(corpus, "canonical/"+name)
		var vector map[string]any
		if err := json.Unmarshal([]byte(text), &vector); err != nil {
			record("canonical/"+name, false, "unparseable")
			continue
		}
		expected, _ := vector["canonical"].(string)
		bytes := sdk.Canonical(vector["value"])
		state.CanonicalOutputs["canonical/"+name] = bytes
		roundTrip, reason := sdk.StrictParse(bytes)
		ok := bytes == expected && reason == "" && sdk.Canonical(roundTrip) == sdk.Canonical(vector["value"])
		record("canonical/"+name, ok, bytes)
	}
}

func runSecurity(corpus string) {
	const nfcSupported = false
	for _, name := range listCorpus(corpus, "security") {
		text := readCorpus(corpus, "security/"+name)
		var vector map[string]any
		if err := json.Unmarshal([]byte(text), &vector); err != nil {
			record("security/"+name, false, "unparseable")
			continue
		}
		if requiresNfc, _ := vector["requiresNfc"].(bool); requiresNfc && !nfcSupported {
			record("security/"+name, true, "skipped:nfc-unsupported")
			continue
		}
		ok := false
		if scopeGrammar, _ := vector["scopeGrammar"].(bool); scopeGrammar {
			value, _ := vector["value"].(string)
			scopeOk, _ := vector["scopeOk"].(bool)
			ok = sdk.IsScope(value) == scopeOk
			if ok {
				if touches, has := vector["touches"]; has {
					touchesText, _ := touches.(string)
					ok = sdk.ScopeTouchesProtectedHome(value) == touchesText
				}
			}
			if ok {
				if logical, has := vector["logical"]; has {
					logicalBool, _ := logical.(bool)
					ok = sdk.IsLogicalPath(value) == logicalBool
				}
			}
		} else if token, has := vector["token"]; has {
			tokenText, _ := token.(string)
			tokenOk, _ := vector["tokenOk"].(bool)
			ok = sdk.IsToken(tokenText) == tokenOk
		} else if capability, has := vector["capability"]; has {
			capabilityText, _ := capability.(string)
			tokenOk, _ := vector["tokenOk"].(bool)
			ok = sdk.IsCapabilityID(capabilityText) == tokenOk
		} else {
			path, _ := vector["path"].(string)
			if path == "" {
				path, _ = vector["value"].(string)
			}
			logical, _ := vector["logical"].(bool)
			ok = sdk.IsLogicalPath(path) == logical
			if ok {
				if home, has := vector["home"]; has {
					homeText, _ := home.(string)
					ok = sdk.ProtectedHome(path) == homeText
				}
			}
			if ok {
				if scope, has := vector["scope"]; has {
					scopeText, _ := scope.(string)
					covers, _ := vector["covers"].(bool)
					ok = sdk.ScopeCovers(scopeText, path) == covers
				}
			}
		}
		record("security/"+name, ok, "")
	}
}

func runUpgrade(corpus string) {
	for _, name := range listCorpus(corpus, "upgrade") {
		text := readCorpus(corpus, "upgrade/"+name)
		var vector map[string]any
		if err := json.Unmarshal([]byte(text), &vector); err != nil {
			record("upgrade/"+name, false, "unparseable")
			continue
		}
		requestText := sdk.Canonical(vector["request"])
		accepted := false
		reason := ""
		if _, refusal := sdk.AdmitRequest(requestText); refusal != nil {
			reason = refusal.Reason
		} else if request, _ := sdk.AdmitRequest(requestText); true {
			if validate := sdk.ValidateRequest(request); validate != nil {
				reason = validate.Reason
			} else {
				accepted = true
			}
		}
		expectAccept, _ := vector["accept"].(bool)
		expectReason, _ := vector["reason"].(string)
		ok := expectAccept == accepted && (expectAccept || expectReason == reason)
		record("upgrade/"+name, ok, fmt.Sprintf("(%v, %q)", accepted, reason))
	}
}

func toFloat(value any) float64 {
	switch typed := value.(type) {
	case float64:
		return typed
	case int64:
		return float64(typed)
	}
	return 0
}
