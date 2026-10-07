// The one input-source contract (issue #106), implemented identically in
// all four runtimes: the host-owned --lekalo-request-file coordinate wins
// whenever present. A piped stdin alongside it is the ordinary host shape
// (the host always spawns adapters with piped stdio), so pipe-ness is
// never ambiguous. Empty stdin without a file flag is the only
// missing-input refusal.
package runtime

import (
	"os"
	"time"
)

// Output carries the builder fragments of one operation handler.
type Output struct {
	Capabilities Value
	Payload      Value
	PlanID       string
	Progress     Value
	Writes       Value
	// Error is a closed in-envelope error the handler chose deliberately
	// as (class, code, message).
	Error    [3]string
	HasError bool
}

// Adapter is the adapter contract: identity plus per-operation handlers.
type Adapter interface {
	Identity() Value
	Handle(operation string, request Value) Output
}

// DeadlineToken exposes the accepted request limit to callbacks. Hard
// cancellation and descendant reaping stay host responsibilities.
type DeadlineToken struct {
	DeadlineMs int64
}

// DeadlineTokenFromRequest builds the token from the request limits.
func DeadlineTokenFromRequest(request Value) DeadlineToken {
	deadline := int64(0)
	if limits := valueOf(request, "limits"); limits != nil {
		if timeout, ok := GetNumber(limits, "timeout_ms"); ok {
			deadline = timeout
		}
	}
	return DeadlineToken{DeadlineMs: deadline}
}

// Serve runs exactly one exchange and returns the process exit code.
func Serve(adapter Adapter) int {
	identity := adapter.Identity()
	tag := GetString(identity, "id")
	raw, err := readRequestText()
	if err != nil {
		Error(tag, "transport: request input unreadable")
		return 1
	}
	request, refusal := AdmitRequest(raw)
	if refusal != nil {
		Error(tag, "request refused: "+refusal.Reason)
		return 1
	}
	operation := GetString(request, "operation")
	output := adapter.Handle(operation, request)
	var response Value
	var buildFailure *BuildError
	if output.HasError {
		response, buildFailure = BuildErrorEnvelope(request, identity, output.Error[0], output.Error[1], output.Error[2], nil)
	} else if output.Capabilities == nil && operation == "describe" {
		buildFailure = &BuildError{Reason: "member"}
	} else {
		response, buildFailure = BuildSuccess(request, identity, output.Capabilities, output.Payload, output.Writes, output.Progress, output.PlanID)
	}
	if buildFailure != nil {
		Error(tag, "response refused: "+buildFailure.Reason)
		response, buildFailure = BuildErrorEnvelope(request, identity, "infrastructure", "sdk-build-refused", "the adapter refused to emit an invalid response", nil)
		if buildFailure != nil {
			return 1
		}
	}
	// Bounded emission (issue #106): the completed envelope is admitted
	// (builders already validate the full closed contract) and serialized
	// into a bounded buffer before a single stdout byte is written. The
	// ceiling is the accepted default OR the request's declared
	// limits.max_output_bytes, whichever is tighter. A response over the
	// ceiling refuses here — never a truncated ok written to the stream.
	bytes := ResponseBytes(response)
	ceiling := generatedBounds.DefaultMaxOutputBytes
	if limits, ok := valueOf(request, "limits").(map[string]any); ok {
		if declared, ok := limits["max_output_bytes"].(float64); ok && declared >= 1 && int64(declared) < ceiling {
			ceiling = int64(declared)
		}
	}
	if int64(len(bytes)) > ceiling {
		Error(tag, "response refused: output-limit")
		return 1
	}
	_, _ = os.Stdout.WriteString(bytes)
	return 0
}

func hasRequestFileFlag() bool {
	for _, arg := range os.Args[1:] {
		if arg == "--lekalo-request-file" {
			return true
		}
	}
	return false
}

// stdinPiped reports whether stdin carries a live pipe (or any non-tty
// stream). It is diagnostics-only since issue #106: pipe-ness never
// refuses an exchange, because the host always spawns adapters with a
// piped stdin — including on file-transport exchanges.

func readRequestText() (string, error) {
	if hasRequestFileFlag() {
		for index, arg := range os.Args[1:] {
			if arg == "--lekalo-request-file" {
				if index+2 < len(os.Args) {
					bytes, err := os.ReadFile(os.Args[index+2])
					return string(bytes), err
				}
			}
		}
	}
	const chunk = 65536
	buffer := make([]byte, 0, chunk)
	scratch := make([]byte, chunk)
	for {
		read, err := os.Stdin.Read(scratch)
		buffer = append(buffer, scratch[:read]...)
		if err != nil || read == 0 || len(buffer) > int(generatedBounds.MaxRequestBytes) {
			break
		}
	}
	return string(buffer), nil
}

var _ = time.Now
