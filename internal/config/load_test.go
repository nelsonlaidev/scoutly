package config

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func TestLoadDiscoversEveryCandidateName(t *testing.T) {
	wantNames := []string{
		"scoutly.config.json",
		"scoutly.config.yaml",
		"scoutly.config.yml",
		"scoutly.config.toml",
		"scoutly.json",
		"scoutly.yaml",
		"scoutly.yml",
		"scoutly.toml",
		".scoutly.json",
		".scoutly.yaml",
		".scoutly.yml",
		".scoutly.toml",
	}
	if strings.Join(candidateNames, "|") != strings.Join(wantNames, "|") {
		t.Fatalf("candidateNames = %v, want %v", candidateNames, wantNames)
	}

	for _, name := range wantNames {
		t.Run(name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, name)
			writeConfig(t, path, 3)

			result, err := Load(LoadOptions{Directory: directory, Version: "test"})
			if err != nil {
				t.Fatalf("Load() error = %v", err)
			}
			if result.ConfigPath != path {
				t.Fatalf("ConfigPath = %q, want %q", result.ConfigPath, path)
			}
			if result.Base.MaxDepth != 3 {
				t.Fatalf("MaxDepth = %d, want 3", result.Base.MaxDepth)
			}
		})
	}
}

func TestLoadUsesDefaultsWhenNoConfigExists(t *testing.T) {
	result, err := Load(LoadOptions{Directory: t.TempDir(), Version: "test"})
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}
	if result.ConfigPath != "" {
		t.Fatalf("ConfigPath = %q, want empty", result.ConfigPath)
	}
	if result.Base != defaults("test") {
		t.Fatalf("Base = %#v, want defaults", result.Base)
	}
}

func TestLoadRejectsMultipleDiscoveredFiles(t *testing.T) {
	directory := t.TempDir()
	writeConfig(t, filepath.Join(directory, "scoutly.json"), 1)
	writeConfig(t, filepath.Join(directory, "scoutly.toml"), 2)

	_, err := Load(LoadOptions{Directory: directory})
	var ambiguous *AmbiguousConfigError
	if !errors.As(err, &ambiguous) {
		t.Fatalf("Load() error = %v, want *AmbiguousConfigError", err)
	}
	if len(ambiguous.Paths) != 2 {
		t.Fatalf("ambiguous paths = %v, want 2 paths", ambiguous.Paths)
	}
	want := "multiple Scoutly config files found: " + strings.Join(ambiguous.Paths, ", ")
	if err.Error() != want {
		t.Fatalf("AmbiguousConfigError.Error() = %q, want %q", err, want)
	}
}

func TestLoadDisableDiscoverySkipsFiles(t *testing.T) {
	directory := t.TempDir()
	writeFile(t, filepath.Join(directory, "scoutly.json"), `{invalid`)

	result, err := Load(LoadOptions{Directory: directory, DisableDiscovery: true, Version: "test"})
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}
	if result.Base != defaults("test") || result.ConfigPath != "" {
		t.Fatalf("Load() result = %#v, want defaults without config path", result)
	}
}

func TestLoadExplicitPathSkipsDiscovery(t *testing.T) {
	directory := t.TempDir()
	writeConfig(t, filepath.Join(directory, "scoutly.toml"), 2)
	writeConfig(t, filepath.Join(directory, "custom.json"), 4)

	result, err := Load(LoadOptions{
		Directory:  directory,
		ConfigPath: "custom.json",
		Version:    "test",
	})
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}
	if result.Base.MaxDepth != 4 {
		t.Fatalf("MaxDepth = %d, want 4", result.Base.MaxDepth)
	}
	if result.ConfigPath != filepath.Join(directory, "custom.json") {
		t.Fatalf("ConfigPath = %q", result.ConfigPath)
	}
}

