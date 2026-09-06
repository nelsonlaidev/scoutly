package audit

import (
	"errors"
	"math"
	"reflect"
	"testing"
	"time"
)

func TestValidationErrorFormatsFields(t *testing.T) {
	t.Parallel()

	err := (&ValidationError{Fields: []FieldError{
		{Field: "max_pages", Message: "must be greater than 0"},
		{Field: "timeout", Message: "must be greater than 0"},
	}}).Error()
	want := "invalid options:\n- max_pages must be greater than 0\n- timeout must be greater than 0"
	if err != want {
		t.Fatalf("ValidationError.Error() = %q, want %q", err, want)
	}
}

func TestDefaultOptions(t *testing.T) {
	want := Options{
		MaxDepth:            10,
		MaxPages:            500,
		KeepFragments:       false,
		IgnoreRedirects:     false,
		RateLimit:           0,
		RespectRobots:       true,
		Sitemaps:            true,
		Images:              true,
		MaxSitemapDocuments: 1000,
		Timeout:             30 * time.Second,
		MaxRedirects:        10,
		UserAgent:           "scoutly/dev",
		Concurrency:         20,
	}

	if got := DefaultOptions(); !reflect.DeepEqual(got, want) {
		t.Fatalf("DefaultOptions() = %#v, want %#v", got, want)
	}
}

func TestOptionsValidateAcceptsBoundaryValues(t *testing.T) {
	options := DefaultOptions()
	options.MaxDepth = 0
	options.MaxRedirects = 0
	options.RateLimit = 0

	if err := options.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
}

func TestOptionsValidateReportsEveryInvalidField(t *testing.T) {
	options := Options{
		MaxDepth:            -1,
		MaxPages:            0,
		RateLimit:           math.NaN(),
		MaxSitemapDocuments: 0,
		Timeout:             0,
		MaxRedirects:        -1,
		UserAgent:           "",
		Concurrency:         0,
	}

	err := options.Validate()
	var validationError *ValidationError
	if !errors.As(err, &validationError) {
		t.Fatalf("Validate() error = %T, want *ValidationError", err)
	}
	if got, want := len(validationError.Fields), 8; got != want {
		t.Fatalf("field errors = %d, want %d: %v", got, want, err)
	}
}

func TestOptionsValidateRejectsUnsafeUserAgent(t *testing.T) {
	options := DefaultOptions()
	options.UserAgent = "scoutly/test\r\nX-Injected: true"

	err := options.Validate()
	var validationError *ValidationError
	if !errors.As(err, &validationError) {
		t.Fatalf("Validate() error = %T, want *ValidationError", err)
	}
	if len(validationError.Fields) != 1 ||
		validationError.Fields[0].Field != "user_agent" {
		t.Fatalf("validation fields = %#v", validationError.Fields)
	}
}
