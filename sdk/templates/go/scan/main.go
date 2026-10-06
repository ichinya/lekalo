// The SDK scan example (issue #106): a capability-limited scan-only
// adapter on the Go adapter SDK. It declares describe and scan honestly
// and refuses everything else with a closed in-envelope error.
package main

import (
	"os"

	sdk "github.com/ichinya/lekalo-adapter-sdk-go/runtime"
)

type scanAdapter struct {
	identity sdk.Value
}

func (a *scanAdapter) Identity() sdk.Value {
	return a.identity
}

func (a *scanAdapter) Handle(operation string, _ sdk.Value) sdk.Output {
	switch operation {
	case "describe":
		capabilities, err := sdk.BuildCapabilities(
			a.identity,
			[]string{"0.3.2"},
			[]string{"describe", "scan"},
			[]string{"file", "stdin"},
			[]string{"sdk-scan-example"},
			nil,
			[]string{"src/**"},
			nil,
			nil,
			false,
			map[string]string{"scan.symbols": "full"},
			nil,
		)
		if err != nil {
			return refused()
		}
		return sdk.Output{Capabilities: capabilities}
	case "scan":
		return sdk.Output{Payload: map[string]any{
			"entries": []any{
				map[string]any{"kind": "source", "path": "src/main.ts"},
				map[string]any{"kind": "source", "path": "src/util.ts"},
			},
			"truncated": false,
		}}
	default:
		return sdk.Output{Error: [3]string{"unsupported", "operation-unsupported", "the scan example implements describe and scan only"}, HasError: true}
	}
}

func refused() sdk.Output {
	return sdk.Output{Error: [3]string{"infrastructure", "sdk-build-refused", "the capability builder refused the declaration"}, HasError: true}
}

func main() {
	identity, err := sdk.BuildAdapterIdentity("sdk-scan-example", "0.1.0", sdk.Sha256("lekalo adapter-sdk scan example"))
	if err != nil {
		os.Exit(1)
	}
	os.Exit(sdk.Serve(&scanAdapter{identity: identity}))
}
