package fraiseql

import (
	"fmt"
	"strings"
	"testing"
)

// EmailScalar is a test email scalar implementation
type EmailScalar struct{}

func (e *EmailScalar) Name() string {
	return "Email"
}

func (e *EmailScalar) Serialize(value interface{}) (interface{}, error) {
	return toString(value), nil
}

func (e *EmailScalar) ParseValue(value interface{}) (interface{}, error) {
	str := toString(value)
	str = strings.TrimSpace(str)
	if !strings.Contains(str, "@") {
		return nil, fmt.Errorf("invalid email format: %s", str)
	}
	return str, nil
}

func (e *EmailScalar) ParseLiteral(ast interface{}) (interface{}, error) {
	m, ok := ast.(map[string]interface{})
	if ok {
		if val, exists := m["value"]; exists {
			return e.ParseValue(val)
		}
	}
	return nil, fmt.Errorf("email literal must be string")
}

// PhoneScalar is a test phone scalar implementation
type PhoneScalar struct{}

func (p *PhoneScalar) Name() string {
	return "Phone"
}

func (p *PhoneScalar) Serialize(value interface{}) (interface{}, error) {
	return toString(value), nil
}

func (p *PhoneScalar) ParseValue(value interface{}) (interface{}, error) {
	str := toString(value)
	str = strings.TrimSpace(str)
	if !strings.HasPrefix(str, "+") {
		return nil, fmt.Errorf("phone must start with +")
	}
	digits := str[1:]
	for _, c := range digits {
		if c < '0' || c > '9' {
			return nil, fmt.Errorf("phone must contain only digits after +")
		}
	}
	if len(digits) < 10 || len(digits) > 14 {
		return nil, fmt.Errorf("phone must be 10-14 digits, got %d", len(digits))
	}
	return str, nil
}

func (p *PhoneScalar) ParseLiteral(ast interface{}) (interface{}, error) {
	m, ok := ast.(map[string]interface{})
	if ok {
		if val, exists := m["value"]; exists {
			return p.ParseValue(val)
		}
	}
	return nil, fmt.Errorf("phone literal must be string")
}

// ========================================================================
// Scalar Registration Tests
// ========================================================================

func TestRegisterCustomScalarGlobally(t *testing.T) {
	defer Reset()

	// Before registration
	if HasCustomScalar("Email") {
		t.Fatal("Email should not be registered yet")
	}

	// Register Email
	RegisterCustomScalar(&EmailScalar{})

	// After registration
	if !HasCustomScalar("Email") {
		t.Fatal("Email should be registered")
	}

	scalar := GetCustomScalar("Email")
	if scalar == nil {
		t.Fatal("Email scalar should be retrievable")
	}
}

func TestPreventsDuplicateNames(t *testing.T) {
	defer Reset()

	RegisterCustomScalar(&EmailScalar{})

	defer func() {
		if r := recover(); r == nil {
			t.Fatal("Expected panic for duplicate scalar name")
		}
	}()

	// Try to register another Email scalar
	RegisterCustomScalar(&EmailScalar{})
}

func TestRegistersMultipleScalars(t *testing.T) {
	defer Reset()

	RegisterCustomScalar(&EmailScalar{})
	RegisterCustomScalar(&PhoneScalar{})

	if !HasCustomScalar("Email") || !HasCustomScalar("Phone") {
		t.Fatal("Both scalars should be registered")
	}
}

func TestValidatesScalarHasName(t *testing.T) {
	defer Reset()

	// Create a scalar with empty name
	defer func() {
		if r := recover(); r == nil {
			t.Fatal("Expected panic for empty scalar name")
		}
	}()

	RegisterCustomScalar(&NoNameScalar{})
}

// NoNameScalar is a test scalar with empty name (for testing validation)
type NoNameScalar struct{}

func (n *NoNameScalar) Name() string {
	return ""
}

func (n *NoNameScalar) Serialize(v interface{}) (interface{}, error) {
	return v, nil
}

func (n *NoNameScalar) ParseValue(v interface{}) (interface{}, error) {
	return v, nil
}

func (n *NoNameScalar) ParseLiteral(v interface{}) (interface{}, error) {
	return v, nil
}

// ========================================================================
// Validation Engine Tests
// ========================================================================

func TestValidateParseValueSuccess(t *testing.T) {
	email := &EmailScalar{}
	result, err := ValidateCustomScalar(email, "user@example.com", "parseValue")
	if err != nil {
		t.Fatalf("Validation should succeed: %v", err)
	}
	if result != "user@example.com" {
		t.Fatalf("Expected 'user@example.com', got %v", result)
	}
}

