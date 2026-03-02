package fraiseql

// Test helpers shared across completeness tests.
// All helpers operate on map[string]interface{} obtained by round-tripping
// through GetSchemaJSON, which mirrors what external consumers see.

import (
	"encoding/json"
	"testing"
)

// schemaMap returns the current registry as a parsed JSON map.
func schemaMap(t *testing.T) map[string]interface{} {
	t.Helper()
	data, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON: %v", err)
	}
	var m map[string]interface{}
	if err := json.Unmarshal(data, &m); err != nil {
		t.Fatalf("Unmarshal: %v", err)
	}
	return m
}

// findQuery looks up a query by name in a schema map.
func findQuery(schema map[string]interface{}, name string) map[string]interface{} {
	queries, _ := schema["queries"].([]interface{})
	for _, q := range queries {
		qm, ok := q.(map[string]interface{})
		if ok && qm["name"] == name {
			return qm
		}
	}
	return nil
}

// findMutation looks up a mutation by name in a schema map.
func findMutation(schema map[string]interface{}, name string) map[string]interface{} {
	mutations, _ := schema["mutations"].([]interface{})
	for _, m := range mutations {
		mm, ok := m.(map[string]interface{})
		if ok && mm["name"] == name {
			return mm
		}
	}
	return nil
}

// findType looks up a type by name in a schema map.
func findType(schema map[string]interface{}, name string) map[string]interface{} {
	types, _ := schema["types"].([]interface{})
	for _, tp := range types {
		tm, ok := tp.(map[string]interface{})
		if ok && tm["name"] == name {
			return tm
		}
	}
	return nil
}

// ---- Query builder method tests ----

