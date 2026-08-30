// Command conformance authors the cross-SDK conformance fixture with the Go SDK's
// public API and exports it via fraiseql.ExportSchema — the exact call the README and
// all four shipped examples make.
//
// Driven by sdks/official/conformance/run.py; see sdks/official/conformance/README.md.
//
// The one rule for every SDK's copy of this file: author through the SDK, never
// hand-assemble the JSON. The pre-existing TestGenerateParitySchema went through
// RegisterType/RegisterQuery but stopped short of ExportSchema, so it never produced a
// document with an unpopulated section — which is precisely why #850 (nil slices
// marshalled to JSON null, rejected by the compiler, every shipped example dead on
// arrival) survived a green parity gate.
package main

import (
	"fmt"
	"os"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

func authorMinimal() error {
	return fraiseql.RegisterType("User", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "email", Type: "String", Nullable: false},
	}, "")
}

func authorMinimalQueries() error {
	return fraiseql.NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		Nullable(false).
		SqlSource("v_user").
		Register()
}

func authorFull() error {
	if err := fraiseql.RegisterType("User", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "email", Type: "String", Nullable: false},
		{
			Name:        "name",
			Type:        "String",
			Nullable:    true,
			Description: `The user's "display" name`,
			Deprecated:  &fraiseql.DeprecationInfo{Reason: "use displayName"},
		},
		{Name: "salary", Type: "Float", Nullable: true, Scope: "read:User.salary"},
	}, "", true); err != nil {
		return err
	}
	if err := fraiseql.RegisterType("Order", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "total", Type: "Float", Nullable: false},
		{Name: "status", Type: "String", Nullable: false},
	}, ""); err != nil {
		return err
	}
	if err := fraiseql.RegisterErrorType("UserNotFound", []fraiseql.FieldInfo{
		{Name: "message", Type: "String", Nullable: false},
		{Name: "code", Type: "String", Nullable: false},
	}, ""); err != nil {
		return err
	}

	if err := fraiseql.RegisterType("Document", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "embedding", Type: "Vector", Nullable: false,
			Vector: fraiseql.NewVectorConfig(1536).WithIndex(fraiseql.IndexIVFFlat).WithMetric(fraiseql.MetricL2)},
		{Name: "fingerprint", Type: "BitVector", Nullable: false,
			Vector: fraiseql.NewVectorConfig(768).WithMetric(fraiseql.MetricHamming)},
		{Name: "compact", Type: "HalfVector", Nullable: true,
			Vector: fraiseql.NewVectorConfig(1536).WithMetric(fraiseql.MetricInnerProduct)},
		{Name: "terms", Type: "SparseVector", Nullable: true,
			Vector: fraiseql.NewVectorConfig(30000).WithIndex(fraiseql.IndexNone)},
		{Name: "similarity", Type: "Float", Nullable: false, VectorDistance: "embedding"},
	}, ""); err != nil {
		return err
	}

	if err := fraiseql.RegisterInputType("CreateUserInput", []fraiseql.FieldInfo{
		{Name: "email", Type: "String", Nullable: false},
		{Name: "name", Type: "String", Nullable: true},
	}, ""); err != nil {
		return err
	}

	fraiseql.Enum("OrderStatus", "PENDING", "SHIPPED", "CANCELLED")

	if err := fraiseql.NewQuery("users").
		ReturnType("User").ReturnsArray(true).Nullable(false).
		SqlSource("v_user").Register(); err != nil {
		return err
	}
	if err := fraiseql.NewQuery("user").
		ReturnType("User").ReturnsArray(false).Nullable(true).
		SqlSource("v_user").
		Arg("id", "ID", nil, false).
		Register(); err != nil {
		return err
	}
	if err := fraiseql.NewQuery("tenantOrders").
		ReturnType("Order").ReturnsArray(true).Nullable(false).
		SqlSource("v_order").
		InjectParams(map[string]string{"tenant_id": "jwt:tenant_id"}).
		CacheTTLSeconds(300).
		RequiresRole("admin").
		// #966's actor allow-list, enforced in the same executor gate as RequiresRole on
		// every transport, and authorable in no SDK until #1123.
		RequiresActor(fraiseql.ActorHumanUser, fraiseql.ActorServiceAccount).
		Register(); err != nil {
		return err
	}

	if err := fraiseql.NewMutation("createUser").
		ReturnType("User").Nullable(false).
		SqlSource("fn_create_user").
		Operation("insert").
		Arg("email", "String", nil, false).
		Arg("name", "String", nil, true).
		InvalidatesViews([]string{"v_user", "v_user_summary"}).
		InvalidatesFactTables([]string{"tf_signup"}).
		RequiresActor(fraiseql.ActorServiceAccount).
		Register(); err != nil {
		return err
	}
	if err := fraiseql.NewMutation("placeOrder").
		ReturnType("Order").Nullable(false).
		SqlSource("fn_place_order").
		Operation("insert").
		InjectParams(map[string]string{"user_id": "jwt:sub"}).
		InvalidatesViews([]string{"v_order_summary"}).
		InvalidatesFactTables([]string{"tf_sale"}).
		Register(); err != nil {
		return err
	}

	return fraiseql.RegisterSubscription(fraiseql.SubscriptionDefinition{
		Name:        "orderUpdated",
		ReturnType:  "Order",
		Arguments:   []fraiseql.ArgumentDefinition{{Name: "orderId", Type: "ID", Nullable: true}},
		Description: "Stream of order update events",
		Topic:       "order_events",
		Filter: &fraiseql.SubscriptionFilter{
			Conditions: []fraiseql.SubscriptionFilterCondition{{Argument: "orderId", Path: "$.id"}},
		},
		Fields: []string{"id", "total"},
	})
}

func author(fixture string) error {
	switch fixture {
	case "minimal":
		if err := authorMinimal(); err != nil {
			return err
		}
		return authorMinimalQueries()
	case "full":
		return authorFull()
	default:
		return fmt.Errorf("unknown fixture %q", fixture)
	}
}

func main() {
	fixture := os.Getenv("FRAISEQL_CONFORMANCE_FIXTURE")
	out := os.Getenv("FRAISEQL_CONFORMANCE_OUT")
	if fixture == "" || out == "" {
		fmt.Fprintln(os.Stderr, "FRAISEQL_CONFORMANCE_FIXTURE and FRAISEQL_CONFORMANCE_OUT must be set")
		os.Exit(2)
	}

	fraiseql.Reset()
	if err := author(fixture); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := fraiseql.ExportSchema(out); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
