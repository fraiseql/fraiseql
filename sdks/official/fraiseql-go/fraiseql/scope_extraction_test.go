package fraiseql

import (
	"encoding/json"
	"testing"
)

/**
 * Phase 18 Cycle 10: Field-Level RBAC for Go SDK
 *
 * Tests that field scopes are properly extracted from struct tags,
 * stored in registry, and exported to JSON for compiler consumption.
 *
 * RED Phase: 21 comprehensive test cases
 * - 15 happy path tests for scope extraction and export
 * - 6 validation tests for error handling
 *
 * Struct tag format:
 * - Single scope: `fraiseql:"name,type=String,scope=read:user.email"`
 * - Multiple scopes: `fraiseql:"name,type=String,scopes=admin;auditor"`
 */

// ============================================================================
// HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
// ============================================================================

func TestSingleScopeExtraction(t *testing.T) {
	// RED: This test fails because FieldInfo doesn't store scope
	Reset()
	defer Reset()

	type UserWithScope struct {
		ID     int     `fraiseql:"id,type=Int"`
		Salary float64 `fraiseql:"salary,type=Float,scope=read:user.salary"`
	}

	fields, err := ExtractFields(&UserWithScope{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	salaryField, exists := fields["salary"]
	if !exists {
		t.Error("salary field not extracted")
	}

	if salaryField.Scope != "read:user.salary" {
		t.Errorf("Expected scope 'read:user.salary', got '%s'", salaryField.Scope)
	}
}

func TestMultipleDifferentScopesExtraction(t *testing.T) {
	// RED: Tests extraction of different scopes on different fields
	Reset()
	defer Reset()

	type UserWithMultipleScopes struct {
		ID    int    `fraiseql:"id,type=Int"`
		Email string `fraiseql:"email,type=String,scope=read:user.email"`
		Phone string `fraiseql:"phone,type=String,scope=read:user.phone"`
		SSN   string `fraiseql:"ssn,type=String,scope=read:user.ssn"`
	}

	fields, err := ExtractFields(&UserWithMultipleScopes{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["email"].Scope != "read:user.email" {
		t.Errorf("email scope mismatch")
	}
	if fields["phone"].Scope != "read:user.phone" {
		t.Errorf("phone scope mismatch")
	}
	if fields["ssn"].Scope != "read:user.ssn" {
		t.Errorf("ssn scope mismatch")
	}
}

func TestPublicFieldNoScopeExtraction(t *testing.T) {
	// RED: Public fields should have empty scope
	Reset()
	defer Reset()

	type UserWithMixedFields struct {
		ID    int    `fraiseql:"id,type=Int"`
		Name  string `fraiseql:"name,type=String"`
		Email string `fraiseql:"email,type=String,scope=read:user.email"`
	}

	fields, err := ExtractFields(&UserWithMixedFields{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["id"].Scope != "" {
		t.Errorf("Expected empty scope for public field, got '%s'", fields["id"].Scope)
	}
}

// ============================================================================
// HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
// ============================================================================

func TestMultipleScopesOnSingleField(t *testing.T) {
	// RED: Field with scopes=scope1;scope2 (semicolon-separated)
	Reset()
	defer Reset()

	type AdminWithMultipleScopes struct {
		ID        int    `fraiseql:"id,type=Int"`
		AdminNotes string `fraiseql:"adminNotes,type=String,scopes=admin;auditor"`
	}

	fields, err := ExtractFields(&AdminWithMultipleScopes{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	adminNotesField := fields["adminNotes"]
	if len(adminNotesField.Scopes) != 2 {
		t.Errorf("Expected 2 scopes, got %d", len(adminNotesField.Scopes))
	}

	if !contains(adminNotesField.Scopes, "admin") || !contains(adminNotesField.Scopes, "auditor") {
		t.Errorf("Scopes array doesn't contain expected values: %v", adminNotesField.Scopes)
	}
}

func TestMixedSingleAndMultipleScopes(t *testing.T) {
	// RED: Type with both single-scope and multi-scope fields
	Reset()
	defer Reset()

	type MixedScopeTypes struct {
		BasicField    string `fraiseql:"basicField,type=String,scope=read:basic"`
		AdvancedField string `fraiseql:"advancedField,type=String,scopes=read:advanced;admin"`
	}

	fields, err := ExtractFields(&MixedScopeTypes{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["basicField"].Scope != "read:basic" {
		t.Error("Single scope field extraction failed")
	}

	if len(fields["advancedField"].Scopes) != 2 {
		t.Error("Multiple scopes extraction failed")
	}
}

func TestScopeArrayOrder(t *testing.T) {
	// RED: Scopes array order must be preserved
	Reset()
	defer Reset()

	type OrderedScopes struct {
		Restricted string `fraiseql:"restricted,type=String,scopes=first;second;third"`
	}

	fields, err := ExtractFields(&OrderedScopes{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	scopes := fields["restricted"].Scopes
	if len(scopes) != 3 || scopes[0] != "first" || scopes[1] != "second" || scopes[2] != "third" {
		t.Errorf("Scope order not preserved: %v", scopes)
	}
}

// ============================================================================
// HAPPY PATH: SCOPE PATTERNS (3 tests)
// ============================================================================

func TestResourceBasedScopePattern(t *testing.T) {
	// RED: Resource pattern like read:User.email
	Reset()
	defer Reset()

	type ResourcePatternScopes struct {
		Email string `fraiseql:"email,type=String,scope=read:User.email"`
		Phone string `fraiseql:"phone,type=String,scope=read:User.phone"`
	}

	fields, err := ExtractFields(&ResourcePatternScopes{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["email"].Scope != "read:User.email" {
		t.Error("Resource pattern not preserved")
	}
}

func TestActionBasedScopePattern(t *testing.T) {
	// RED: Action patterns like read:*, write:*, admin:*
	Reset()
	defer Reset()

	type ActionPatternScopes struct {
		ReadableField  string `fraiseql:"readableField,type=String,scope=read:User.*"`
		WritableField  string `fraiseql:"writableField,type=String,scope=write:User.*"`
	}

	fields, err := ExtractFields(&ActionPatternScopes{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["readableField"].Scope != "read:User.*" {
		t.Error("Action pattern not preserved for read")
	}
	if fields["writableField"].Scope != "write:User.*" {
		t.Error("Action pattern not preserved for write")
	}
}

func TestGlobalWildcardScope(t *testing.T) {
	// RED: Global wildcard matching all scopes
	Reset()
	defer Reset()

	type GlobalWildcardScope struct {
		AdminOverride string `fraiseql:"adminOverride,type=String,scope=*"`
	}

	fields, err := ExtractFields(&GlobalWildcardScope{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["adminOverride"].Scope != "*" {
		t.Errorf("Global wildcard not preserved, got '%s'", fields["adminOverride"].Scope)
	}
}

// ============================================================================
// HAPPY PATH: JSON EXPORT (3 tests)
// ============================================================================

func TestScopeExportToJsonSingleScope(t *testing.T) {
	// RED: Scope must appear in JSON export
	Reset()
	defer Reset()

	type ExportTestSingleScope struct {
		Salary float64 `fraiseql:"salary,type=Float,scope=read:user.salary"`
	}

	RegisterTypes(ExportTestSingleScope{})

	schemaJSON := GetSchemaJSON(false)
	var schema map[string]interface{}
	if err := json.Unmarshal([]byte(schemaJSON), &schema); err != nil {
		t.Fatalf("Failed to unmarshal JSON: %v", err)
	}

	types, ok := schema["types"].([]interface{})
	if !ok || len(types) == 0 {
		t.Fatal("No types in schema")
	}

	typeObj := types[0].(map[string]interface{})
	fields := typeObj["fields"].([]interface{})
	salaryFieldObj := fields[0].(map[string]interface{})

	if scope, ok := salaryFieldObj["scope"]; !ok || scope != "read:user.salary" {
		t.Errorf("Scope not exported to JSON or incorrect value: %v", scope)
	}
}

func TestScopeExportToJsonMultipleScopes(t *testing.T) {
	// RED: scopes array exported as scopes field in JSON
	Reset()
	defer Reset()

	type ExportTestMultipleScopes struct {
		Restricted string `fraiseql:"restricted,type=String,scopes=scope1;scope2"`
	}

	RegisterTypes(ExportTestMultipleScopes{})

	schemaJSON := GetSchemaJSON(false)
	var schema map[string]interface{}
	if err := json.Unmarshal([]byte(schemaJSON), &schema); err != nil {
		t.Fatalf("Failed to unmarshal JSON: %v", err)
	}

	types := schema["types"].([]interface{})
	typeObj := types[0].(map[string]interface{})
	fields := typeObj["fields"].([]interface{})
	fieldObj := fields[0].(map[string]interface{})

	scopes, ok := fieldObj["scopes"].([]interface{})
	if !ok || len(scopes) != 2 {
		t.Errorf("Scopes not exported to JSON or incorrect length: %v", scopes)
	}
}

func TestPublicFieldJsonExport(t *testing.T) {
	// RED: Public fields without scope should not have scope in JSON
	Reset()
	defer Reset()

	type ExportTestPublicField struct {
		ID   int    `fraiseql:"id,type=Int"`
		Name string `fraiseql:"name,type=String"`
	}

	RegisterTypes(ExportTestPublicField{})

	schemaJSON := GetSchemaJSON(false)
	var schema map[string]interface{}
	if err := json.Unmarshal([]byte(schemaJSON), &schema); err != nil {
		t.Fatalf("Failed to unmarshal JSON: %v", err)
	}

	types := schema["types"].([]interface{})
	typeObj := types[0].(map[string]interface{})
	fields := typeObj["fields"].([]interface{})
	idFieldObj := fields[0].(map[string]interface{})

	// Public field should not have scope key
	if _, hasScope := idFieldObj["scope"]; hasScope {
		t.Error("Public field should not have 'scope' key in JSON")
	}
	if _, hasScopes := idFieldObj["scopes"]; hasScopes {
		t.Error("Public field should not have 'scopes' key in JSON")
	}
}

// ============================================================================
// HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
// ============================================================================

func TestScopePreservedWithMetadata(t *testing.T) {
	// RED: Scope doesn't interfere with type, nullable, name
	Reset()
	defer Reset()

	type ScopeWithMetadata struct {
		Salary float64 `fraiseql:"salary,type=Float,scope=read:user.salary"`
	}

	fields, err := ExtractFields(&ScopeWithMetadata{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	salaryField := fields["salary"]
	if salaryField.Type != "Float" {
		t.Error("Type metadata not preserved")
	}
	if salaryField.Scope != "read:user.salary" {
		t.Error("Scope not preserved")
	}
}

func TestScopeWithNullableField(t *testing.T) {
	// RED: Scope works on nullable fields
	Reset()
	defer Reset()

	type ScopeWithNullable struct {
		OptionalEmail *string `fraiseql:"optionalEmail,type=String,nullable=true,scope=read:user.email"`
	}

	fields, err := ExtractFields(&ScopeWithNullable{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	emailField := fields["optionalEmail"]
	if !emailField.Nullable {
		t.Error("Nullable metadata not preserved")
	}
	if emailField.Scope != "read:user.email" {
		t.Error("Scope not preserved with nullable")
	}
}

func TestMultipleScopedFieldsMetadataIndependence(t *testing.T) {
	// RED: Each field's metadata is independent
	Reset()
	defer Reset()

	type MetadataIndependence struct {
		Field1 string `fraiseql:"field1,type=String,scope=scope1"`
		Field2 string `fraiseql:"field2,type=String,scope=scope2"`
	}

	fields, err := ExtractFields(&MetadataIndependence{})
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["field1"].Scope != "scope1" {
		t.Error("Field1 scope incorrect")
	}
	if fields["field2"].Scope != "scope2" {
		t.Error("Field2 scope incorrect")
	}
}

// ============================================================================
// VALIDATION: ERROR HANDLING (6 tests)
// ============================================================================

func TestInvalidScopeFormatDetection(t *testing.T) {
	// RED: Invalid scopes should be detected
	Reset()
	defer Reset()

	type InvalidScopeFormat struct {
		Field string `fraiseql:"field,type=String,scope=invalid_scope_no_colon"`
	}

	_, err := ExtractFields(&InvalidScopeFormat{})
	if err == nil {
		t.Error("Should reject invalid scope format (missing colon)")
	}
}

func TestEmptyScopeRejection(t *testing.T) {
	// RED: Empty string scope should be invalid
	Reset()
	defer Reset()

	type EmptyScope struct {
		Field string `fraiseql:"field,type=String,scope="`
	}

	_, err := ExtractFields(&EmptyScope{})
	if err == nil {
		t.Error("Should reject empty scope")
	}
}

func TestEmptyScopesArrayRejection(t *testing.T) {
	// RED: Empty scopes array should be invalid
	Reset()
	defer Reset()

	type EmptyScopesArray struct {
		Field string `fraiseql:"field,type=String,scopes="`
	}

	_, err := ExtractFields(&EmptyScopesArray{})
	if err == nil {
		t.Error("Should reject empty scopes array")
	}
}

func TestInvalidActionWithHyphensValidation(t *testing.T) {
	// RED: Hyphens in action prefix are invalid
	Reset()
	defer Reset()

	type InvalidActionWithHyphens struct {
		Field string `fraiseql:"field,type=String,scope=invalid-action:resource"`
	}

	_, err := ExtractFields(&InvalidActionWithHyphens{})
	if err == nil {
		t.Error("Should reject hyphens in action prefix")
	}
}

func TestInvalidResourceWithHyphensValidation(t *testing.T) {
	// RED: Hyphens in resource name are invalid
	Reset()
	defer Reset()

	type InvalidResourceWithHyphens struct {
		Field string `fraiseql:"field,type=String,scope=read:invalid-resource-name"`
	}

	_, err := ExtractFields(&InvalidResourceWithHyphens{})
	if err == nil {
		t.Error("Should reject hyphens in resource name")
	}
}

func TestConflictingBothScopeAndScopes(t *testing.T) {
	// RED: Can't have both scope= and scopes= on same field
	Reset()
	defer Reset()

	type ConflictingScopeAndScopes struct {
		Field string `fraiseql:"field,type=String,scope=read:user.email,scopes=admin;auditor"`
	}

	_, err := ExtractFields(&ConflictingScopeAndScopes{})
	if err == nil {
		t.Error("Should reject field with both scope and scopes")
	}
}

// ============================================================================
// TEST HELPERS
// ============================================================================

func contains(slice []string, item string) bool {
	for _, v := range slice {
		if v == item {
			return true
		}
	}
	return false
}