func TestQueryBuilderSqlSourceMethod(t *testing.T) {
	Reset()
	err := NewQuery("products").ReturnType("Product").SqlSource("v_product").Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	if err := RegisterType("Product", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	q := findQuery(schemaMap(t), "products")
	if q == nil {
		t.Fatal("query 'products' not found")
	}
	if got := q["sql_source"]; got != "v_product" {
		t.Errorf("sql_source: want %q, got %v", "v_product", got)
	}
}

func TestQueryBuilderInjectParams(t *testing.T) {
	Reset()
	if err := RegisterType("Order", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("orders").
		ReturnType("Order").
		SqlSource("v_order").
		InjectParams(map[string]string{"tenant_id": "jwt:tenant_id"}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "orders")
	if q == nil {
		t.Fatal("query 'orders' not found")
	}
	inject, ok := q["inject_params"].(map[string]interface{})
	if !ok {
		t.Fatalf("inject_params not present or wrong type: %T", q["inject_params"])
	}
	param, ok := inject["tenant_id"].(map[string]interface{})
	if !ok {
		t.Fatalf("inject_params.tenant_id not present or wrong type")
	}
	if param["source"] != "jwt" {
		t.Errorf("inject_params.tenant_id.source: want %q, got %v", "jwt", param["source"])
	}
	if param["claim"] != "tenant_id" {
		t.Errorf("inject_params.tenant_id.claim: want %q, got %v", "tenant_id", param["claim"])
	}
}

func TestQueryBuilderInjectParamsMultiple(t *testing.T) {
	Reset()
	if err := RegisterType("Order", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("orderSummary").
		ReturnType("Order").
		SqlSource("v_order_summary").
		InjectParams(map[string]string{
			"user_id":   "jwt:sub",
			"tenant_id": "jwt:tenant_id",
		}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "orderSummary")
	inject, ok := q["inject_params"].(map[string]interface{})
	if !ok {
		t.Fatal("inject_params missing")
	}
	if len(inject) != 2 {
		t.Errorf("expected 2 inject_params, got %d", len(inject))
	}
	userId := inject["user_id"].(map[string]interface{})
	if userId["claim"] != "sub" {
		t.Errorf("user_id.claim: want %q, got %v", "sub", userId["claim"])
	}
}

func TestQueryBuilderCacheTTLSeconds(t *testing.T) {
	Reset()
	if err := RegisterType("Product", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("products").
		ReturnType("Product").
		SqlSource("v_product").
		CacheTTLSeconds(600).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "products")
	if got := q["cache_ttl_seconds"]; got != float64(600) {
		t.Errorf("cache_ttl_seconds: want %v, got %v", float64(600), got)
	}
}

func TestQueryBuilderCacheTTLSecondsZero(t *testing.T) {
	Reset()
	if err := RegisterType("Order", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	// Zero TTL must appear in JSON (meaning: explicitly disable caching).
	err := NewQuery("orders").
		ReturnType("Order").
		SqlSource("v_order").
		CacheTTLSeconds(0).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "orders")
	got, present := q["cache_ttl_seconds"]
	if !present {
		t.Fatal("cache_ttl_seconds should be present even when 0")
	}
	if got != float64(0) {
		t.Errorf("cache_ttl_seconds: want 0, got %v", got)
	}
}

func TestQueryBuilderCacheTTLSecondsAbsent(t *testing.T) {
	Reset()
	if err := RegisterType("Product", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("products").ReturnType("Product").SqlSource("v_product").Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "products")
	if _, present := q["cache_ttl_seconds"]; present {
		t.Error("cache_ttl_seconds should be absent when not set")
	}
}

func TestQueryBuilderAdditionalViews(t *testing.T) {
	Reset()
	if err := RegisterType("Report", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("reports").
		ReturnType("Report").
		SqlSource("v_report").
		AdditionalViews([]string{"v_report_summary"}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "reports")
	views, ok := q["additional_views"].([]interface{})
	if !ok {
		t.Fatalf("additional_views not present or wrong type: %T", q["additional_views"])
	}
	if len(views) != 1 || views[0] != "v_report_summary" {
		t.Errorf("additional_views: want [v_report_summary], got %v", views)
	}
}

func TestQueryBuilderRequiresRole(t *testing.T) {
	Reset()
	if err := RegisterType("Admin", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("adminData").
		ReturnType("Admin").
		SqlSource("v_admin").
		RequiresRole("admin").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "adminData")
	if got := q["requires_role"]; got != "admin" {
		t.Errorf("requires_role: want %q, got %v", "admin", got)
	}
}

func TestQueryBuilderDeprecation(t *testing.T) {
	Reset()
	if err := RegisterType("X", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("oldQuery").
		ReturnType("X").
		SqlSource("v_x").
		Deprecated("Use newQuery instead").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "oldQuery")
	dep, ok := q["deprecation"].(map[string]interface{})
	if !ok {
		t.Fatalf("deprecation not present or wrong type: %T", q["deprecation"])
	}
	if dep["reason"] != "Use newQuery instead" {
		t.Errorf("deprecation.reason: want %q, got %v", "Use newQuery instead", dep["reason"])
	}
}

func TestQueryBuilderRelayCursorTypeUUID(t *testing.T) {
	Reset()
	if err := RegisterType("Product", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewQuery("products").
		ReturnType("Product").
		SqlSource("v_product").
		ReturnsArray(true).
		Relay(true).
		RelayCursorColumn("id").
		RelayCursorType("uuid").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	q := findQuery(schemaMap(t), "products")
	if q["relay"] != true {
		t.Errorf("relay: want true, got %v", q["relay"])
	}
	if q["relay_cursor_column"] != "id" {
		t.Errorf("relay_cursor_column: want %q, got %v", "id", q["relay_cursor_column"])
	}
	if q["relay_cursor_type"] != "uuid" {
		t.Errorf("relay_cursor_type: want %q, got %v", "uuid", q["relay_cursor_type"])
	}
}

// ---- Mutation builder method tests ----

func TestMutationBuilderSqlSourceMethod(t *testing.T) {
	Reset()
	if err := RegisterType("User", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewMutation("createUser").
		ReturnType("User").
		SqlSource("fn_create_user").
		Operation("insert").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	m := findMutation(schemaMap(t), "createUser")
	if m == nil {
		t.Fatal("mutation 'createUser' not found")
	}
	if got := m["sql_source"]; got != "fn_create_user" {
		t.Errorf("sql_source: want %q, got %v", "fn_create_user", got)
	}
	if got := m["operation"]; got != "insert" {
		t.Errorf("operation: want %q, got %v", "insert", got)
	}
}

func TestMutationBuilderInjectParams(t *testing.T) {
	Reset()
	if err := RegisterType("Order", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewMutation("createOrder").
		ReturnType("Order").
		SqlSource("fn_create_order").
		InjectParams(map[string]string{
			"user_id":   "jwt:sub",
			"tenant_id": "jwt:org_id",
		}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	m := findMutation(schemaMap(t), "createOrder")
	inject, ok := m["inject_params"].(map[string]interface{})
	if !ok {
		t.Fatalf("inject_params missing: %T", m["inject_params"])
	}
	userId := inject["user_id"].(map[string]interface{})
	if userId["source"] != "jwt" || userId["claim"] != "sub" {
		t.Errorf("user_id inject: want {jwt, sub}, got %v", userId)
	}
	tenantId := inject["tenant_id"].(map[string]interface{})
	if tenantId["claim"] != "org_id" {
		t.Errorf("tenant_id.claim: want %q, got %v", "org_id", tenantId["claim"])
	}
}

func TestMutationBuilderInvalidatesViews(t *testing.T) {
	Reset()
	if err := RegisterType("Order", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewMutation("createOrder").
		ReturnType("Order").
		SqlSource("fn_create_order").
		InvalidatesViews([]string{"v_order_summary", "v_order_items"}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	m := findMutation(schemaMap(t), "createOrder")
	views, ok := m["invalidates_views"].([]interface{})
	if !ok {
		t.Fatalf("invalidates_views missing: %T", m["invalidates_views"])
	}
	if len(views) != 2 {
		t.Errorf("invalidates_views: want 2 entries, got %d", len(views))
	}
	if views[0] != "v_order_summary" {
		t.Errorf("invalidates_views[0]: want %q, got %v", "v_order_summary", views[0])
	}
}

func TestMutationBuilderInvalidatesFactTables(t *testing.T) {
	Reset()
	if err := RegisterType("Sale", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewMutation("recordSale").
		ReturnType("Sale").
		SqlSource("fn_record_sale").
		InvalidatesFactTables([]string{"tf_sales"}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	m := findMutation(schemaMap(t), "recordSale")
	tables, ok := m["invalidates_fact_tables"].([]interface{})
	if !ok {
		t.Fatalf("invalidates_fact_tables missing: %T", m["invalidates_fact_tables"])
	}
	if len(tables) != 1 || tables[0] != "tf_sales" {
		t.Errorf("invalidates_fact_tables: want [tf_sales], got %v", tables)
	}
}

func TestMutationBuilderDeprecation(t *testing.T) {
	Reset()
	if err := RegisterType("User", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	err := NewMutation("oldCreateUser").
		ReturnType("User").
		SqlSource("fn_old_create_user").
		Deprecated("Use createUser instead").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	m := findMutation(schemaMap(t), "oldCreateUser")
	dep, ok := m["deprecation"].(map[string]interface{})
	if !ok {
		t.Fatalf("deprecation missing: %T", m["deprecation"])
	}
	if dep["reason"] != "Use createUser instead" {
		t.Errorf("deprecation.reason: want %q, got %v", "Use createUser instead", dep["reason"])
	}
}

// ---- Type registration tests ----

func TestRegisterErrorType(t *testing.T) {
	Reset()

	err := RegisterErrorType("UserNotFound", []FieldInfo{
		{Name: "message", Type: "String", Nullable: false},
		{Name: "code", Type: "String", Nullable: false},
	}, "User was not found")
	if err != nil {
		t.Fatalf("RegisterErrorType failed: %v", err)
	}

	schema := schemaMap(t)
	tp := findType(schema, "UserNotFound")
	if tp == nil {
		t.Fatal("type 'UserNotFound' not found")
	}
	if tp["is_error"] != true {
		t.Errorf("is_error: want true, got %v", tp["is_error"])
	}
	fields, ok := tp["fields"].([]interface{})
	if !ok {
		t.Fatalf("fields missing: %T", tp["fields"])
	}
	if len(fields) != 2 {
		t.Errorf("fields: want 2, got %d", len(fields))
	}
}

func TestRegisterErrorTypeDuplicate(t *testing.T) {
	Reset()
	_ = RegisterErrorType("MyError", []FieldInfo{{Name: "message", Type: "String"}}, "")
	err := RegisterErrorType("MyError", []FieldInfo{{Name: "message", Type: "String"}}, "")
	if err == nil {
		t.Fatal("expected error for duplicate error type registration")
	}
}

func TestRegisteredTypeSqlSourceIsSnakeCase(t *testing.T) {
	Reset()

	err := RegisterType("OrderItem", []FieldInfo{{Name: "id", Type: "ID"}}, "An order item")
	if err != nil {
		t.Fatalf("RegisterType failed: %v", err)
	}

	schema := schemaMap(t)
	tp := findType(schema, "OrderItem")
	if tp == nil {
		t.Fatal("type 'OrderItem' not found")
	}
	if got := tp["sql_source"]; got != "v_order_item" {
		t.Errorf("sql_source: want %q, got %v", "v_order_item", got)
	}
}

func TestRegisteredTypeUserSqlSource(t *testing.T) {
	Reset()

	err := RegisterType("User", []FieldInfo{{Name: "id", Type: "ID"}}, "A user")
	if err != nil {
		t.Fatalf("RegisterType failed: %v", err)
	}

	tp := findType(schemaMap(t), "User")
	if got := tp["sql_source"]; got != "v_user" {
		t.Errorf("sql_source: want %q, got %v", "v_user", got)
	}
}

func TestRegisteredTypeJsonbColumnDefault(t *testing.T) {
	Reset()

	err := RegisterType("SimpleType", []FieldInfo{{Name: "id", Type: "ID"}}, "")
	if err != nil {
		t.Fatalf("RegisterType failed: %v", err)
	}

	tp := findType(schemaMap(t), "SimpleType")
	// jsonb_column is omitted from JSON when not set; "data" is the runtime default.
	if jsonbCol, ok := tp["jsonb_column"]; ok {
		if jsonbCol != "data" {
			t.Errorf("jsonb_column: want \"data\" or absent, got %v", jsonbCol)
		}
	}
	// Absence is acceptable — the runtime applies the "data" default.
}

func TestToSnakeCaseSingleWord(t *testing.T) {
	if got := toSnakeCase("User"); got != "user" {
		t.Errorf("toSnakeCase(User) = %q, want %q", got, "user")
	}
}

func TestToSnakeCaseTwoWords(t *testing.T) {
	if got := toSnakeCase("OrderItem"); got != "order_item" {
		t.Errorf("toSnakeCase(OrderItem) = %q, want %q", got, "order_item")
	}
}

func TestToSnakeCaseThreeWords(t *testing.T) {
	if got := toSnakeCase("OrderItemDetail"); got != "order_item_detail" {
		t.Errorf("toSnakeCase(OrderItemDetail) = %q, want %q", got, "order_item_detail")
	}
}
