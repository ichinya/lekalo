// The per-exchange process lifetime (issue #106): read exactly one
// bounded request from stdin or the host-owned --lekalo-request-file
// coordinate, decode and validate it, dispatch one operation callback,
// and emit exactly one compact canonical response on stdout.
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
	Error [3]string
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
	if hasRequestFileFlag() && stdinPiped() {
		Error(tag, "transport: ambiguous input sources")
		return 1
	}
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
	_, _ = os.Stdout.WriteString(ResponseBytes(response))
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

func stdinPiped() bool {
	stat, err := os.Stdin.Stat()
	if err != nil {
		return false
	}
	return stat.Mode()&os.ModeCharDevice == 0
}

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
