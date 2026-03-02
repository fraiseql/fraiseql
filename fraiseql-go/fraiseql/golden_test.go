package fraiseql

// golden_test.go — Cycle 5: Golden schema comparison.
//
// Registers the same schema as golden fixture 01-basic-query-mutation.json and
// asserts that key fields in the generated JSON match the fixture.  The test
// does NOT require byte-for-byte equality because the fixture uses richer
// field types (e.g. structured "operation" objects) that the Go SDK intentionally
// represents as plain strings; it validates the fields that the SDK does produce.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// repoRoot returns the repository root by walking up from this file.
func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	// file is .../fraiseql-go/fraiseql/golden_test.go; walk up two dirs.
	return filepath.Join(filepath.Dir(file), "..", "..")
}

func TestGoldenFixture01BasicQueryMutation(t *testing.T) {
	defer Reset()
	Reset()

	// Register the User type (matches fixture 01).
	if err := RegisterType("User", []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "email", Type: "String", Nullable: false},
		{Name: "name", Type: "String", Nullable: true},
	}, "A registered user in the system"); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	// Register "users" list query.
	if err := NewQuery("users").
		ReturnType("User").
		SqlSource("v_user").
		ReturnsArray(true).
		Description("List all users with optional filtering").
		Register(); err != nil {
		t.Fatalf("Register users: %v", err)
	}

	// Register "user" single query.
	if err := NewQuery("user").
		ReturnType("User").
		SqlSource("v_user").
		Nullable(true).
		Description("Fetch a single user by ID").
		Register(); err != nil {
		t.Fatalf("Register user: %v", err)
	}

	// Register "createUser" mutation.
	if err := NewMutation("createUser").
		ReturnType("User").
		SqlSource("fn_create_user").
		Operation("insert").
		Description("Create a new user account").
		Register(); err != nil {
		t.Fatalf("Register createUser: %v", err)
	}

	// Load the golden fixture.
	goldenPath := filepath.Join(repoRoot(t), "tests", "fixtures", "golden", "01-basic-query-mutation.json")
	goldenBytes, err := os.ReadFile(goldenPath)
	if err != nil {
		t.Fatalf("read golden fixture: %v", err)
	}
	var golden map[string]interface{}
	if err := json.Unmarshal(goldenBytes, &golden); err != nil {
		t.Fatalf("unmarshal golden: %v", err)
	}

	generated := schemaMap(t)

	// Assert type fields.
	genType := findType(generated, "User")
	if genType == nil {
		t.Fatal("generated schema missing type 'User'")
	}
	goldenType := findType(golden, "User")
	if goldenType == nil {
		t.Fatal("golden fixture missing type 'User' — fixture may have changed")
	}
	if genType["name"] != goldenType["name"] {
		t.Errorf("type name: want %v, got %v", goldenType["name"], genType["name"])
	}
	if genType["sql_source"] != goldenType["sql_source"] {
		t.Errorf("type sql_source: want %v, got %v", goldenType["sql_source"], genType["sql_source"])
	}

	// Assert "users" query fields.
	genUsers := findQuery(generated, "users")
	if genUsers == nil {
		t.Fatal("generated schema missing query 'users'")
	}
	goldenUsers := findQuery(golden, "users")
	if goldenUsers == nil {
		t.Fatal("golden fixture missing query 'users'")
	}
	if genUsers["sql_source"] != goldenUsers["sql_source"] {
		t.Errorf("users.sql_source: want %v, got %v", goldenUsers["sql_source"], genUsers["sql_source"])
	}
	if genUsers["returns_list"] != goldenUsers["returns_list"] {
		t.Errorf("users.returns_list: want %v, got %v", goldenUsers["returns_list"], genUsers["returns_list"])
	}
	if genUsers["return_type"] != goldenUsers["return_type"] {
		t.Errorf("users.return_type: want %v, got %v", goldenUsers["return_type"], genUsers["return_type"])
	}

	// Assert "createUser" mutation fields.
	genCreate := findMutation(generated, "createUser")
	if genCreate == nil {
		t.Fatal("generated schema missing mutation 'createUser'")
	}
	goldenCreate := findMutation(golden, "createUser")
	if goldenCreate == nil {
		t.Fatal("golden fixture missing mutation 'createUser'")
	}
	if genCreate["sql_source"] != goldenCreate["sql_source"] {
		t.Errorf("createUser.sql_source: want %v, got %v", goldenCreate["sql_source"], genCreate["sql_source"])
	}
	if genCreate["return_type"] != goldenCreate["return_type"] {
		t.Errorf("createUser.return_type: want %v, got %v", goldenCreate["return_type"], genCreate["return_type"])
	}
}
