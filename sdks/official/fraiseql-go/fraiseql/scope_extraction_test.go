package fraiseql

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"
)

/**
 * Field-Level RBAC for Go SDK
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

	fields, err := ExtractFields(reflect.TypeOf(UserWithScope{}))
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

	fields, err := ExtractFields(reflect.TypeOf(UserWithMultipleScopes{}))
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

	fields, err := ExtractFields(reflect.TypeOf(UserWithMixedFields{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["id"].Scope != "" {
		t.Errorf("Expected empty scope for public field, got '%s'", fields["id"].Scope)
	}
}

// ============================================================================
// MULTIPLE SCOPES ON A SINGLE FIELD ARE REFUSED (3 tests)
//
// These three tests previously asserted that `scopes=a;b` extracted cleanly into a
// FieldInfo.Scopes array. It did — and then the array was emitted as a `scopes` key
// that the compiler does not read, so the compiled field carried no scope at all and
// the runtime served it to callers holding none (#807). The extraction worked; the
// gate it was extracting never existed. Multiple required scopes have no
// representation in the compiled schema or the runtime field filter, so the SDK now
// refuses them at authoring time rather than accepting a declaration it cannot honour.
// ============================================================================

func TestMultipleScopesOnSingleFieldAreRefused(t *testing.T) {
	Reset()
	defer Reset()

	type AdminWithMultipleScopes struct {
		ID        int    `fraiseql:"id,type=Int"`
		AdminNotes string `fraiseql:"adminNotes,type=String,scopes=admin;auditor"`
	}

	_, err := ExtractFields(reflect.TypeOf(AdminWithMultipleScopes{}))
	if err == nil {
		t.Fatal("multiple scopes must be refused: the compiled field would carry no scope at all")
	}
	if !strings.Contains(err.Error(), "not supported") {
		t.Errorf("error must explain that multiple scopes are unsupported, got: %v", err)
	}
}

func TestSingleScopeSurvivesAndMultipleAreRefused(t *testing.T) {
	Reset()
	defer Reset()

	type SingleScopeType struct {
		BasicField string `fraiseql:"basicField,type=String,scope=read:basic"`
	}

	fields, err := ExtractFields(reflect.TypeOf(SingleScopeType{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}
	if fields["basicField"].Scope != "read:basic" {
		t.Error("Single scope field extraction failed")
	}

	type MultiScopeType struct {
		AdvancedField string `fraiseql:"advancedField,type=String,scopes=read:advanced;admin"`
	}

	if _, err := ExtractFields(reflect.TypeOf(MultiScopeType{})); err == nil {
		t.Error("a multi-scope field in the same type must still be refused")
	}
}

func TestSingletonScopesListIsAcceptedAsASingleScope(t *testing.T) {
	// `scopes=one` carries exactly one requirement, which the compiled schema and the
	// runtime can both represent, so it is accepted and normalised onto `Scope`.
	// Anything longer is refused by TestMultipleScopesOnSingleFieldAreRefused.
	Reset()
	defer Reset()

	type SingletonScopes struct {
		Restricted string `fraiseql:"restricted,type=String,scopes=only:one"`
	}

	fields, err := ExtractFields(reflect.TypeOf(SingletonScopes{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["restricted"].Scope != "only:one" {
		t.Errorf("a singleton scopes list must normalise onto Scope, got %q",
			fields["restricted"].Scope)
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

	fields, err := ExtractFields(reflect.TypeOf(ResourcePatternScopes{}))
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

	fields, err := ExtractFields(reflect.TypeOf(ActionPatternScopes{}))
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

	fields, err := ExtractFields(reflect.TypeOf(GlobalWildcardScope{}))
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

	schemaJSON, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}
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

	// The exported key must be `requires_scope` — the key the compiler reads. This
	// assertion used to pin `scope`, so it certified the exact drift that left every
	// Go-authored field gate off the compiled schema (#807): the SDK's own contract
	// test agreed with the SDK and neither agreed with the compiler.
	if scope, ok := salaryFieldObj["requires_scope"]; !ok || scope != "read:user.salary" {
		t.Errorf("requires_scope not exported to JSON or incorrect value: %v", scope)
	}
	if _, drifted := salaryFieldObj["scope"]; drifted {
		t.Error("the drifted `scope` key must no longer be emitted")
	}
}

func TestMultipleScopesNeverReachTheExportedSchema(t *testing.T) {
	// This test used to assert that a `scopes` array appeared in the exported JSON.
	// It did appear — and the compiler ignored it, so the field shipped ungated. The
	// SDK now refuses the declaration at registration time; the important property is
	// that no schema is ever exported carrying an unreadable scope key.
	Reset()
	defer Reset()

	type ExportTestMultipleScopes struct {
		Restricted string `fraiseql:"restricted,type=String,scopes=scope1;scope2"`
	}

	if err := RegisterTypes(ExportTestMultipleScopes{}); err == nil {
		t.Fatal("registering a multi-scope field must fail rather than export an unreadable key")
	}

	schemaJSON, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}
	if strings.Contains(string(schemaJSON), `"scopes"`) {
		t.Errorf("the exported schema must never carry a `scopes` key: %s", schemaJSON)
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

	schemaJSON, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}
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

	fields, err := ExtractFields(reflect.TypeOf(ScopeWithMetadata{}))
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

	fields, err := ExtractFields(reflect.TypeOf(ScopeWithNullable{}))
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
		Field1 string `fraiseql:"field1,type=String,scope=read:scope1"`
		Field2 string `fraiseql:"field2,type=String,scope=read:scope2"`
	}

	fields, err := ExtractFields(reflect.TypeOf(MetadataIndependence{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["field1"].Scope != "read:scope1" {
		t.Error("Field1 scope incorrect")
	}
	if fields["field2"].Scope != "read:scope2" {
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

	_, err := ExtractFields(reflect.TypeOf(InvalidScopeFormat{}))
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

	_, err := ExtractFields(reflect.TypeOf(EmptyScope{}))
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

	_, err := ExtractFields(reflect.TypeOf(EmptyScopesArray{}))
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

	_, err := ExtractFields(reflect.TypeOf(InvalidActionWithHyphens{}))
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

	_, err := ExtractFields(reflect.TypeOf(InvalidResourceWithHyphens{}))
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

	_, err := ExtractFields(reflect.TypeOf(ConflictingScopeAndScopes{}))
	if err == nil {
		t.Error("Should reject field with both scope and scopes")
	}
}

