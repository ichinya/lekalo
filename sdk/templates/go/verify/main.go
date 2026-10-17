// The SDK verify example (issue #106): a verification adapter on the Go
// adapter SDK. It declares describe and verify honestly, reports
// findings through the typed builder, and never widens its declared
// read surface. Read-only; no writes; no network; no shell.
package main

import (
	"os"

	sdk "github.com/ichinya/lekalo-adapter-sdk-go/runtime"
)

type verifyAdapter struct {
	identity sdk.Value
}

func (a *verifyAdapter) Identity() sdk.Value {
	return a.identity
}

func (a *verifyAdapter) Handle(operation string, _ sdk.Value) sdk.Output {
	switch operation {
	case "describe":
		capabilities, err := sdk.BuildCapabilities(
			a.identity,
			[]string{"0.3.2"},
			[]string{"describe", "verify"},
			[]string{"file", "stdin"},
			[]string{"sdk-verify-example"},
			nil,
			[]string{".lekalo/ir/**"},
			nil,
			[]string{"0.2.16"},
			false,
			map[string]string{"verify.scenarios": "full"},
			nil,
		)
		if err != nil {
			return refused()
		}
		return sdk.Output{Capabilities: capabilities}
	case "verify":
		finding, err := sdk.BuildFinding("scenario-missing", "src/main.ts", "no scenarios declared")
		if err != nil {
			return refused()
		}
		return sdk.Output{Payload: map[string]any{"findings": []any{finding}, "ok": true}}
	default:
		return sdk.Output{Error: [3]string{"unsupported", "operation-unsupported", "the verify example implements describe and verify only"}, HasError: true}
	}
}

func refused() sdk.Output {
	return sdk.Output{Error: [3]string{"infrastructure", "sdk-build-refused", "the capability builder refused the declaration"}, HasError: true}
}

func main() {
	identity, err := sdk.BuildAdapterIdentity("sdk-verify-example", "0.1.0", sdk.Sha256("lekalo adapter-sdk verify example"))
	if err != nil {
		os.Exit(1)
	}
	os.Exit(sdk.Serve(&verifyAdapter{identity: identity}))
}
