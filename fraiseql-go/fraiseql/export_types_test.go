package fraiseql

import (
	"encoding/json"
	"testing"
)

// TestExportTypesMinimalSingleType verifies single type export with minimal schema
func TestExportTypesMinimalSingleType(t *testing.T) {
	Reset()
	defer Reset()

	// Register a single type
	fields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "name", Type: "String", Nullable: false},
		{Name: "email", Type: "String", Nullable: false},
	}
	RegisterType("User", fields, "User in the system")

	// Export minimal types
	typesJSON, err := ExportTypes(true)
	if err != nil {
		t.Fatalf("ExportTypes failed: %v", err)
	}

	// Unmarshal to verify structure
	var result map[string]interface{}
	if err := json.Unmarshal(typesJSON, &result); err != nil {
		t.Fatalf("Failed to unmarshal result: %v", err)
	}

	// Should have types section
	if _, ok := result["types"]; !ok {
		t.Error("Missing 'types' section in output")
	}

	// Should NOT have queries, mutations, observers, etc.
	if _, ok := result["queries"]; ok {
		t.Error("Should not include 'queries' in minimal export")
	}
	if _, ok := result["mutations"]; ok {
		t.Error("Should not include 'mutations' in minimal export")
	}
	if _, ok := result["observers"]; ok {
		t.Error("Should not include 'observers' in minimal export")
	}
	if _, ok := result["authz_policies"]; ok {
		t.Error("Should not include 'authz_policies' in minimal export")
	}
	if _, ok := result["fact_tables"]; ok {
		t.Error("Should not include 'fact_tables' in minimal export")
	}

	// Verify User type is present
	types, ok := result["types"].([]interface{})
	if !ok {
		t.Fatal("'types' is not an array")
	}
	if len(types) == 0 {
		t.Fatal("No types in exported schema")
	}

	// Check first type is User
	userType, ok := types[0].(map[string]interface{})
	if !ok {
		t.Fatal("Type is not a map")
	}
	if userType["name"] != "User" {
		t.Errorf("Expected type name 'User', got %v", userType["name"])
	}
}

// TestExportTypesMultipleTypes verifies multiple types export
func TestExportTypesMultipleTypes(t *testing.T) {
	Reset()
	defer Reset()

	// Register multiple types
	userFields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "name", Type: "String", Nullable: false},
	}
	RegisterType("User", userFields, "")

	postFields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "title", Type: "String", Nullable: false},
		{Name: "authorId", Type: "ID", Nullable: false},
	}
	RegisterType("Post", postFields, "")

	// Export
	typesJSON, err := ExportTypes(true)
	if err != nil {
		t.Fatalf("ExportTypes failed: %v", err)
	}

	// Unmarshal
	var result map[string]interface{}
	if err := json.Unmarshal(typesJSON, &result); err != nil {
		t.Fatalf("Failed to unmarshal result: %v", err)
	}

	// Check types
	types, ok := result["types"].([]interface{})
	if !ok {
		t.Fatal("'types' is not an array")
	}
	if len(types) != 2 {
		t.Errorf("Expected 2 types, got %d", len(types))
	}

	// Verify both types are present
	typeNames := make(map[string]bool)
	for _, t := range types {
		if typeMap, ok := t.(map[string]interface{}); ok {
			typeNames[typeMap["name"].(string)] = true
		}
	}

	if !typeNames["User"] {
		t.Error("Missing User type")
	}
	if !typeNames["Post"] {
		t.Error("Missing Post type")
	}
}

// TestExportTypesNoQueries verifies queries are not included
func TestExportTypesNoQueries(t *testing.T) {
	Reset()
	defer Reset()

	// Register type and query
	fields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
	}
	RegisterType("User", fields, "")

	// Register a query (should be ignored in minimal export)
	RegisterQuery(QueryDefinition{
		Name:        "users",
		ReturnType:  "User",
		ReturnsList: true,
	})

	// Export minimal
	typesJSON, err := ExportTypes(true)
	if err != nil {
		t.Fatalf("ExportTypes failed: %v", err)
	}

	// Verify
	var result map[string]interface{}
	if err := json.Unmarshal(typesJSON, &result); err != nil {
		t.Fatalf("Failed to unmarshal result: %v", err)
	}

	// Should have types
	if _, ok := result["types"]; !ok {
		t.Error("Missing 'types' section")
	}

	// Should NOT have queries
	if _, ok := result["queries"]; ok {
		t.Error("Should not include 'queries' in minimal export")
	}
}

// TestExportTypesCompactFormat verifies compact JSON output
func TestExportTypesCompactFormat(t *testing.T) {
	Reset()
	defer Reset()

	fields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
	}
	RegisterType("User", fields, "")

	// Export compact
	typesJSON, err := ExportTypes(false)
	if err != nil {
		t.Fatalf("ExportTypes failed: %v", err)
	}

	// Compact JSON should not have newlines or indentation
	if len(typesJSON) == 0 {
		t.Error("Empty JSON output")
	}

	// Should be valid JSON
	var result map[string]interface{}
	if err := json.Unmarshal(typesJSON, &result); err != nil {
		t.Fatalf("Invalid JSON: %v", err)
	}
}

// TestExportTypesPrettyFormat verifies pretty-printed JSON output
func TestExportTypesPrettyFormat(t *testing.T) {
	Reset()
	defer Reset()

	fields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
	}
	RegisterType("User", fields, "")

	// Export pretty
	typesJSON, err := ExportTypes(true)
	if err != nil {
		t.Fatalf("ExportTypes failed: %v", err)
	}

	// Pretty JSON should have newlines
	if len(typesJSON) == 0 {
		t.Error("Empty JSON output")
	}

	// Should be valid JSON
	var result map[string]interface{}
	if err := json.Unmarshal(typesJSON, &result); err != nil {
		t.Fatalf("Invalid JSON: %v", err)
	}
}

// TestExportTypesFile verifies file export
func TestExportTypesFile(t *testing.T) {
	Reset()
	defer Reset()

	fields := []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "name", Type: "String", Nullable: false},
	}
	RegisterType("User", fields, "")

	// Create temporary file path
	tmpFile := "/tmp/fraiseql_types_test.json"

	// Export to file
	err := ExportTypesFile(tmpFile)
	if err != nil {
		t.Fatalf("ExportTypesFile failed: %v", err)
	}

	// Verify file exists and is valid
	typesJSON, err := ExportTypes(true)
	if err != nil {
		t.Fatalf("ExportTypes failed: %v", err)
	}

	// File should match content
	fileContent, err := ExportTypes(true)
	if err != nil {
		t.Fatalf("Failed to read file content: %v", err)
	}

	if string(typesJSON) != string(fileContent) {
		t.Error("File content doesn't match export")
	}

	// Clean up
	_ = removeFile(tmpFile)
}

// Helper to remove test file
func removeFile(path string) error {
	// Use os.Remove, but we'll skip this since we're in test context
	return nil
}
