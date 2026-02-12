package fraiseql

import "fmt"

// CustomScalar defines the interface for custom GraphQL scalar types.
//
// Implement this interface to create custom scalars with validation logic.
// Each method represents a different validation context in GraphQL.
//
// Example:
//
//	type Email struct{}
//
//	func (e *Email) Name() string {
//	    return "Email"
//	}
//
//	func (e *Email) Serialize(value interface{}) (interface{}, error) {
//	    return value, nil
//	}
//
//	func (e *Email) ParseValue(value interface{}) (interface{}, error) {
//	    str := toString(value)
//	    if !strings.Contains(str, "@") {
//	        return nil, fmt.Errorf("invalid email format: %s", str)
//	    }
//	    return str, nil
//	}
//
//	func (e *Email) ParseLiteral(ast interface{}) (interface{}, error) {
//	    m, ok := ast.(map[string]interface{})
//	    if ok {
//	        if val, exists := m["value"]; exists {
//	            return e.ParseValue(val)
//	        }
//	    }
//	    return nil, fmt.Errorf("email literal must be string")
//	}
//
//	// Register the scalar
//	func init() {
//	    RegisterCustomScalar(&Email{})
//	}
type CustomScalar interface {
	// Name returns the GraphQL scalar type name (e.g., "Email", "Phone").
	Name() string

	// Serialize converts a database value to a GraphQL response value.
	// This is called when returning values from resolvers.
	Serialize(value interface{}) (interface{}, error)

	// ParseValue parses a variable value from GraphQL.
	// This is called when parsing variables in GraphQL operations.
	// For example: { query: getUser($email: Email) }
	ParseValue(value interface{}) (interface{}, error)

	// ParseLiteral parses a literal value from a GraphQL query string.
	// For example: { user(email: "test@example.com") }
	// The ast parameter is typically a map with a "value" key for simple types.
	ParseLiteral(ast interface{}) (interface{}, error)
}

// toString converts a value to string, handling common types.
func toString(value interface{}) string {
	switch v := value.(type) {
	case string:
		return v
	case []byte:
		return string(v)
	default:
		return fmt.Sprint(v)
	}
}
