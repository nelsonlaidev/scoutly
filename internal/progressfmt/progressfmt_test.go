package progressfmt

import (
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestDuration(t *testing.T) {
	t.Parallel()

	for _, test := range []struct {
		name     string
		duration time.Duration
		want     string
	}{
		{name: "negative", duration: -time.Second, want: "0:00"},
		{name: "subsecond", duration: 1500 * time.Millisecond, want: "0:01"},
		{name: "unbounded minutes", duration: 125 * time.Minute, want: "125:00"},
	} {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := Duration(test.duration); got != test.want {
				t.Errorf("Duration(%s) = %q, want %q", test.duration, got, test.want)
			}
		})
	}
}

func TestResource(t *testing.T) {
	t.Parallel()

	if got := Resource(audit.ResourceProgress{Checked: 3}); got != "3/--" {
		t.Errorf("Resource() without total = %q, want %q", got, "3/--")
	}
	total := 8
	if got := Resource(audit.ResourceProgress{Checked: 3, Total: &total}); got != "3/8" {
		t.Errorf("Resource() with total = %q, want %q", got, "3/8")
	}
}
