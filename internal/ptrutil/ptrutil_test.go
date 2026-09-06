package ptrutil

import "testing"

func TestClone(t *testing.T) {
	t.Parallel()

	value := new("scoutly")
	cloned := Clone(value)
	if cloned == nil || *cloned != *value {
		t.Fatalf("Clone() = %v, want %q", cloned, *value)
	}
	if cloned == value {
		t.Fatal("Clone() returned the input pointer")
	}
	if Clone[string](nil) != nil {
		t.Fatal("Clone(nil) returned a non-nil pointer")
	}
}
