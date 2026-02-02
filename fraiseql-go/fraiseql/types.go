package fraiseql

import (
	"fmt"
	"reflect"
	"strings"
	"time"
)

// FieldInfo represents metadata about a struct field
type FieldInfo struct {
	Name     string   `json:"name"`
	Type     string   `json:"type"`
	Nullable bool     `json:"nullable"`
	Scope    string   `json:"scope,omitempty"`
	Scopes   []string `json:"scopes,omitempty"`
}

// goToGraphQLType converts a Go type to GraphQL type string and nullable flag
// Examples:
//
//	int -> ("Int", false)
//	*int -> ("Int", true)
//	string -> ("String", false)
//	*string -> ("String", true)
//	[]User -> ("[User]", false)
//	*[]User -> ("[User]", true)
//	bool -> ("Boolean", false)
//	float64 -> ("Float", false)
func goToGraphQLType(goType reflect.Type) (string, bool, error) {
	nullable := false

	// Handle pointer types (nullable)
	if goType.Kind() == reflect.Pointer {
		nullable = true
		goType = goType.Elem()
	}

	// Handle slice/array types
	if goType.Kind() == reflect.Slice || goType.Kind() == reflect.Array {
		elemType := goType.Elem()
		elemGraphQLType, elemNullable, err := goToGraphQLType(elemType)
		if err != nil {
			return "", false, err
		}

		// Build list type notation
		listType := fmt.Sprintf("[%s", elemGraphQLType)
		if !elemNullable {
			listType += "!"
		}
		listType += "]"

		return listType, false, nil // Lists themselves are not nullable
	}

	// Handle basic types
	switch goType.Kind() {
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return "Int", nullable, nil
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return "Int", nullable, nil
	case reflect.Float32, reflect.Float64:
		return "Float", nullable, nil
	case reflect.String:
		return "String", nullable, nil
	case reflect.Bool:
		return "Boolean", nullable, nil
	case reflect.Struct:
		// Handle special struct types
		switch goType {
		case reflect.TypeOf(time.Time{}):
			return "String", nullable, nil
		case reflect.TypeOf(time.Duration(0)):
			return "String", nullable, nil
		default:
			// Custom struct types use their name
			return goType.Name(), nullable, nil
		}
	default:
		return "", false, fmt.Errorf("unsupported Go type: %v", goType.String())
	}
}

// ExtractFields extracts field information from a struct using reflection and struct tags
// Tag format: `fraiseql:"field_name,type=GraphQLType,nullable=true"`
// Returns map of field name -> FieldInfo
func ExtractFields(structType reflect.Type) (map[string]FieldInfo, error) {
	if structType.Kind() == reflect.Pointer {
		structType = structType.Elem()
	}

	if structType.Kind() != reflect.Struct {
		return nil, fmt.Errorf("expected struct type, got %v", structType.Kind())
	}

	fields := make(map[string]FieldInfo)
	numFields := structType.NumField()

	for i := 0; i < numFields; i++ {
		field := structType.Field(i)

		// Skip unexported fields
		if field.PkgPath != "" && !field.IsExported() {
			continue
		}

		// Skip embedded fields
		if field.Anonymous {
			continue
		}

		// Try to get fraiseql tag
		tagStr, ok := field.Tag.Lookup("fraiseql")
		if !ok {
			// If no explicit tag, infer from field name and type
			graphQLType, nullable, err := goToGraphQLType(field.Type)
			if err != nil {
				return nil, fmt.Errorf("cannot infer type for field %s: %w", field.Name, err)
			}
			fields[field.Name] = FieldInfo{
				Name:     field.Name,
				Type:     graphQLType,
				Nullable: nullable,
			}
			continue
		}

		// Parse tag: field_name,type=GraphQLType,nullable=true
		fieldInfo, err := parseFieldTag(tagStr, field.Name, field.Type)
		if err != nil {
			return nil, fmt.Errorf("invalid tag for field %s: %w", field.Name, err)
		}
		fields[fieldInfo.Name] = fieldInfo
	}

	return fields, nil
}

