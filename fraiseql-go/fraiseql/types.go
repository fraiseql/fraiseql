package fraiseql

import (
	"fmt"
	"reflect"
	"strings"
	"time"
)

// FieldInfo represents metadata about a struct field
type FieldInfo struct {
	Name     string `json:"name"`
	Type     string `json:"type"`
	Nullable bool   `json:"nullable"`
}

// goToGraphQLType converts a Go type to GraphQL type string and nullable flag
// Examples:
//   int -> ("Int", false)
//   *int -> ("Int", true)
//   string -> ("String", false)
//   *string -> ("String", true)
//   []User -> ("[User]", false)
//   *[]User -> ("[User]", true)
//   bool -> ("Boolean", false)
//   float64 -> ("Float", false)
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
// Format: fieldname,type=GraphQLType,nullable=true
func parseFieldTag(tag string, fieldName string, fieldType reflect.Type) (FieldInfo, error) {
	parts := strings.Split(tag, ",")
	if len(parts) == 0 {
		return FieldInfo{}, fmt.Errorf("empty tag")
	}

	fieldInfo := FieldInfo{
		Name: fieldName, // Default to struct field name
	}

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
		}
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
