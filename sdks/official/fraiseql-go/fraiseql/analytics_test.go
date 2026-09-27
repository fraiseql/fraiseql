package fraiseql

import "testing"

// onlyFactTable returns the one fact table the registry emits.
func onlyFactTable(t *testing.T) map[string]interface{} {
	t.Helper()
	tables, ok := schemaMap(t)["fact_tables"].([]interface{})
	if !ok || len(tables) != 1 {
		t.Fatalf("fact_tables: want exactly one, got %v", schemaMap(t)["fact_tables"])
	}
	ft, ok := tables[0].(map[string]interface{})
	if !ok {
		t.Fatalf("fact_tables[0]: want an object, got %T", tables[0])
	}
	return ft
}

// A linked fact table emits `type_name`: the compiler reads an aggregate or window over it
// as a read of that type, under the type's field gates and role.
func TestFactTableTypeNameIsEmitted(t *testing.T) {
	Reset()
	err := NewFactTable("data").
		TableName("tf_sales").
		TypeName("Sale").
		Measure("revenue", "Float", false).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	if got := onlyFactTable(t)["type_name"]; got != "Sale" {
		t.Errorf("type_name: want %q, got %v", "Sale", got)
	}
}

// An unlinked fact table's document is what it was: no `type_name` key at all.
func TestFactTableWithoutTypeNameOmitsTheKey(t *testing.T) {
	Reset()
	err := NewFactTable("data").
		TableName("tf_sales").
		Measure("revenue", "Float", false).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	if got, present := onlyFactTable(t)["type_name"]; present {
		t.Errorf("type_name: want the key absent, got %v", got)
	}
}
