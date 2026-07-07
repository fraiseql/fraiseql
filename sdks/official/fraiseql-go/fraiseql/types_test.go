package fraiseql

import (
	"reflect"
	"testing"
	"time"
)

func TestGoToGraphQLType(t *testing.T) {
	tests := []struct {
		name         string
		goType       reflect.Type
		expectedType string
		expectedNull bool
		shouldError  bool
	}{
		{
			name:         "int",
			goType:       reflect.TypeOf(0),
			expectedType: "Int",
			expectedNull: false,
		},
		{
			name:         "pointer to int",
			goType:       reflect.TypeOf((*int)(nil)),
			expectedType: "Int",
			expectedNull: true,
		},
		{
			name:         "int32",
			goType:       reflect.TypeOf(int32(0)),
			expectedType: "Int",
			expectedNull: false,
		},
		{
			name:         "int64",
			goType:       reflect.TypeOf(int64(0)),
			expectedType: "Int",
			expectedNull: false,
		},
		{
			name:         "float64",
			goType:       reflect.TypeOf(0.0),
			expectedType: "Float",
			expectedNull: false,
		},
		{
			name:         "pointer to float64",
			goType:       reflect.TypeOf((*float64)(nil)),
			expectedType: "Float",
			expectedNull: true,
		},
		{
			name:         "string",
			goType:       reflect.TypeOf(""),
			expectedType: "String",
			expectedNull: false,
		},
		{
			name:         "pointer to string",
			goType:       reflect.TypeOf((*string)(nil)),
			expectedType: "String",
			expectedNull: true,
		},
		{
			name:         "bool",
			goType:       reflect.TypeOf(false),
			expectedType: "Boolean",
			expectedNull: false,
		},
		{
			name:         "pointer to bool",
			goType:       reflect.TypeOf((*bool)(nil)),
			expectedType: "Boolean",
			expectedNull: true,
		},
		{
			name:         "time.Time",
			goType:       reflect.TypeOf(time.Time{}),
			expectedType: "String",
			expectedNull: false,
		},
		{
			name:         "slice of int",
			goType:       reflect.TypeOf([]int{}),
			expectedType: "[Int!]",
			expectedNull: false,
		},
		{
			name:         "slice of strings",
			goType:       reflect.TypeOf([]string{}),
			expectedType: "[String!]",
			expectedNull: false,
		},
		{
			name:         "slice of nullable strings",
			goType:       reflect.TypeOf([]*string{}),
			expectedType: "[String]",
			expectedNull: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			graphQLType, nullable, err := goToGraphQLType(tt.goType)

			if (err != nil) != tt.shouldError {
				t.Errorf("unexpected error: %v", err)
			}

			if graphQLType != tt.expectedType {
				t.Errorf("expected type %q, got %q", tt.expectedType, graphQLType)
			}

			if nullable != tt.expectedNull {
				t.Errorf("expected nullable %v, got %v", tt.expectedNull, nullable)
			}
		})
	}
}

type testUserType struct {
	ID        int
	Name      string
	Email     string
	CreatedAt time.Time
}

type testPostType struct {
	ID        int       `fraiseql:"id,type=Int"`
	Title     string    `fraiseql:"title,type=String"`
	Content   string    `fraiseql:"content,type=String"`
	Published bool      `fraiseql:"published,type=Boolean"`
	CreatedAt time.Time `fraiseql:"createdAt,type=String"`
}

type testNullableFieldsType struct {
	ID       int     `fraiseql:"id,type=Int"`
	Name     *string `fraiseql:"name,type=String,nullable=true"`
	Email    *string `fraiseql:"email,type=String,nullable=true"`
	IsActive *bool   `fraiseql:"isActive,type=Boolean,nullable=true"`
}

