// Bridge to the generated vocabulary (issue #106): the runtime reads the
// generated constants through these helpers so regeneration flows
// through without touching hand-maintained codecs.
package runtime

import (
	"github.com/ichinya/lekalo-adapter-sdk-go/generated"
)

// GeneratedEnums returns the closed vocabulary of one enum name.
func GeneratedEnums(name string) []string {
	return generated.Enums[name]
}

var generatedBounds = struct {
	MaxRequestBytes       int64
	MaxPlanEntries        int
	MaxIrVersions         int
	MaxDeclaredCapabilities int
	MaxScopes             int
	MaxConstraintValue    int64
	LimitsTimeoutMin      int64
	LimitsTimeoutMax      int64
	LimitsOutputMin       int64
	LimitsOutputMax       int64
	ErrorCodeMax          int64
	ErrorMessageMax       int64
	ErrorDetailsMax       int64
	ErrorDetailMax        int64
	ProgressStepsMax      int64
	ProgressTextMax       int64
	ScanEntriesMax        int64
	FindingsMax           int64
	BindingsMax           int64
	ReferencesMax         int64
	ProtocolVersionsMax   int64
	OperationsMaxLint     int64
	TransportsMax         int64
	TargetsMax            int64
	ProfilesMax           int64
	PathMax               int64
}{
	MaxRequestBytes:         generated.MAX_REQUEST_BYTES,
	MaxPlanEntries:          int(generated.MAX_PLAN_ENTRIES),
	MaxIrVersions:           int(generated.MAX_IR_VERSIONS),
	MaxDeclaredCapabilities: int(generated.MAX_DECLARED_CAPABILITIES),
	MaxScopes:               int(generated.MAX_SCOPES),
	MaxConstraintValue:      generated.MAX_CONSTRAINT_VALUE,
	LimitsTimeoutMin:        generated.LIMITS_TIMEOUT_MIN,
	LimitsTimeoutMax:        generated.LIMITS_TIMEOUT_MAX,
	LimitsOutputMin:         generated.LIMITS_OUTPUT_MIN,
	LimitsOutputMax:         generated.LIMITS_OUTPUT_MAX,
	ErrorCodeMax:            generated.ERROR_CODE_MAX,
	ErrorMessageMax:         generated.ERROR_MESSAGE_MAX,
	ErrorDetailsMax:         generated.ERROR_DETAILS_MAX,
	ErrorDetailMax:          generated.ERROR_DETAIL_MAX,
	ProgressStepsMax:        generated.PROGRESS_STEPS_MAX,
	ProgressTextMax:         generated.PROGRESS_TEXT_MAX,
	ScanEntriesMax:          generated.SCAN_ENTRIES_MAX,
	FindingsMax:             generated.FINDINGS_MAX,
	BindingsMax:             generated.BINDINGS_MAX,
	ReferencesMax:           generated.REFERENCES_MAX,
	ProtocolVersionsMax:     generated.PROTOCOL_VERSIONS_MAX,
	OperationsMaxLint:       generated.OPERATIONS_MAX_LINT,
	TransportsMax:           generated.TRANSPORTS_MAX,
	TargetsMax:              generated.TARGETS_MAX,
	ProfilesMax:             generated.PROFILES_MAX,
	PathMax:                 generated.PATH_MAX,
}
