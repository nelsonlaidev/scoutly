package config

import (
	"encoding/json"
	"errors"
	"io"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"testing/iotest"

	"go.yaml.in/yaml/v3"
)

func TestDecodeFileSelectsDecoderByExtension(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.JSON", content: `{"max_depth": 3}`},
		{name: "config.YAML", content: "max_depth: 3\n"},
		{name: "config.YML", content: "max_depth: 3\n"},
		{name: "config.TOML", content: "max_depth = 3\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), test.name)
			writeFile(t, path, test.content)

			overrides, err := decodeFile(path)
			if err != nil {
				t.Fatalf("decodeFile() error = %v", err)
			}
			if overrides.MaxDepth == nil || *overrides.MaxDepth != 3 {
				t.Fatalf("MaxDepth = %v, want 3", overrides.MaxDepth)
			}
		})
	}
}

func TestDecodeFileReportsContext(t *testing.T) {
	t.Run("open", func(t *testing.T) {
		path := filepath.Join(t.TempDir(), "missing.json")

		if _, err := decodeFile(path); err == nil || !strings.Contains(err.Error(), "open config") {
			t.Fatalf("decodeFile() error = %v, want open config context", err)
		}
	})

	t.Run("unsupported format", func(t *testing.T) {
		path := filepath.Join(t.TempDir(), "config.txt")
		writeFile(t, path, "max_depth = 3\n")

		if _, err := decodeFile(path); err == nil || !strings.Contains(err.Error(), `unsupported config format ".txt"`) {
			t.Fatalf("decodeFile() error = %v, want unsupported format", err)
		}
	})

	t.Run("decode", func(t *testing.T) {
		path := filepath.Join(t.TempDir(), "config.json")
		writeFile(t, path, `{`)

		if _, err := decodeFile(path); err == nil || !strings.Contains(err.Error(), "decode config") {
			t.Fatalf("decodeFile() error = %v, want decode config context", err)
		}
	})
}

func TestDecodersPreserveExplicitZeroPresence(t *testing.T) {
	tests := []struct {
		name   string
		decode func() (Overrides, error)
	}{
		{name: "JSON", decode: func() (Overrides, error) {
			return decodeJSON(strings.NewReader(`{"rate_limit": 0}`))
		}},
		{name: "YAML", decode: func() (Overrides, error) {
			return decodeYAML(strings.NewReader("rate_limit: 0\n"))
		}},
		{name: "TOML", decode: func() (Overrides, error) {
			return decodeTOML(strings.NewReader("rate_limit = 0\n"))
		}},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			overrides, err := test.decode()
			if err != nil {
				t.Fatalf("decode error = %v", err)
			}
			if overrides.RateLimit == nil || *overrides.RateLimit != 0 {
				t.Fatalf("RateLimit = %v, want explicit 0", overrides.RateLimit)
			}
		})
	}
}

func TestDecodersDecodeSupportedScalarTypes(t *testing.T) {
	tests := []struct {
		name    string
		content string
		decode  func(io.Reader) (Overrides, error)
	}{
		{
			name:    "JSON",
			content: `{"max_depth": 3, "rate_limit": 1.5, "respect_robots": false, "user_agent": "test"}`,
			decode:  decodeJSON,
		},
		{
			name:    "YAML",
			content: "max_depth: 3\nrate_limit: 1.5\nrespect_robots: false\nuser_agent: test\n",
			decode:  decodeYAML,
		},
		{
			name:    "TOML",
			content: "max_depth = 3\nrate_limit = 1.5\nrespect_robots = false\nuser_agent = \"test\"\n",
			decode:  decodeTOML,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			overrides, err := test.decode(strings.NewReader(test.content))
			if err != nil {
				t.Fatalf("decode error = %v", err)
			}
			if overrides.MaxDepth == nil || *overrides.MaxDepth != 3 {
				t.Fatalf("MaxDepth = %v, want 3", overrides.MaxDepth)
			}
			if overrides.RateLimit == nil || *overrides.RateLimit != 1.5 {
				t.Fatalf("RateLimit = %v, want 1.5", overrides.RateLimit)
			}
			if overrides.RespectRobots == nil || *overrides.RespectRobots {
				t.Fatalf("RespectRobots = %v, want false", overrides.RespectRobots)
			}
			if overrides.UserAgent == nil || *overrides.UserAgent != "test" {
				t.Fatalf("UserAgent = %v, want test", overrides.UserAgent)
			}
		})
	}
}

