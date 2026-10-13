package runtime

import "testing"

type panickingIdentityAdapter struct{}

func (panickingIdentityAdapter) Identity() Value { panic("private panic payload") }
func (panickingIdentityAdapter) Handle(string, Value) Output {
	panic("the handler must not run")
}

func TestServePanicBarrierDoesNotInvokeUserCodeAgain(t *testing.T) {
	if exit := Serve(panickingIdentityAdapter{}); exit != 1 {
		t.Fatalf("panic barrier returned %d, want 1", exit)
	}
}
