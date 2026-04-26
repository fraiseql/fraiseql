package fraiseql_test

import (
	"testing"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// Test helper to extract dispatch config from query
func getDispatchConfig(t *testing.T, queryName string) map[string]interface{} {
	t.Helper()
	schema := fraiseql.GetSchema()
	for _, q := range schema.Queries {
		if q.Name == queryName {
			// The dispatch config would be in q.Config or a separate field
			// For this test, we're verifying the builder accepts the config
			return q.Config
		}
	}
	t.Fatalf("query %s not found", queryName)
	return nil
}

func TestDispatchExplicitMapping(t *testing.T) {
	fraiseql.ClearRegistry()

	// Register enum
	fraiseql.Enum("TimeInterval", map[string]string{
		"DAY":   "day",
		"WEEK":  "week",
		"MONTH": "month",
	})

	// Register types
	type Order struct {
		ID int `fraiseql:"id,type=Int"`
	}

	fraiseql.RegisterTypes(Order{})

	// Register query with dispatch mapping
	fraiseql.NewQuery("orders").
		ReturnType(Order{}).
		ReturnsArray(true).
		SqlSourceDispatch("timeInterval", map[string]string{
			"DAY":   "tf_orders_day",
			"WEEK":  "tf_orders_week",
			"MONTH": "tf_orders_month",
		}).
		Arg("timeInterval", "TimeInterval", nil).
		Register()

	schema := fraiseql.GetSchema()
	if len(schema.Queries) == 0 {
		t.Fatal("query not registered")
	}

	q := schema.Queries[0]
	if q.Name != "orders" {
		t.Errorf("expected query name 'orders', got %s", q.Name)
	}

	// Verify dispatch config is present
	if q.Config == nil || q.Config["sql_source_dispatch"] == nil {
		t.Error("sql_source_dispatch config not found")
	}
}

func TestDispatchTemplate(t *testing.T) {
	fraiseql.ClearRegistry()

	// Register enum
	fraiseql.Enum("Environment", map[string]string{
		"STAGING":    "staging",
		"PRODUCTION": "production",
	})

	// Register type
	type User struct {
		ID int `fraiseql:"id,type=Int"`
	}

	fraiseql.RegisterTypes(User{})

	// Register query with dispatch template
	fraiseql.NewQuery("users").
		ReturnType(User{}).
		ReturnsArray(true).
		SqlSourceDispatchWithTemplate("env", "v_users_{env}").
		Arg("env", "Environment", nil).
		Register()

	schema := fraiseql.GetSchema()
	q := schema.Queries[0]

	// Verify dispatch config
	if q.Config == nil || q.Config["sql_source_dispatch"] == nil {
		t.Error("sql_source_dispatch config not found")
	}
}

func TestDispatchMutualExclusivity(t *testing.T) {
	fraiseql.ClearRegistry()

	fraiseql.Enum("Region", map[string]string{
		"US": "us",
		"EU": "eu",
	})

	type Data struct {
		ID int `fraiseql:"id,type=Int"`
	}

	fraiseql.RegisterTypes(Data{})

	// This should ideally error, but for now we just ensure both can be set
	// The compiler will validate mutual exclusivity
	fraiseql.NewQuery("data").
		ReturnType(Data{}).
		ReturnsArray(true).
		SqlSource("v_data").
		SqlSourceDispatch("region", map[string]string{
			"US": "v_us_data",
			"EU": "v_eu_data",
		}).
		Arg("region", "Region", nil).
		Register()

	schema := fraiseql.GetSchema()
	q := schema.Queries[0]

	// Both should be present - compiler validates mutual exclusivity
	// sql_source is extracted to SqlSource field by Register(); dispatch stays in Config
	if q.SqlSource == "" {
		t.Error("sql_source should be present")
	}
	if q.Config["sql_source_dispatch"] == nil {
		t.Error("sql_source_dispatch should be present")
	}
}

func TestDispatchWithOtherArguments(t *testing.T) {
	fraiseql.ClearRegistry()

	fraiseql.Enum("Shard", map[string]string{
		"S1": "shard1",
		"S2": "shard2",
	})

	type Item struct {
		ID int `fraiseql:"id,type=Int"`
	}

	fraiseql.RegisterTypes(Item{})

	// Register query with dispatch and other arguments
	fraiseql.NewQuery("items").
		ReturnType(Item{}).
		ReturnsArray(true).
		SqlSourceDispatch("shard", map[string]string{
			"S1": "t_items_s1",
			"S2": "t_items_s2",
		}).
		Arg("shard", "Shard", nil).
		Arg("limit", "Int", 10).
		Arg("offset", "Int", 0).
		Register()

	schema := fraiseql.GetSchema()
	q := schema.Queries[0]

	// Verify dispatch config
	if q.Config["sql_source_dispatch"] == nil {
		t.Error("sql_source_dispatch config not found")
	}

	// Verify other arguments are still present
	if len(q.Arguments) != 3 {
		t.Errorf("expected 3 arguments, got %d", len(q.Arguments))
	}
}

func TestDispatchConfigBuilderChaining(t *testing.T) {
	fraiseql.ClearRegistry()

	fraiseql.Enum("Type", map[string]string{
		"A": "a",
		"B": "b",
	})

	type Item struct {
		ID int `fraiseql:"id,type=Int"`
	}

	fraiseql.RegisterTypes(Item{})

	// Test that builder chaining works
	fraiseql.NewQuery("typed_items").
		ReturnType(Item{}).
		ReturnsArray(true).
		SqlSourceDispatch("type", map[string]string{
			"A": "t_items_a",
			"B": "t_items_b",
		}).
		Arg("type", "Type", nil).
		Description("Get items by type").
		Register()

	schema := fraiseql.GetSchema()
	q := schema.Queries[0]

	if q.Description != "Get items by type" {
		t.Errorf("expected description 'Get items by type', got %s", q.Description)
	}
	if q.Config["sql_source_dispatch"] == nil {
		t.Error("sql_source_dispatch not preserved after chaining")
	}
}
