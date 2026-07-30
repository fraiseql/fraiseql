package fraiseql_test

import (
	"strings"
	"testing"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

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
	err := fraiseql.NewQuery("orders").
		ReturnType(Order{}).
		ReturnsArray(true).
		SqlSourceDispatch("timeInterval", map[string]string{
			"DAY":   "tf_orders_day",
			"WEEK":  "tf_orders_week",
			"MONTH": "tf_orders_month",
		}).
		Arg("timeInterval", "TimeInterval", nil).
		Register()

	// `sql_source_dispatch` has no consumer anywhere in the compiler — not in the
	// intermediate schema, not in the converter, not in the compiled artifact. It used
	// to be emitted under `config`, which made the whole schema uncompilable
	// (`unknown field \`config\``); dropping it silently would be worse, since the author
	// would get a clean compile and a query that never dispatches.
	if err == nil {
		t.Fatal("SqlSourceDispatch must refuse: the compiler implements no dynamic source selection")
	}
	if !strings.Contains(err.Error(), "SqlSourceDispatch") {
		t.Errorf("the error must name the offending setting, got: %v", err)
	}
	if len(fraiseql.GetSchema().Queries) != 0 {
		t.Error("a refused query must not be registered")
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
	err := fraiseql.NewQuery("users").
		ReturnType(User{}).
		ReturnsArray(true).
		SqlSourceDispatchWithTemplate("env", "v_users_{env}").
		Arg("env", "Environment", nil).
		Register()

	// See TestDispatchExplicitMapping: refused rather than emitted, because the setting
	// has no consumer and emitting it made the schema uncompilable.
	if err == nil {
		t.Fatal("SqlSourceDispatchWithTemplate must refuse: the compiler implements no dynamic source selection")
	}
	if len(fraiseql.GetSchema().Queries) != 0 {
		t.Error("a refused query must not be registered")
	}
}

// The remaining cases all exercised `sql_source_dispatch` surviving into `Config`. It
// does not survive anywhere useful: no part of the compiler reads it, and emitting it
// under `config` made the whole schema uncompilable. One test of the refusal replaces
// four tests of a setting that never did anything.
func TestDispatchRefusalIsIndependentOfOtherSettings(t *testing.T) {
	fraiseql.ClearRegistry()

	fraiseql.Enum("Region", map[string]string{"US": "us", "EU": "eu"})

	type Data struct {
		ID int `fraiseql:"id,type=Int"`
	}
	fraiseql.RegisterTypes(Data{})

	err := fraiseql.NewQuery("data").
		ReturnType(Data{}).
		ReturnsArray(true).
		SqlSource("v_data").
		SqlSourceDispatch("region", map[string]string{"US": "v_us_data", "EU": "v_eu_data"}).
		Arg("region", "Region", nil).
		Register()

	if err == nil {
		t.Fatal("SqlSourceDispatch must refuse even when a static SqlSource is also set")
	}
	if !strings.Contains(err.Error(), "data") {
		t.Errorf("the error must name the query, got: %v", err)
	}
	if len(fraiseql.GetSchema().Queries) != 0 {
		t.Error("a refused query must not be registered")
	}
}