func TestLoadExpandsHomeInExplicitPath(t *testing.T) {
	home := t.TempDir()
	setTestHome(t, home)
	path := filepath.Join(home, "custom.toml")
	writeConfig(t, path, 4)

	result, err := Load(LoadOptions{
		Directory:  t.TempDir(),
		ConfigPath: "~/custom.toml",
		Version:    "test",
	})
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}
	if result.ConfigPath != path {
		t.Fatalf("ConfigPath = %q, want %q", result.ConfigPath, path)
	}
}

func TestLoadExpandsHomeDirectoryPath(t *testing.T) {
	home := t.TempDir()
	setTestHome(t, home)

	if _, err := Load(LoadOptions{Directory: t.TempDir(), ConfigPath: "~"}); err == nil || !strings.Contains(err.Error(), "not a regular file") {
		t.Fatalf("Load() error = %v, want non-regular home path", err)
	}
}

func TestLoadReportsWorkingAndHomeDirectoryErrors(t *testing.T) {
	t.Run("default working directory", func(t *testing.T) {
		t.Chdir(t.TempDir())
		if _, err := Load(LoadOptions{}); err != nil {
			t.Fatalf("Load() error = %v", err)
		}
	})

	t.Run("home directory", func(t *testing.T) {
		setTestHome(t, "")
		if _, err := Load(LoadOptions{Directory: t.TempDir(), ConfigPath: "~"}); err == nil || !strings.Contains(err.Error(), "resolve home directory") {
			t.Fatalf("Load() error = %v, want home-directory context", err)
		}
	})
}

func TestLoadRejectsInvalidSourceSelection(t *testing.T) {
	if _, err := Load(LoadOptions{ConfigPath: "scoutly.toml", DisableDiscovery: true}); err == nil {
		t.Fatal("Load() error = nil")
	}
}

func TestLoadRejectsUnsupportedFormat(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "custom.xml")
	writeFile(t, path, `<config/>`)

	if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil || !strings.Contains(err.Error(), "unsupported config format") {
		t.Fatalf("Load() error = %v", err)
	}
}

func TestLoadRejectsMissingExplicitFile(t *testing.T) {
	directory := t.TempDir()
	if _, err := Load(LoadOptions{Directory: directory, ConfigPath: "missing.toml"}); err == nil || !strings.Contains(err.Error(), "stat config") {
		t.Fatalf("Load() error = %v", err)
	}
}

func TestLoadRejectsNonRegularCandidate(t *testing.T) {
	directory := t.TempDir()
	if err := os.Mkdir(filepath.Join(directory, "scoutly.toml"), 0o700); err != nil {
		t.Fatalf("create config directory: %v", err)
	}

	if _, err := Load(LoadOptions{Directory: directory}); err == nil || !strings.Contains(err.Error(), "not a regular file") {
		t.Fatalf("Load() error = %v", err)
	}
}

func TestLoadRejectsNonRegularExplicitPath(t *testing.T) {
	directory := t.TempDir()
	if _, err := Load(LoadOptions{Directory: directory, ConfigPath: directory}); err == nil || !strings.Contains(err.Error(), "not a regular file") {
		t.Fatalf("Load() error = %v", err)
	}
}

func TestDiscoverReportsCandidateStatErrors(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "scoutly.toml")
	if err := os.Symlink(filepath.Base(path), path); err != nil {
		t.Fatal(err)
	}

	if _, err := discover(directory); err == nil || !strings.Contains(err.Error(), "stat config candidate") {
		t.Fatalf("discover() error = %v", err)
	}
}

func TestLoadRejectsUnknownFields(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.json", content: `{"max_dept": 3}`},
		{name: "config.yaml", content: "max_dept: 3\n"},
		{name: "config.toml", content: "max_dept = 3\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, test.name)
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil {
				t.Fatal("Load() error = nil")
			}
		})
	}
}

func TestLoadRejectsInvalidTypes(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.json", content: `{"max_depth": "three"}`},
		{name: "config.yaml", content: "max_depth: three\n"},
		{name: "config.toml", content: `max_depth = "three"`},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, test.name)
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil {
				t.Fatal("Load() error = nil")
			}
		})
	}
}