func TestDecodeJSONRejectsInvalidDocuments(t *testing.T) {
	tests := []struct {
		name          string
		content       string
		want          string
		wantTruncated bool
	}{
		{name: "empty", content: "", want: "EOF"},
		{name: "non-object", content: "[]", want: "configuration must be an object"},
		{name: "unknown field", content: `{"max_dept": 3}`, want: `unknown field "max_dept"`},
		{name: "non-canonical field", content: `{"Max_Depth": 3}`, want: `unknown field "Max_Depth"`},
		{name: "duplicate field", content: `{"max_depth": 1, "max_depth": 2}`, want: `duplicate field "max_depth"`},
		{name: "null field", content: `{"max_depth": null}`, want: `field "max_depth" cannot be null`},
		{name: "invalid field type", content: `{"max_depth": "three"}`, want: "cannot unmarshal"},
		{name: "multiple documents", content: `{"max_depth": 1} {"max_depth": 2}`, want: "multiple JSON documents"},
		{name: "malformed document", content: `{"max_depth":`, wantTruncated: true},
		{name: "malformed key", content: `{"max_depth": 1,`, wantTruncated: true},
		{name: "missing closing delimiter", content: `{"max_depth": 1`, wantTruncated: true},
		{name: "invalid trailing data", content: `{"max_depth": 1} invalid`, want: "invalid character"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, err := decodeJSON(strings.NewReader(test.content))
			if err == nil {
				t.Fatal("decodeJSON() error = nil")
			}
			if test.wantTruncated {
				var syntaxError *json.SyntaxError
				if !errors.Is(err, io.ErrUnexpectedEOF) && !errors.As(err, &syntaxError) {
					t.Fatalf("decodeJSON() error = %v, want truncated JSON error", err)
				}
				return
			}
			if !strings.Contains(err.Error(), test.want) {
				t.Fatalf("decodeJSON() error = %v, want containing %q", err, test.want)
			}
		})
	}
}

func TestDecodeYAMLRejectsInvalidDocuments(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{name: "empty", content: "", want: "EOF"},
		{name: "non-mapping", content: "- max_depth\n", want: "configuration must be a mapping"},
		{name: "null document", content: "null\n", want: "configuration must be a mapping"},
		{name: "non-string key", content: "1: value\n", want: "configuration keys must be strings"},
		{name: "unknown field", content: "max_dept: 3\n", want: `unknown field "max_dept"`},
		{name: "duplicate field", content: "max_depth: 1\nmax_depth: 2\n", want: `duplicate field "max_depth"`},
		{name: "null field", content: "max_depth: null\n", want: `field "max_depth" cannot be null`},
		{name: "fractional integer", content: "max_depth: 1.5\n", want: `field "max_depth" has invalid type !!float`},
		{name: "legacy boolean", content: "respect_robots: yes\n", want: `field "respect_robots" has invalid type !!str`},
		{name: "boolean float", content: "rate_limit: true\n", want: `field "rate_limit" has invalid type !!bool`},
		{name: "numeric string", content: "user_agent: 1\n", want: `field "user_agent" has invalid type !!int`},
		{name: "multiple documents", content: "max_depth: 1\n---\nmax_depth: 2\n", want: "multiple YAML documents"},
		{name: "malformed document", content: "max_depth: [\n", want: "did not find expected node content"},
		{name: "malformed second document", content: "max_depth: 1\n---\n[", want: "did not find expected node content"},
		{name: "integer overflow", content: "max_depth: !!int 999999999999999999999999999999999999\n", want: "cannot decode"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, err := decodeYAML(strings.NewReader(test.content))
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("decodeYAML() error = %v, want containing %q", err, test.want)
			}
		})
	}
}

func TestYAMLValidationRejectsMalformedNodesAndUnknownKinds(t *testing.T) {
	t.Parallel()

	if err := validateYAMLDocument(&yaml.Node{Kind: yaml.DocumentNode}); err == nil {
		t.Fatal("validateYAMLDocument() accepted an empty document")
	}
	if matchesYAMLKind(&yaml.Node{Kind: yaml.ScalarNode}, reflect.Invalid) {
		t.Fatal("matchesYAMLKind() accepted an unsupported reflect kind")
	}
}

func TestDecodeTOMLRejectsInvalidDocuments(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "unknown field", content: "max_dept = 3\n"},
		{name: "invalid field type", content: `max_depth = "three"`},
		{name: "duplicate field", content: "max_depth = 1\nmax_depth = 2\n"},
		{name: "malformed document", content: "max_depth = [\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, err := decodeTOML(strings.NewReader(test.content)); err == nil {
				t.Fatal("decodeTOML() error = nil")
			}
		})
	}
}

func TestDecodersPropagateReaderErrors(t *testing.T) {
	want := errors.New("read failed")
	tests := []struct {
		name   string
		decode func(io.Reader) (Overrides, error)
	}{
		{name: "JSON", decode: decodeJSON},
		{name: "YAML", decode: decodeYAML},
		{name: "TOML", decode: decodeTOML},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, err := test.decode(iotest.ErrReader(want))
			if err == nil || !strings.Contains(err.Error(), want.Error()) {
				t.Fatalf("decode error = %v, want %v", err, want)
			}
		})
	}
}
