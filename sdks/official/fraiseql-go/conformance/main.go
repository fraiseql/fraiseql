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
		// Two words and a digit segment (#1249). Go's author writes the wire name, so
		// these are already camelCase; the SDKs whose identifiers are idiomatic
		// (Python, Ruby, Elixir, C#, F#) are where the translation is exercised.
		{Name: "lastLoginAt", Type: "String", Nullable: true},
		{Name: "phone1", Type: "String", Nullable: true},
	}, "", true); err != nil {
		return err
	}
	if err := fraiseql.RegisterType("Order", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "total", Type: "Float", Nullable: false},
		{Name: "status", Type: "String", Nullable: false},
		// The column User.orders joins on, published under the naming convention.
		{Name: "fkUser", Type: "ID", Nullable: false},
	}, ""); err != nil {
		return err
	}
	// Both directions, deliberately (#1266): which join column is read off which side
	// swaps with the cardinality, so a fixture carrying only OneToMany would be uniform
	// in exactly the dimension that selects the branch. The keys name SQL *columns*
	// (fk_user) while Order publishes the field as fkUser.
	if err := fraiseql.RegisterTypeRelationships("User", fraiseql.Relationship{
		Name: "orders", TargetType: "Order", Cardinality: fraiseql.OneToMany,
		ForeignKey: "fk_user", ReferencedKey: "id",
	}); err != nil {
		return err
	}
	if err := fraiseql.RegisterTypeRelationships("Order", fraiseql.Relationship{
		Name: "user", TargetType: "User", Cardinality: fraiseql.ManyToOne,
		ForeignKey: "fk_user", ReferencedKey: "id",
	}); err != nil {
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

	// `displayName` is two words: a hand-authored input type's field names are a third
	// registration path, distinct from a type's fields and from a `crud` type's generated
	// input objects, and no fixture name had ever reached it (#1255).
	if err := fraiseql.RegisterInputType("CreateUserInput", []fraiseql.FieldInfo{
		{Name: "email", Type: "String", Nullable: false},
		{Name: "name", Type: "String", Nullable: true},
		{Name: "displayName", Type: "String", Nullable: true},
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
	// Two-word argument, deliberately (#1255): every argument in this fixture used to be
	// `id`, `email` or `name`, which spell the same in every convention, so no SDK's
	// argument-name translation was exercised and three did not have one. Go authors the
	// wire name directly, so there is nothing to translate here — the declaration exists
	// so the comparator can see the SDKs where there is.
	if err := fraiseql.NewQuery("tenantOrders").
		ReturnType("Order").ReturnsArray(true).Nullable(false).
		SqlSource("v_order").
		Arg("includeArchived", "Boolean", nil, true).
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
		Arg("displayName", "String", nil, true).
		InvalidatesViews([]string{"v_user", "v_user_summary"}).
		InvalidatesFactTables([]string{"tf_signup"}).
		// #1253: the role gate on the write side. `MutationDefinition` had no
		// `RequiresRole` at all until #1123 — `QueryDefinition` did — and no construct
		// compared the two, so the gap was found by reading rather than by a gate.
		RequiresRole("admin").
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
