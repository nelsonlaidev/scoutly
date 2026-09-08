package config

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"strings"

	"github.com/pelletier/go-toml/v2"
	"go.yaml.in/yaml/v3"
)

func decodeFile(path string) (Overrides, error) {
	// The caller explicitly selects the configuration path.
	//nolint:gosec // Opening that path is the purpose of this function.
	file, err := os.Open(path)
	if err != nil {
		return Overrides{}, fmt.Errorf("open config %q: %w", path, err)
	}
	defer func() {
		_ = file.Close()
	}()

	var overrides Overrides

	switch strings.ToLower(filepath.Ext(path)) {
	case ".json":
		overrides, err = decodeJSON(file)
	case ".yaml", ".yml":
		overrides, err = decodeYAML(file)
	case ".toml":
		overrides, err = decodeTOML(file)
	default:
		return Overrides{}, fmt.Errorf("unsupported config format %q", filepath.Ext(path))
	}
	if err != nil {
		return Overrides{}, fmt.Errorf("decode config %q: %w", path, err)
	}

	return overrides, nil
}

func decodeJSON(reader io.Reader) (Overrides, error) {
	data, err := io.ReadAll(reader)
	if err != nil {
		return Overrides{}, err
	}
	if err := validateJSONDocument(data); err != nil {
		return Overrides{}, err
	}

	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()

	var overrides Overrides
	if err := decoder.Decode(&overrides); err != nil {
		return Overrides{}, err
	}

	return overrides, nil
}

func decodeYAML(reader io.Reader) (Overrides, error) {
	decoder := yaml.NewDecoder(reader)

	var document yaml.Node
	if err := decoder.Decode(&document); err != nil {
		return Overrides{}, err
	}

	var extra yaml.Node
	err := decoder.Decode(&extra)
	if !errors.Is(err, io.EOF) {
		if err == nil {
			return Overrides{}, errors.New("multiple YAML documents are not allowed")
		}
		return Overrides{}, err
	}

	if err := validateYAMLDocument(&document); err != nil {
		return Overrides{}, err
	}

	var overrides Overrides
	if err := document.Decode(&overrides); err != nil {
		return Overrides{}, err
	}

	return overrides, nil
}

func decodeTOML(reader io.Reader) (Overrides, error) {
	var overrides Overrides
	if err := toml.NewDecoder(reader).DisallowUnknownFields().Decode(&overrides); err != nil {
		return Overrides{}, err
	}

	return overrides, nil
}

func validateJSONDocument(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok || delimiter != '{' {
		return errors.New("configuration must be an object")
	}

	schema := overrideSchema()
	seen := make(map[string]struct{}, len(schema))
	for decoder.More() {
		token, err := decoder.Token()
		if err != nil {
			return normalizeJSONStructureError(err)
		}
		key, ok := token.(string)
		if !ok {
			return errors.New("configuration keys must be strings")
		}
		if _, ok := schema[key]; !ok {
			return fmt.Errorf("unknown field %q", key)
		}
		if _, ok := seen[key]; ok {
			return fmt.Errorf("duplicate field %q", key)
		}
		seen[key] = struct{}{}

		var value json.RawMessage
		if err := decoder.Decode(&value); err != nil {
			return normalizeJSONStructureError(err)
		}
		if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return fmt.Errorf("field %q cannot be null", key)
		}
		if key == "rules" {
			if err := validateJSONRules(value); err != nil {
				return err
			}
		}
	}

	if _, err := decoder.Token(); err != nil {
		return normalizeJSONStructureError(err)
	}

	var extra any
	err = decoder.Decode(&extra)
	if !errors.Is(err, io.EOF) {
		if err == nil {
			return errors.New("multiple JSON documents are not allowed")
		}
		return err
	}

	return nil
}