// parseFieldTag parses a fraiseql struct tag
// Format: fieldname,type=GraphQLType,nullable=true,scope=read:user.email,scopes=admin;auditor
func parseFieldTag(tag string, fieldName string, fieldType reflect.Type) (FieldInfo, error) {
	parts := strings.Split(tag, ",")
	if len(parts) == 0 {
		return FieldInfo{}, fmt.Errorf("empty tag")
	}

	fieldInfo := FieldInfo{
		Name: fieldName, // Default to struct field name
	}

	var hasSingleScope bool
	var hasMultipleScopes bool

	// First part can be field name override or type spec
	if parts[0] != "" && !strings.Contains(parts[0], "=") {
		fieldInfo.Name = strings.TrimSpace(parts[0])
	}

	// Parse key=value pairs
	for i := 0; i < len(parts); i++ {
		part := strings.TrimSpace(parts[i])
		if part == "" || !strings.Contains(part, "=") {
			continue
		}

		kv := strings.SplitN(part, "=", 2)
		if len(kv) != 2 {
			continue
		}

		key := strings.TrimSpace(kv[0])
		value := strings.TrimSpace(kv[1])

		switch key {
		case "type":
			fieldInfo.Type = value
		case "nullable":
			fieldInfo.Nullable = value == "true"
		case "scope":
			if value == "" {
				return FieldInfo{}, fmt.Errorf("empty scope value for field %s", fieldName)
			}
			if err := validateScope(value, fieldName); err != nil {
				return FieldInfo{}, err
			}
			fieldInfo.Scope = value
			hasSingleScope = true
		case "scopes":
			if value == "" {
				return FieldInfo{}, fmt.Errorf("empty scopes value for field %s", fieldName)
			}
			scopes := strings.Split(value, ";")
			if len(scopes) == 0 {
				return FieldInfo{}, fmt.Errorf("empty scopes array for field %s", fieldName)
			}
			for _, scope := range scopes {
				scope = strings.TrimSpace(scope)
				if scope == "" {
					return FieldInfo{}, fmt.Errorf("empty scope in scopes array for field %s", fieldName)
				}
				if err := validateScope(scope, fieldName); err != nil {
					return FieldInfo{}, err
				}
			}
			fieldInfo.Scopes = scopes
			hasMultipleScopes = true
		}
	}

	// Ensure not both scope and scopes are specified
	if hasSingleScope && hasMultipleScopes {
		return FieldInfo{}, fmt.Errorf("field %s cannot have both scope and scopes", fieldName)
	}

	// If type not specified in tag, infer it
	if fieldInfo.Type == "" {
		graphQLType, nullable, err := goToGraphQLType(fieldType)
		if err != nil {
			return FieldInfo{}, err
		}
		fieldInfo.Type = graphQLType
		// Only use inferred nullable if not explicitly set
		if !strings.Contains(tag, "nullable") {
			fieldInfo.Nullable = nullable
		}
	}

	if fieldInfo.Type == "" {
		return FieldInfo{}, fmt.Errorf("type not specified in tag")
	}

	return fieldInfo, nil
}

// validateScope validates scope format: action:resource
// Valid patterns:
// - * (global wildcard)
// - action:resource (read:user.email, write:User.salary)
// - action:* (admin:*, read:*)
func validateScope(scope string, fieldName string) error {
	if scope == "" {
		return fmt.Errorf("field %s has empty scope", fieldName)
	}

	// Global wildcard is always valid
	if scope == "*" {
		return nil
	}

	// Must contain at least one colon
	if !strings.Contains(scope, ":") {
		return fmt.Errorf("field %s has invalid scope '%s' (missing colon)", fieldName, scope)
	}

	parts := strings.SplitN(scope, ":", 2)
	if len(parts) != 2 {
		return fmt.Errorf("field %s has invalid scope '%s'", fieldName, scope)
	}

	action := parts[0]
	resource := parts[1]

	// Validate action: [a-zA-Z_][a-zA-Z0-9_]*
	if !isValidAction(action) {
		return fmt.Errorf("field %s has invalid action in scope '%s' (must be alphanumeric + underscore)", fieldName, scope)
	}

	// Validate resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
	if !isValidResource(resource) {
		return fmt.Errorf("field %s has invalid resource in scope '%s' (must be alphanumeric + underscore + dot, or *)", fieldName, scope)
	}

	return nil
}

// isValidAction validates that action matches [a-zA-Z_][a-zA-Z0-9_]*
func isValidAction(action string) bool {
	if len(action) == 0 {
		return false
	}

	// First character must be letter or underscore
	first := rune(action[0])
	if !(isLetter(first) || first == '_') {
		return false
	}

	// Rest must be letters, digits, or underscores
	for i := 1; i < len(action); i++ {
		ch := rune(action[i])
		if !(isLetter(ch) || isDigit(ch) || ch == '_') {
			return false
		}
	}

	return true
}

// isValidResource validates that resource matches [a-zA-Z_][a-zA-Z0-9_.]*|*
func isValidResource(resource string) bool {
	if resource == "*" {
		return true
	}

	if len(resource) == 0 {
		return false
	}

	// First character must be letter or underscore
	first := rune(resource[0])
	if !(isLetter(first) || first == '_') {
		return false
	}

	// Rest must be letters, digits, underscores, or dots
	for i := 1; i < len(resource); i++ {
		ch := rune(resource[i])
		if !(isLetter(ch) || isDigit(ch) || ch == '_' || ch == '.') {
			return false
		}
	}

	return true
}

func isLetter(ch rune) bool {
	return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z')
}

func isDigit(ch rune) bool {
	return ch >= '0' && ch <= '9'
}