func TestLoadRejectsLossyYAMLValues(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "fractional integer", content: "max_depth: 1.9\n"},
		{name: "legacy boolean", content: "respect_robots: yes\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, "config.yaml")
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil {
				t.Fatal("Load() error = nil")
			}
		})
	}
}

func TestLoadRejectsNullFields(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.json", content: `{"max_depth": null}`},
		{name: "config.yaml", content: "max_depth: null\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, test.name)
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil || !strings.Contains(err.Error(), "cannot be null") {
				t.Fatalf("Load() error = %v", err)
			}
		})
	}
}

func TestLoadRejectsDuplicateAndNonCanonicalJSONFields(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "duplicate", content: `{"max_depth": 1, "max_depth": 2}`},
		{name: "noncanonical", content: `{"Max_Depth": 1}`},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, "config.json")
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil {
				t.Fatal("Load() error = nil")
			}
		})
	}
}

func TestLoadPreservesExplicitZeroAndFalse(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.json", content: `{"max_depth": 0, "rate_limit": 0, "respect_robots": false}`},
		{name: "config.yaml", content: "max_depth: 0\nrate_limit: 0\nrespect_robots: false\n"},
		{name: "config.toml", content: "max_depth = 0\nrate_limit = 0\nrespect_robots = false\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, test.name)
			writeFile(t, path, test.content)

			result, err := Load(LoadOptions{Directory: directory, ConfigPath: path})
			if err != nil {
				t.Fatalf("Load() error = %v", err)
			}
			if result.Base.MaxDepth != 0 || result.Base.RateLimit != 0 || result.Base.RespectRobots {
				t.Fatalf("Base did not preserve zero/false values: %#v", result.Base)
			}
		})
	}
}

func TestLoadRejectsMultipleDocuments(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.json", content: `{"max_depth": 1} {"max_depth": 2}`},
		{name: "config.yaml", content: "max_depth: 1\n---\nmax_depth: 2\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, test.name)
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil || !strings.Contains(err.Error(), "multiple") {
				t.Fatalf("Load() error = %v", err)
			}
		})
	}
}

func TestLoadRejectsNullDocument(t *testing.T) {
	tests := []struct {
		name    string
		content string
	}{
		{name: "config.json", content: "null"},
		{name: "config.yaml", content: "null\n"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			path := filepath.Join(directory, test.name)
			writeFile(t, path, test.content)

			if _, err := Load(LoadOptions{Directory: directory, ConfigPath: path}); err == nil || !strings.Contains(err.Error(), "must be") {
				t.Fatalf("Load() error = %v", err)
			}
		})
	}
}

func TestRuntimeOverridesTakePriorityOverFile(t *testing.T) {
	directory := t.TempDir()
	writeConfig(t, filepath.Join(directory, "scoutly.toml"), 3)

	result, err := Load(LoadOptions{Directory: directory, Version: "test"})
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}

	effective := apply(result.Base, Overrides{MaxDepth: new(7)})
	if effective.MaxDepth != 7 {
		t.Fatalf("MaxDepth = %d, want 7", effective.MaxDepth)
	}
}

func writeConfig(t *testing.T, path string, maxDepth int) {
	t.Helper()

	value := strconv.Itoa(maxDepth)
	var content string
	switch filepath.Ext(path) {
	case ".json":
		content = `{"max_depth": ` + value + `}`
	case ".yaml", ".yml":
		content = "max_depth: " + value + "\n"
	case ".toml":
		content = "max_depth = " + value + "\n"
	default:
		t.Fatalf("unsupported test config extension %q", filepath.Ext(path))
	}

	writeFile(t, path, content)
}

func setTestHome(t *testing.T, home string) {
	t.Helper()

	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
}

func writeFile(t *testing.T, path, content string) {
	t.Helper()

	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write %q: %v", path, err)
	}
}
