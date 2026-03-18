package fraiseql

// generate_parity_schema_test.go — Cross-SDK parity schema generator.
//
// Writes the parity schema to the file specified by SCHEMA_OUTPUT_FILE,
// or prints it to stdout if the variable is unset.
//
// Usage:
//
//	SCHEMA_OUTPUT_FILE=/tmp/schema_go.json go test -run TestGenerateParitySchema -v ./fraiseql/

import (
	"encoding/json"
	"fmt"
	"os"
	"testing"
)

func TestGenerateParitySchema(t *testing.T) {
	Reset()

	// --- Types ---

	if err := RegisterType("User", []FieldInfo{
		{Name: "id",    Type: "ID",     Nullable: false},
		{Name: "email", Type: "String", Nullable: false},
		{Name: "name",  Type: "String", Nullable: false},
	}, ""); err != nil {
		t.Fatal(err)
	}

	if err := RegisterType("Order", []FieldInfo{
		{Name: "id",    Type: "ID",    Nullable: false},
		{Name: "total", Type: "Float", Nullable: false},
	}, ""); err != nil {
		t.Fatal(err)
	}

	if err := RegisterErrorType("UserNotFound", []FieldInfo{
		{Name: "message", Type: "String", Nullable: false},
		{Name: "code",    Type: "String", Nullable: false},
	}, ""); err != nil {
		t.Fatal(err)
	}

	// --- Queries ---

	if err := NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		Nullable(false).
		SqlSource("v_user").
		Register(); err != nil {
		t.Fatal(err)
	}

	if err := NewQuery("tenantOrders").
		ReturnType("Order").
		ReturnsArray(true).
		Nullable(false).
		SqlSource("v_order").
		InjectParams(map[string]string{"tenant_id": "jwt:tenant_id"}).
		CacheTTLSeconds(300).
		RequiresRole("admin").
		Register(); err != nil {
		t.Fatal(err)
	}

	// --- Mutations ---

	if err := NewMutation("createUser").
		ReturnType("User").
		SqlSource("fn_create_user").
		Operation("insert").
		Arg("email", "String", nil, false).
		Arg("name", "String", nil, false).
		Register(); err != nil {
		t.Fatal(err)
	}

	if err := NewMutation("placeOrder").
		ReturnType("Order").
		SqlSource("fn_place_order").
		Operation("insert").
		InjectParams(map[string]string{"user_id": "jwt:sub"}).
		InvalidatesViews([]string{"v_order_summary"}).
		InvalidatesFactTables([]string{"tf_sales"}).
		Register(); err != nil {
		t.Fatal(err)
	}

	// Emit schema as JSON
	schema := GetSchema()
	output := map[string]interface{}{
		"types":     schema.Types,
		"queries":   schema.Queries,
		"mutations": schema.Mutations,
	}
	data, err := json.MarshalIndent(output, "", "  ")
	if err != nil {
		t.Fatal(err)
	}

	if outPath := os.Getenv("SCHEMA_OUTPUT_FILE"); outPath != "" {
		if err := os.WriteFile(outPath, data, 0o644); err != nil {
			t.Fatalf("writing schema to %s: %v", outPath, err)
		}
		fmt.Printf("Schema written to %s\n", outPath)
	} else {
		fmt.Println(string(data))
	}
}
