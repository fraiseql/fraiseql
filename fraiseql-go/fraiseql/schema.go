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
	if len(schema.Observers) > 0 {
		fmt.Printf("   Observers: %d\n", len(schema.Observers))
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
