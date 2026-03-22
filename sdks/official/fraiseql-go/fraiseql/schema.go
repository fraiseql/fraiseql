package fraiseql

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

var builtinScalars = map[string]struct{}{
	"String": {}, "Int": {}, "Float": {}, "Boolean": {}, "ID": {},
}

// validateSchemaBeforeExport checks that all query and mutation return types
// refer to registered types, returning a descriptive error if not.
func validateSchemaBeforeExport(schema Schema) error {
	registeredNames := make(map[string]struct{})
	for _, t := range schema.Types {
		registeredNames[t.Name] = struct{}{}
	}
	for k, v := range builtinScalars {
		registeredNames[k] = v
	}

	var errs []string

	for _, q := range schema.Queries {
		if _, ok := registeredNames[q.ReturnType]; !ok && q.ReturnType != "" {
			errs = append(errs, fmt.Sprintf(
				"query %q has return type %q which is not a registered type", q.Name, q.ReturnType,
			))
		}
	}
	for _, m := range schema.Mutations {
		if _, ok := registeredNames[m.ReturnType]; !ok && m.ReturnType != "" {
			errs = append(errs, fmt.Sprintf(
				"mutation %q has return type %q which is not a registered type", m.Name, m.ReturnType,
			))
		}
	}

	if len(errs) > 0 {
		return fmt.Errorf(
			"schema validation failed before export. Fix the following errors:\n  - %s",
			strings.Join(errs, "\n  - "),
		)
	}
	return nil
}

// ExportSchema exports the schema registry to a JSON file
// Returns error if file cannot be written
func ExportSchema(outputPath string) error {
	schema := GetSchema()
	if err := validateSchemaBeforeExport(schema); err != nil {
		return err
	}

	schemaJSON, err := GetSchemaJSON(true)
	if err != nil {
		return fmt.Errorf("failed to marshal schema to JSON: %w", err)
	}

	// Write to file
	err = os.WriteFile(outputPath, schemaJSON, 0o644)
	if err != nil {
		return fmt.Errorf("failed to write schema file: %w", err)
	}

	// Print summary
	fmt.Printf("✅ Schema exported to %s\n", outputPath)
	fmt.Printf("   Types: %d\n", len(schema.Types))
	fmt.Printf("   Queries: %d\n", len(schema.Queries))
	fmt.Printf("   Mutations: %d\n", len(schema.Mutations))
	if len(schema.FactTables) > 0 {
		fmt.Printf("   Fact Tables: %d\n", len(schema.FactTables))
	}
	if len(schema.AggregateQueries) > 0 {
		fmt.Printf("   Aggregate Queries: %d\n", len(schema.AggregateQueries))
	}

	return nil
}

// ExportSchemaRaw exports the schema registry to JSON bytes
// The pretty parameter controls formatting
func ExportSchemaRaw(pretty bool) ([]byte, error) {
	return GetSchemaJSON(pretty)
}

// MarshalJSON implements json.Marshaler for the Schema type
// This ensures proper JSON formatting
func (s Schema) MarshalJSON() ([]byte, error) {
	type Alias Schema
	return json.Marshal(&struct {
		*Alias
	}{
		Alias: (*Alias)(&s),
	})
}

// GetSchemaWithFederation returns the schema struct with federation metadata populated.
// serviceName is the logical subgraph name.
// defaultKeyFields is the default key fields for types without explicit KeyFields (defaults to ["id"]).
func GetSchemaWithFederation(serviceName string, defaultKeyFields []string) Schema {
	schema := GetSchema()

	if len(defaultKeyFields) == 0 {
		defaultKeyFields = []string{"id"}
	}

	var entities []FederationEntity
	for _, t := range schema.Types {
		if t.IsError {
			continue
		}
		keyFields := t.KeyFields
		if len(keyFields) == 0 {
			keyFields = defaultKeyFields
		}
		entities = append(entities, FederationEntity{
			Name:      t.Name,
			KeyFields: keyFields,
		})
	}

	schema.Federation = &FederationConfig{
		Enabled:       true,
		ServiceName:   serviceName,
		ApolloVersion: 2,
		Entities:      entities,
	}

	return schema
}

// ExportSchemaWithFederation exports the schema with federation metadata.
// serviceName is the logical subgraph name.
// defaultKeyFields is the default key fields for types without explicit KeyFields (defaults to ["id"]).
func ExportSchemaWithFederation(outputPath string, serviceName string, defaultKeyFields []string) error {
	schema := GetSchemaWithFederation(serviceName, defaultKeyFields)
	if err := validateSchemaBeforeExport(schema); err != nil {
		return err
	}

	schemaJSON, err := json.MarshalIndent(schema, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal schema to JSON: %w", err)
	}

	err = os.WriteFile(outputPath, schemaJSON, 0o644)
	if err != nil {
		return fmt.Errorf("failed to write schema file: %w", err)
	}

	fmt.Printf("✅ Schema exported to %s (federation: %s)\n", outputPath, serviceName)
	fmt.Printf("   Types: %d\n", len(schema.Types))
	fmt.Printf("   Queries: %d\n", len(schema.Queries))
	fmt.Printf("   Mutations: %d\n", len(schema.Mutations))
	fmt.Printf("   Federation entities: %d\n", len(schema.Federation.Entities))

	return nil
}

// ExportTypes exports only types to a minimal JSON structure
// This is used for the TOML-based workflow where types come from SDKs
// and configuration (queries, mutations, etc.) comes from fraiseql.toml
// The pretty parameter controls JSON formatting
func ExportTypes(pretty bool) ([]byte, error) {
	schema := GetSchema()

	// Build minimal schema with only types
	minimalSchema := map[string]interface{}{
		"types": schema.Types,
	}

	if pretty {
		return json.MarshalIndent(minimalSchema, "", "  ")
	}
	return json.Marshal(minimalSchema)
}

// ExportTypesFile exports types to a file using ExportTypes()
func ExportTypesFile(outputPath string) error {
	typesJSON, err := ExportTypes(true)
	if err != nil {
		return fmt.Errorf("failed to marshal types to JSON: %w", err)
	}

	// Write to file
	err = os.WriteFile(outputPath, typesJSON, 0o644)
	if err != nil {
		return fmt.Errorf("failed to write types file: %w", err)
	}

	// Print summary
	schema := GetSchema()
	fmt.Printf("✅ Types exported to %s\n", outputPath)
	fmt.Printf("   Types: %d\n", len(schema.Types))
	fmt.Printf("\n🎯 Next steps:\n")
	fmt.Printf("   1. fraiseql compile fraiseql.toml --types %s\n", outputPath)
	fmt.Printf("   2. This merges types with TOML configuration\n")
	fmt.Printf("   3. Result: schema.compiled.json with types + all config\n")

	return nil
}
