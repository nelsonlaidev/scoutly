package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

var candidateNames = []string{
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

type LoadOptions struct {
	Directory        string
	ConfigPath       string
	DisableDiscovery bool
	Version          string
}

type LoadResult struct {
	ConfigPath string
	Base       Config
}

type AmbiguousConfigError struct {
	Paths []string
}

func (e *AmbiguousConfigError) Error() string {
	return fmt.Sprintf("multiple Scoutly config files found: %s", strings.Join(e.Paths, ", "))
}

func Load(options LoadOptions) (LoadResult, error) {
	result := LoadResult{Base: defaults(options.Version)}

	if options.ConfigPath != "" && options.DisableDiscovery {
		return LoadResult{}, errors.New("config path and disabled config discovery cannot be used together")
	}

	if options.DisableDiscovery {
		return result, nil
	}

	directory := options.Directory
	if directory == "" {
		var err error
		directory, err = os.Getwd()
		if err != nil {
			return LoadResult{}, fmt.Errorf("get working directory: %w", err)
		}
	}

	path := options.ConfigPath
	if strings.HasPrefix(path, "~/") || path == "~" {
		home, err := os.UserHomeDir()
		if err != nil {
			return LoadResult{}, fmt.Errorf("resolve home directory in config path %q: %w", path, err)
		}
		if path == "~" {
			path = home
		} else {
			path = filepath.Join(home, strings.TrimPrefix(path, "~/"))
		}
	}
	if path != "" && !filepath.IsAbs(path) {
		path = filepath.Join(directory, path)
	}

	if path == "" {
		var err error
		path, err = discover(directory)
		if err != nil {
			return LoadResult{}, err
		}
		if path == "" {
			return result, nil
		}
	}

	info, err := os.Stat(path)
	if err != nil {
		return LoadResult{}, fmt.Errorf("stat config %q: %w", path, err)
	}
	if !info.Mode().IsRegular() {
		return LoadResult{}, fmt.Errorf("config %q is not a regular file", path)
	}

	overrides, err := decodeFile(path)
	if err != nil {
		return LoadResult{}, err
	}

	result.ConfigPath = path
	result.Base = apply(result.Base, overrides)

	return result, nil
}

func discover(directory string) (string, error) {
	paths := make([]string, 0, 1)

	for _, name := range candidateNames {
		path := filepath.Join(directory, name)
		info, err := os.Stat(path)
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return "", fmt.Errorf("stat config candidate %q: %w", path, err)
		}
		if !info.Mode().IsRegular() {
			return "", fmt.Errorf("config candidate %q is not a regular file", path)
		}

		paths = append(paths, path)
	}

	if len(paths) > 1 {
		return "", &AmbiguousConfigError{Paths: paths}
	}
	if len(paths) == 0 {
		return "", nil
	}

	return paths[0], nil
}
