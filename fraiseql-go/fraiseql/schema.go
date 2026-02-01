package fraiseql

import (
	"encoding/json"
	"fmt"
	"os"
)

// ExportSchema exports the schema registry to a JSON file
// Returns error if file cannot be written
func ExportSchema(outputPath string) error {
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
	schema := GetSchema()
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