func TestExtractFields(t *testing.T) {
	tests := []struct {
		name          string
		input         interface{}
		expectedCount int
		checkField    func(t *testing.T, fields map[string]FieldInfo)
	}{
		{
			name:          "basic struct",
			input:         testUserType{},
			expectedCount: 4,
			checkField: func(t *testing.T, fields map[string]FieldInfo) {
				if fields["ID"].Type != "Int" {
					t.Errorf("expected ID type Int, got %s", fields["ID"].Type)
				}
				if fields["ID"].Nullable {
					t.Error("expected ID to not be nullable")
				}
				if fields["Name"].Type != "String" {
					t.Errorf("expected Name type String, got %s", fields["Name"].Type)
				}
				if fields["CreatedAt"].Type != "String" {
					t.Errorf("expected CreatedAt type String, got %s", fields["CreatedAt"].Type)
				}
			},
		},
		{
			name:          "struct with explicit tags",
			input:         testPostType{},
			expectedCount: 5,
			checkField: func(t *testing.T, fields map[string]FieldInfo) {
				if fields["id"].Type != "Int" {
					t.Errorf("expected id type Int, got %s", fields["id"].Type)
				}
				if fields["title"].Type != "String" {
					t.Errorf("expected title type String, got %s", fields["title"].Type)
				}
				if fields["published"].Type != "Boolean" {
					t.Errorf("expected published type Boolean, got %s", fields["published"].Type)
				}
			},
		},
		{
			name:          "struct with nullable fields",
			input:         testNullableFieldsType{},
			expectedCount: 4,
			checkField: func(t *testing.T, fields map[string]FieldInfo) {
				if !fields["name"].Nullable {
					t.Error("expected name to be nullable")
				}
				if !fields["email"].Nullable {
					t.Error("expected email to be nullable")
				}
				if !fields["isActive"].Nullable {
					t.Error("expected isActive to be nullable")
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			structType := reflect.TypeOf(tt.input)
			fields, err := ExtractFields(structType)
			if err != nil {
				t.Fatalf("ExtractFields failed: %v", err)
			}

			if len(fields) != tt.expectedCount {
				t.Errorf("expected %d fields, got %d", tt.expectedCount, len(fields))
			}

			if tt.checkField != nil {
				tt.checkField(t, fields)
			}
		})
	}
}

func TestExtractFieldsPointerType(t *testing.T) {
	// Test that passing a pointer to a struct works
	structType := reflect.TypeOf((*testUserType)(nil))
	fields, err := ExtractFields(structType)
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if len(fields) != 4 {
		t.Errorf("expected 4 fields, got %d", len(fields))
	}

	if fields["ID"].Type != "Int" {
		t.Errorf("expected ID type Int, got %s", fields["ID"].Type)
	}
}

func TestParseFieldTag(t *testing.T) {
	tests := []struct {
		name        string
		tag         string
		fieldName   string
		fieldType   reflect.Type
		expected    FieldInfo
		shouldError bool
	}{
		{
			name:      "simple tag with type override",
			tag:       "customName,type=String",
			fieldName: "Name",
			fieldType: reflect.TypeOf(""),
			expected: FieldInfo{
				Name:     "customName",
				Type:     "String",
				Nullable: false,
			},
		},
		{
			name:      "tag with nullable",
			tag:       "email,type=String,nullable=true",
			fieldName: "Email",
			fieldType: reflect.TypeOf(""),
			expected: FieldInfo{
				Name:     "email",
				Type:     "String",
				Nullable: true,
			},
		},
		{
			name:      "tag with spaces",
			tag:       "id , type = Int , nullable = false",
			fieldName: "ID",
			fieldType: reflect.TypeOf(0),
			expected: FieldInfo{
				Name:     "id",
				Type:     "Int",
				Nullable: false,
			},
		},
		{
			name:      "tag without field name override",
			tag:       "type=Boolean",
			fieldName: "IsActive",
			fieldType: reflect.TypeOf(false),
			expected: FieldInfo{
				Name:     "IsActive",
				Type:     "Boolean",
				Nullable: false,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := parseFieldTag(tt.tag, tt.fieldName, tt.fieldType)

			if (err != nil) != tt.shouldError {
				t.Errorf("unexpected error: %v", err)
			}

			if result.Name != tt.expected.Name {
				t.Errorf("expected name %q, got %q", tt.expected.Name, result.Name)
			}

			if result.Type != tt.expected.Type {
				t.Errorf("expected type %q, got %q", tt.expected.Type, result.Type)
			}

			if result.Nullable != tt.expected.Nullable {
				t.Errorf("expected nullable %v, got %v", tt.expected.Nullable, result.Nullable)
			}
		})
	}
}

// --- Entity-identity contract (ADR-0017): a field named "id" is emitted as "ID" ---

// testIdStringType exercises the untagged reflection path: an exported Go `ID`
// field (which is emitted verbatim as the GraphQL field name) typed `string`
// must be canonicalized to GraphQL "ID", while a non-id string field stays
// "String" and a numeric id stays "Int".
type testIdStringType struct {
	ID       string  // untagged string id -> "ID"
	Nickname *string // nullable non-id string -> "String", nullable preserved
	Name     string  // non-id string -> "String"
}

type testIdNullableStringType struct {
	ID *string // untagged nullable string id -> "ID", nullable preserved
}

type testIdIntType struct {
	ID int // numeric id stays "Int"
}

func TestCanonicalizeIdType(t *testing.T) {
	tests := []struct {
		name      string
		fieldName string
		inputType string
		expected  string
	}{
		{name: "lowercase id string -> ID", fieldName: "id", inputType: "String", expected: "ID"},
		{name: "lowercase id UUID -> ID", fieldName: "id", inputType: "UUID", expected: "ID"},
		{name: "go-cased ID string -> ID", fieldName: "ID", inputType: "String", expected: "ID"},
		{name: "mixed-case Id string -> ID", fieldName: "Id", inputType: "String", expected: "ID"},
		{name: "id Int stays Int", fieldName: "id", inputType: "Int", expected: "Int"},
		{name: "non-id string stays String", fieldName: "name", inputType: "String", expected: "String"},
		{name: "authorId string stays String", fieldName: "authorId", inputType: "String", expected: "String"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := canonicalizeIdType(tt.fieldName, tt.inputType); got != tt.expected {
				t.Errorf("canonicalizeIdType(%q, %q) = %q, want %q", tt.fieldName, tt.inputType, got, tt.expected)
			}
		})
	}
}