func TestValidateParseValueFailure(t *testing.T) {
	email := &EmailScalar{}
	_, err := ValidateCustomScalar(email, "invalid-email", "parseValue")
	if err == nil {
		t.Fatal("Validation should fail for invalid email")
	}

	valErr, ok := err.(*ScalarValidationError)
	if !ok {
		t.Fatal("Error should be ScalarValidationError")
	}
	if valErr.ScalarName != "Email" {
		t.Fatalf("Expected scalar name 'Email', got %s", valErr.ScalarName)
	}
}

func TestValidateSerialize(t *testing.T) {
	email := &EmailScalar{}
	result, err := ValidateCustomScalar(email, "user@example.com", "serialize")
	if err != nil {
		t.Fatalf("Serialization should succeed: %v", err)
	}
	if result != "user@example.com" {
		t.Fatalf("Expected 'user@example.com', got %v", result)
	}
}

func TestValidateParseLiteral(t *testing.T) {
	email := &EmailScalar{}
	ast := map[string]interface{}{"value": "user@example.com"}
	result, err := ValidateCustomScalar(email, ast, "parseLiteral")
	if err != nil {
		t.Fatalf("Parsing literal should succeed: %v", err)
	}
	if result != "user@example.com" {
		t.Fatalf("Expected 'user@example.com', got %v", result)
	}
}

func TestValidateMultipleScalars(t *testing.T) {
	defer Reset()

	email := &EmailScalar{}
	phone := &PhoneScalar{}

	// Email validation
	result, err := ValidateCustomScalar(email, "test@test.com", "parseValue")
	if err != nil {
		t.Fatalf("Email validation should succeed: %v", err)
	}
	if result != "test@test.com" {
		t.Fatalf("Expected 'test@test.com', got %v", result)
	}

	// Phone validation
	result, err = ValidateCustomScalar(phone, "+12025551234", "parseValue")
	if err != nil {
		t.Fatalf("Phone validation should succeed: %v", err)
	}
	if result != "+12025551234" {
		t.Fatalf("Expected '+12025551234', got %v", result)
	}

	// Both fail appropriately
	_, err = ValidateCustomScalar(email, "notanemail", "parseValue")
	if err == nil {
		t.Fatal("Email validation should fail")
	}

	_, err = ValidateCustomScalar(phone, "invalid", "parseValue")
	if err == nil {
		t.Fatal("Phone validation should fail")
	}
}

func TestValidateInvalidContext(t *testing.T) {
	email := &EmailScalar{}
	_, err := ValidateCustomScalar(email, "test@test.com", "invalidContext")
	if err == nil {
		t.Fatal("Should fail for invalid context")
	}
}

func TestValidateDefaultContext(t *testing.T) {
	email := &EmailScalar{}
	result, err := ValidateCustomScalarWithDefault(email, "test@test.com")
	if err != nil {
		t.Fatalf("Validation with default context should succeed: %v", err)
	}
	if result != "test@test.com" {
		t.Fatalf("Expected 'test@test.com', got %v", result)
	}
}

// ========================================================================
// Error Message Tests
// ========================================================================

func TestErrorIncludesScalarName(t *testing.T) {
	email := &EmailScalar{}
	_, err := ValidateCustomScalar(email, "notanemail", "parseValue")
	if err == nil {
		t.Fatal("Should have error")
	}

	if !strings.Contains(err.Error(), "Email") {
		t.Fatalf("Error should include scalar name: %s", err.Error())
	}
}

func TestErrorIncludesContext(t *testing.T) {
	email := &EmailScalar{}
	_, err := ValidateCustomScalar(email, "notanemail", "parseValue")
	if err == nil {
		t.Fatal("Should have error")
	}

	if !strings.Contains(err.Error(), "parseValue") {
		t.Fatalf("Error should include context: %s", err.Error())
	}
}

func TestErrorIncludesMessage(t *testing.T) {
	email := &EmailScalar{}
	_, err := ValidateCustomScalar(email, "notanemail", "parseValue")
	if err == nil {
		t.Fatal("Should have error")
	}

	if !strings.Contains(err.Error(), "invalid email format") {
		t.Fatalf("Error should include message: %s", err.Error())
	}
}

func TestErrorGetters(t *testing.T) {
	email := &EmailScalar{}
	_, err := ValidateCustomScalar(email, "notanemail", "parseValue")
	if err == nil {
		t.Fatal("Should have error")
	}

	valErr, ok := err.(*ScalarValidationError)
	if !ok {
		t.Fatal("Error should be ScalarValidationError")
	}

	if valErr.ScalarName != "Email" {
		t.Fatalf("Expected scalar name 'Email', got %s", valErr.ScalarName)
	}

	if valErr.Context != "parseValue" {
		t.Fatalf("Expected context 'parseValue', got %s", valErr.Context)
	}
}

