package config

import (
	"errors"
	"fmt"
	"strings"

	"github.com/nelsonlaidev/scoutly/audit"
)

type validationError struct {
	Fields []audit.FieldError
}

func (e *validationError) Error() string {
	lines := make([]string, 0, len(e.Fields)+1)
	lines = append(lines, "invalid configuration:")

	for _, field := range e.Fields {
		lines = append(lines, fmt.Sprintf("- %s %s", field.Field, field.Message))
	}

	return strings.Join(lines, "\n")
}

func validate(cfg Config) error {
	result := &validationError{Fields: make([]audit.FieldError, 0)}

	if err := cfg.AuditOptions().Validate(); err != nil {
		var optionsError *audit.ValidationError
		if !errors.As(err, &optionsError) {
			return fmt.Errorf("validate audit options: %w", err)
		}
		result.Fields = append(result.Fields, optionsError.Fields...)
	}

	if cfg.Format != "text" && cfg.Format != "json" {
		result.Fields = append(result.Fields, audit.FieldError{
			Field:   "format",
			Message: "must be one of: text, json",
		})
	}
	if cfg.Progress != "auto" && cfg.Progress != "always" && cfg.Progress != "never" {
		result.Fields = append(result.Fields, audit.FieldError{
			Field:   "progress",
			Message: "must be one of: auto, always, never",
		})
	}

	if len(result.Fields) == 0 {
		return nil
	}

	return result
}