func validateJSONRules(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok || delimiter != '{' {
		return errors.New("field \"rules\" must be an object")
	}

	seen := make(map[string]struct{})
	for decoder.More() {
		token, err := decoder.Token()
		if err != nil {
			return normalizeJSONStructureError(err)
		}
		key, ok := token.(string)
		if !ok {
			return errors.New("rule keys must be strings")
		}
		if _, exists := seen[key]; exists {
			return fmt.Errorf("duplicate rule %q", key)
		}
		seen[key] = struct{}{}

		var level json.RawMessage
		if err := decoder.Decode(&level); err != nil {
			return normalizeJSONStructureError(err)
		}
		if bytes.Equal(bytes.TrimSpace(level), []byte("null")) {
			return fmt.Errorf("rule %q cannot be null", key)
		}
		var value string
		if err := json.Unmarshal(level, &value); err != nil {
			return fmt.Errorf("rule %q must be a string: %w", key, err)
		}
	}

	if _, err := decoder.Token(); err != nil {
		return normalizeJSONStructureError(err)
	}
	return nil
}

func normalizeJSONStructureError(err error) error {
	if errors.Is(err, io.EOF) {
		return io.ErrUnexpectedEOF
	}
	return err
}

func validateYAMLDocument(document *yaml.Node) error {
	if document.Kind != yaml.DocumentNode || len(document.Content) != 1 {
		return errors.New("configuration must be a mapping")
	}

	mapping := document.Content[0]
	if mapping.Kind != yaml.MappingNode {
		return errors.New("configuration must be a mapping")
	}

	schema := overrideSchema()
	seen := make(map[string]struct{}, len(schema))
	for index := 0; index < len(mapping.Content); index += 2 {
		keyNode := mapping.Content[index]
		valueNode := mapping.Content[index+1]
		if keyNode.Kind != yaml.ScalarNode || keyNode.Tag != "!!str" {
			return errors.New("configuration keys must be strings")
		}

		key := keyNode.Value
		kind, ok := schema[key]
		if !ok {
			return fmt.Errorf("unknown field %q", key)
		}
		if _, ok := seen[key]; ok {
			return fmt.Errorf("duplicate field %q", key)
		}
		seen[key] = struct{}{}

		if valueNode.Tag == "!!null" {
			return fmt.Errorf("field %q cannot be null", key)
		}
		if !matchesYAMLKind(valueNode, kind) {
			return fmt.Errorf("field %q has invalid type %s", key, valueNode.Tag)
		}
		if key == "rules" {
			if err := validateYAMLRules(valueNode); err != nil {
				return err
			}
		}
	}

	return nil
}

func validateYAMLRules(mapping *yaml.Node) error {
	seen := make(map[string]struct{}, len(mapping.Content)/2)
	for index := 0; index < len(mapping.Content); index += 2 {
		keyNode := mapping.Content[index]
		valueNode := mapping.Content[index+1]
		if keyNode.Kind != yaml.ScalarNode || keyNode.Tag != "!!str" {
			return errors.New("rule keys must be strings")
		}

		key := keyNode.Value
		if _, exists := seen[key]; exists {
			return fmt.Errorf("duplicate rule %q", key)
		}
		seen[key] = struct{}{}
		if valueNode.Tag == "!!null" {
			return fmt.Errorf("rule %q cannot be null", key)
		}
		if valueNode.Kind != yaml.ScalarNode || valueNode.Tag != "!!str" {
			return fmt.Errorf("rule %q has invalid type %s", key, valueNode.Tag)
		}
	}
	return nil
}

func overrideSchema() map[string]reflect.Kind {
	typeOfOverrides := reflect.TypeFor[Overrides]()
	schema := make(map[string]reflect.Kind, typeOfOverrides.NumField())

	for field := range typeOfOverrides.Fields() {
		key, _, _ := strings.Cut(field.Tag.Get("json"), ",")
		fieldType := field.Type
		if fieldType.Kind() == reflect.Pointer {
			fieldType = fieldType.Elem()
		}
		schema[key] = fieldType.Kind()
	}

	return schema
}

func matchesYAMLKind(node *yaml.Node, kind reflect.Kind) bool {
	switch kind {
	case reflect.Int:
		return node.Kind == yaml.ScalarNode && node.Tag == "!!int"
	case reflect.Float64:
		return node.Kind == yaml.ScalarNode && (node.Tag == "!!int" || node.Tag == "!!float")
	case reflect.Bool:
		return node.Kind == yaml.ScalarNode && node.Tag == "!!bool"
	case reflect.String:
		return node.Kind == yaml.ScalarNode && node.Tag == "!!str"
	case reflect.Map:
		return node.Kind == yaml.MappingNode
	default:
		return false
	}
}