// ========================================================================
// Utility Function Tests
// ========================================================================

func TestGetAllCustomScalars(t *testing.T) {
	defer Reset()

	RegisterCustomScalar(&EmailScalar{})
	RegisterCustomScalar(&PhoneScalar{})

	scalars := GetAllCustomScalars()

	if len(scalars) != 2 {
		t.Fatalf("Expected 2 scalars, got %d", len(scalars))
	}

	if _, ok := scalars["Email"]; !ok {
		t.Fatal("Email should be in scalars")
	}

	if _, ok := scalars["Phone"]; !ok {
		t.Fatal("Phone should be in scalars")
	}
}

func TestGetAllCustomScalarsEmpty(t *testing.T) {
	defer Reset()

	scalars := GetAllCustomScalars()
	if len(scalars) != 0 {
		t.Fatalf("Expected 0 scalars, got %d", len(scalars))
	}
}

// ========================================================================
// Email Scalar Specific Tests
// ========================================================================

func TestEmailValidatesCorrect(t *testing.T) {
	email := &EmailScalar{}
	result, err := email.ParseValue("test@example.com")
	if err != nil {
		t.Fatalf("Should validate correct email: %v", err)
	}
	if result != "test@example.com" {
		t.Fatalf("Expected 'test@example.com', got %v", result)
	}
}

func TestEmailRejectsNoAt(t *testing.T) {
	email := &EmailScalar{}
	_, err := email.ParseValue("notanemail")
	if err == nil {
		t.Fatal("Should reject email without @")
	}
}

func TestEmailSerializes(t *testing.T) {
	email := &EmailScalar{}
	result, err := email.Serialize("test@example.com")
	if err != nil {
		t.Fatalf("Should serialize: %v", err)
	}
	if result != "test@example.com" {
		t.Fatalf("Expected 'test@example.com', got %v", result)
	}
}

// ========================================================================
// Phone Scalar Specific Tests
// ========================================================================

func TestPhoneValidatesE164(t *testing.T) {
	phone := &PhoneScalar{}
	result, err := phone.ParseValue("+12025551234")
	if err != nil {
		t.Fatalf("Should validate E.164 format: %v", err)
	}
	if result != "+12025551234" {
		t.Fatalf("Expected '+12025551234', got %v", result)
	}
}

func TestPhoneRequiresPlus(t *testing.T) {
	phone := &PhoneScalar{}
	_, err := phone.ParseValue("2025551234")
	if err == nil {
		t.Fatal("Should require + prefix")
	}
}

func TestPhoneValidatesDigitCount(t *testing.T) {
	phone := &PhoneScalar{}
	_, err := phone.ParseValue("+123")
	if err == nil {
		t.Fatal("Should validate digit count")
	}
}

func TestPhoneSerializes(t *testing.T) {
	phone := &PhoneScalar{}
	result, err := phone.Serialize("+12025551234")
	if err != nil {
		t.Fatalf("Should serialize: %v", err)
	}
	if result != "+12025551234" {
		t.Fatalf("Expected '+12025551234', got %v", result)
	}
}

// ========================================================================
// Schema Export Tests
// ========================================================================

func TestCustomScalarsExportedInSchema(t *testing.T) {
	defer Reset()

	RegisterCustomScalar(&EmailScalar{})
	RegisterCustomScalar(&PhoneScalar{})

	schema := GetSchema()

	if len(schema.CustomScalars) != 2 {
		t.Fatalf("Expected 2 custom scalars in schema, got %d", len(schema.CustomScalars))
	}

	// Check that scalars are in the schema
	hasEmail := false
	hasPhone := false

	for _, scalar := range schema.CustomScalars {
		name, ok := scalar["name"].(string)
		if ok {
			if name == "Email" {
				hasEmail = true
			}
			if name == "Phone" {
				hasPhone = true
			}
		}
	}

	if !hasEmail {
		t.Fatal("Email scalar should be in schema")
	}
	if !hasPhone {
		t.Fatal("Phone scalar should be in schema")
	}
}

func TestUnregisterCustomScalar(t *testing.T) {
	defer Reset()

	RegisterCustomScalar(&EmailScalar{})
	if !HasCustomScalar("Email") {
		t.Fatal("Email should be registered")
	}

	UnregisterCustomScalar("Email")
	if HasCustomScalar("Email") {
		t.Fatal("Email should be unregistered")
	}
}

func TestClearCustomScalars(t *testing.T) {
	defer Reset()

	RegisterCustomScalar(&EmailScalar{})
	RegisterCustomScalar(&PhoneScalar{})

	ClearCustomScalars()

	if len(GetAllCustomScalars()) != 0 {
		t.Fatal("All scalars should be cleared")
	}
}