func TestExtractFieldsEntityIdentityUntagged(t *testing.T) {
	fields, err := ExtractFields(reflect.TypeOf(testIdStringType{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["ID"].Type != "ID" {
		t.Errorf("expected untagged string id to be canonicalized to ID, got %q", fields["ID"].Type)
	}
	if fields["ID"].Nullable {
		t.Error("expected non-pointer id to be non-nullable")
	}
	if fields["Name"].Type != "String" {
		t.Errorf("expected non-id string field to stay String, got %q", fields["Name"].Type)
	}
	if fields["Nickname"].Type != "String" {
		t.Errorf("expected non-id nullable string to stay String, got %q", fields["Nickname"].Type)
	}
	if !fields["Nickname"].Nullable {
		t.Error("expected nullable non-id field to stay nullable")
	}
}

func TestExtractFieldsEntityIdentityNullableId(t *testing.T) {
	fields, err := ExtractFields(reflect.TypeOf(testIdNullableStringType{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["ID"].Type != "ID" {
		t.Errorf("expected nullable string id to be canonicalized to ID, got %q", fields["ID"].Type)
	}
	if !fields["ID"].Nullable {
		t.Error("expected nullability to be preserved on a *string id")
	}
}

func TestExtractFieldsEntityIdentityNumericId(t *testing.T) {
	fields, err := ExtractFields(reflect.TypeOf(testIdIntType{}))
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	if fields["ID"].Type != "Int" {
		t.Errorf("expected numeric id to stay Int, got %q", fields["ID"].Type)
	}
}

func TestParseFieldTagEntityIdentity(t *testing.T) {
	tests := []struct {
		name         string
		tag          string
		fieldName    string
		fieldType    reflect.Type
		expectedType string
	}{
		{
			name:         "tagged id inferred string -> ID",
			tag:          "id",
			fieldName:    "ID",
			fieldType:    reflect.TypeOf(""),
			expectedType: "ID",
		},
		{
			name:         "tagged id explicit String -> ID",
			tag:          "id,type=String",
			fieldName:    "ID",
			fieldType:    reflect.TypeOf(""),
			expectedType: "ID",
		},
		{
			name:         "tagged id explicit UUID -> ID",
			tag:          "id,type=UUID",
			fieldName:    "ID",
			fieldType:    reflect.TypeOf(""),
			expectedType: "ID",
		},
		{
			name:         "tagged id explicit Int stays Int",
			tag:          "id,type=Int",
			fieldName:    "ID",
			fieldType:    reflect.TypeOf(0),
			expectedType: "Int",
		},
		{
			name:         "tagged non-id string stays String",
			tag:          "authorId,type=String",
			fieldName:    "AuthorID",
			fieldType:    reflect.TypeOf(""),
			expectedType: "String",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := parseFieldTag(tt.tag, tt.fieldName, tt.fieldType)
			if err != nil {
				t.Fatalf("parseFieldTag failed: %v", err)
			}
			if result.Type != tt.expectedType {
				t.Errorf("expected type %q, got %q", tt.expectedType, result.Type)
			}
		})
	}
}

func TestExtractFieldsNonStruct(t *testing.T) {
	// Should return error for non-struct types
	structType := reflect.TypeOf(123)
	_, err := ExtractFields(structType)
	if err == nil {
		t.Error("expected error for non-struct type")
	}
}

func TestExtractFieldsUnexportedFields(t *testing.T) {
	type testPrivateType struct {
		ID   int
		name string
	}
	_ = testPrivateType{name: "unused"} // reference field to satisfy staticcheck U1000

	structType := reflect.TypeOf(testPrivateType{})
	fields, err := ExtractFields(structType)
	if err != nil {
		t.Fatalf("ExtractFields failed: %v", err)
	}

	// Should only have ID field
	if len(fields) != 1 {
		t.Errorf("expected 1 field, got %d", len(fields))
	}

	if _, ok := fields["ID"]; !ok {
		t.Error("expected ID field")
	}
}
