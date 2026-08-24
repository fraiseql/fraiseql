package fraiseql

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestRegisterSubscription(t *testing.T) {
	// Reset registry before each test
	Reset()

	t.Run("simple subscription", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:        "orderCreated",
			ReturnType:  "Order",
			Arguments:   []ArgumentDefinition{},
			Description: "Subscribe to new orders",
		})

		schema := GetSchema()
		if len(schema.Subscriptions) != 1 {
			t.Errorf("expected 1 subscription, got %d", len(schema.Subscriptions))
		}

		sub := schema.Subscriptions[0]
		if sub.Name != "orderCreated" {
			t.Errorf("expected name 'orderCreated', got %q", sub.Name)
		}
		if sub.ReturnType != "Order" {
			t.Errorf("expected return type 'Order', got %q", sub.ReturnType)
		}
	})

	t.Run("subscription with topic", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:        "orderCreated",
			ReturnType:  "Order",
			Arguments:   []ArgumentDefinition{},
			Topic:       "order_events",
			Description: "Subscribe to new orders",
		})

		schema := GetSchema()
		sub := schema.Subscriptions[0]
		if sub.Topic != "order_events" {
			t.Errorf("expected topic 'order_events', got %q", sub.Topic)
		}
	})

	// Replaces "subscription with operation filter". A subscription does not filter on
	// a DML verb — the runtime has no such member — it maps its own arguments onto JSON
	// paths in the event payload.
	t.Run("subscription with event filter", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:       "userUpdated",
			ReturnType: "User",
			Arguments: []ArgumentDefinition{
				{Name: "userId", Type: "ID", Nullable: true},
			},
			Filter: &SubscriptionFilter{
				Conditions: []SubscriptionFilterCondition{
					{Argument: "userId", Path: "$.id"},
				},
			},
			Description: "Subscribe to user updates",
		})

		schema := GetSchema()
		sub := schema.Subscriptions[0]
		if sub.Filter == nil || len(sub.Filter.Conditions) != 1 {
			t.Fatalf("expected 1 filter condition, got %+v", sub.Filter)
		}
		if sub.Filter.Conditions[0].Path != "$.id" {
			t.Errorf("expected path '$.id', got %q", sub.Filter.Conditions[0].Path)
		}
	})

	t.Run("subscription with arguments", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:       "orderStatusChanged",
			ReturnType: "Order",
			Arguments: []ArgumentDefinition{
				{Name: "userId", Type: "String", Nullable: true},
				{Name: "status", Type: "String", Nullable: true},
			},
			Description: "Subscribe to order status changes",
		})

		schema := GetSchema()
		sub := schema.Subscriptions[0]
		if len(sub.Arguments) != 2 {
			t.Errorf("expected 2 arguments, got %d", len(sub.Arguments))
		}
		if sub.Arguments[0].Name != "userId" {
			t.Errorf("expected first argument name 'userId', got %q", sub.Arguments[0].Name)
		}
		if !sub.Arguments[0].Nullable {
			t.Error("expected first argument to be nullable")
		}
	})

	// Replaces "nullable subscription". `fields` projects a subset of the event.
	t.Run("subscription projecting event fields", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:        "userDeleted",
			ReturnType:  "User",
			Arguments:   []ArgumentDefinition{},
			Fields:      []string{"id"},
			Description: "Subscribe to user deletions",
		})

		schema := GetSchema()
		sub := schema.Subscriptions[0]
		if len(sub.Fields) != 1 || sub.Fields[0] != "id" {
			t.Errorf("expected fields [id], got %v", sub.Fields)
		}
	})

	t.Run("multiple subscriptions", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:       "orderCreated",
			ReturnType: "Order",
			Arguments:  []ArgumentDefinition{},
		})

		RegisterSubscription(SubscriptionDefinition{
			Name:       "orderUpdated",
			ReturnType: "Order",
			Arguments:  []ArgumentDefinition{},
		})

		RegisterSubscription(SubscriptionDefinition{
			Name:       "userCreated",
			ReturnType: "User",
			Arguments:  []ArgumentDefinition{},
		})

		schema := GetSchema()
		if len(schema.Subscriptions) != 3 {
			t.Errorf("expected 3 subscriptions, got %d", len(schema.Subscriptions))
		}
	})
}

// TestSubscriptionJSONHasOnlyCompilerMembers pins the emitted key set.
//
// `IntermediateSubscription` denies unknown fields, so one extra key fails the *whole
// document* at `fraiseql compile`. Nothing here used to check that — every assertion
// above read the struct back through Go, where `entity_type`/`nullable`/`operation`
// round-tripped perfectly and compiled nowhere (#1024).
func TestSubscriptionJSONHasOnlyCompilerMembers(t *testing.T) {
	Reset()

	RegisterSubscription(SubscriptionDefinition{
		Name:       "orderUpdated",
		ReturnType: "Order",
		Arguments: []ArgumentDefinition{
			{Name: "orderId", Type: "ID", Nullable: true},
		},
		Description: "Stream of order update events",
		Topic:       "order_events",
		Filter: &SubscriptionFilter{
			Conditions: []SubscriptionFilterCondition{{Argument: "orderId", Path: "$.id"}},
		},
		Fields:     []string{"id", "total"},
		Deprecated: &DeprecationInfo{Reason: "use orderEvents"},
	})

	data, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}

	var schema map[string]interface{}
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("Unmarshal failed: %v", err)
	}

	subs, ok := schema["subscriptions"].([]interface{})
	if !ok || len(subs) != 1 {
		t.Fatalf("expected 1 subscription in JSON, got schema: %s", string(data))
	}
	sub, ok := subs[0].(map[string]interface{})
	if !ok {
		t.Fatalf("subscription is not an object: %s", string(data))
	}

	allowed := map[string]bool{
		"name": true, "return_type": true, "arguments": true, "description": true,
		"topic": true, "filter": true, "fields": true, "deprecated": true,
	}
	for key := range sub {
		if !allowed[key] {
			t.Errorf("emitted key %q is not a member of IntermediateSubscription", key)
		}
	}
	if _, present := sub["return_type"]; !present {
		t.Errorf("return_type absent from emitted subscription: %s", string(data))
	}
}

func TestSubscriptionJSONOmitsUnsetOptions(t *testing.T) {
	Reset()

	RegisterSubscription(SubscriptionDefinition{
		Name:       "orderCreated",
		ReturnType: "Order",
		Arguments:  []ArgumentDefinition{},
	})

	data, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}

	var schema map[string]interface{}
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("Unmarshal failed: %v", err)
	}

	sub := schema["subscriptions"].([]interface{})[0].(map[string]interface{})
	for _, key := range []string{"description", "topic", "filter", "fields", "deprecated"} {
		if _, present := sub[key]; present {
			t.Errorf("unset option %q was emitted: %s", key, string(data))
		}
	}
}

func TestResetClearsSubscriptions(t *testing.T) {
	Reset()

	RegisterSubscription(SubscriptionDefinition{
		Name:       "orderCreated",
		ReturnType: "Order",
		Arguments:  []ArgumentDefinition{},
	})

	schema := GetSchema()
	if len(schema.Subscriptions) != 1 {
		t.Errorf("expected 1 subscription before reset, got %d", len(schema.Subscriptions))
	}

	Reset()

	schema = GetSchema()
	if len(schema.Subscriptions) != 0 {
		t.Errorf("expected 0 subscriptions after reset, got %d", len(schema.Subscriptions))
	}
}

func TestGetSchemaIncludesSubscriptions(t *testing.T) {
	Reset()

	// Register a type
	RegisterType("Order", []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "total", Type: "Float", Nullable: false},
	}, "An order")

	// Register a query
	RegisterQuery(QueryDefinition{
		Name:        "orders",
		ReturnType:  "Order",
		ReturnsList: true,
		Nullable:    false,
		Arguments:   []ArgumentDefinition{},
	})

	// Register a subscription
	RegisterSubscription(SubscriptionDefinition{
		Name:        "orderCreated",
		ReturnType:  "Order",
		Arguments:   []ArgumentDefinition{},
		Topic:       "orders",
		Description: "Subscribe to new orders",
	})

	schema := GetSchema()

	// Verify all components are present
	if len(schema.Types) != 1 {
		t.Errorf("expected 1 type, got %d", len(schema.Types))
	}
	if len(schema.Queries) != 1 {
		t.Errorf("expected 1 query, got %d", len(schema.Queries))
	}
	if len(schema.Subscriptions) != 1 {
		t.Errorf("expected 1 subscription, got %d", len(schema.Subscriptions))
	}
}

func TestMutationConfigFieldsTopLevel(t *testing.T) {
	Reset()

	err := NewMutation("createUser").
		ReturnType("User").
		Config(map[string]interface{}{
			"operation":  "create",
			"sql_source": "user",
		}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	data, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}

	var schema map[string]interface{}
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("Unmarshal failed: %v", err)
	}

	mutations, ok := schema["mutations"].([]interface{})
	if !ok || len(mutations) != 1 {
		t.Fatalf("expected 1 mutation in JSON, got schema: %s", string(data))
	}

	mut, ok := mutations[0].(map[string]interface{})
	if !ok {
		t.Fatal("mutation is not a JSON object")
	}

	if got, ok := mut["operation"].(string); !ok || got != "create" {
		t.Errorf("expected top-level operation='create', got %v", mut["operation"])
	}
	if got, ok := mut["sql_source"].(string); !ok || got != "user" {
		t.Errorf("expected top-level sql_source='user', got %v", mut["sql_source"])
	}
	if _, hasConfig := mut["config"]; hasConfig {
		t.Error("expected no 'config' key when all config keys are known, but found one")
	}
}

func TestQueryConfigFieldsTopLevel(t *testing.T) {
	Reset()

	err := NewQuery("getUser").
		ReturnType("User").
		Config(map[string]interface{}{
			"sql_source": "user",
		}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	data, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}

	var schema map[string]interface{}
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("Unmarshal failed: %v", err)
	}

	queries, ok := schema["queries"].([]interface{})
	if !ok || len(queries) != 1 {
		t.Fatalf("expected 1 query in JSON, got schema: %s", string(data))
	}

	qry, ok := queries[0].(map[string]interface{})
	if !ok {
		t.Fatal("query is not a JSON object")
	}

	if got, ok := qry["sql_source"].(string); !ok || got != "user" {
		t.Errorf("expected top-level sql_source='user', got %v", qry["sql_source"])
	}
	if _, hasConfig := qry["config"]; hasConfig {
		t.Error("expected no 'config' key when all config keys are known, but found one")
	}
}

func TestConfigIsNotSerialized(t *testing.T) {
	Reset()

	err := NewMutation("createUser").
		ReturnType("User").
		Config(map[string]interface{}{
			"operation":   "create",
			"custom_flag": true,
		}).
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	data, err := GetSchemaJSON(false)
	if err != nil {
		t.Fatalf("GetSchemaJSON failed: %v", err)
	}

	// `config` is an SDK-internal bag. The compiler has no such key and, denying unknown
	// fields, rejects the whole schema when it sees one — so a query or mutation that
	// carried any unrecognized Config entry made the export uncompilable. Recognized
	// entries (`operation` here) are lifted onto their own keys; the rest stay in Go.
	if strings.Contains(string(data), "\"config\"") {
		t.Errorf("`config` must not be serialized — the compiler denies it: %s", data)
	}
	if !strings.Contains(string(data), "\"operation\":\"create\"") {
		t.Errorf("a recognized config entry must still reach its own key: %s", data)
	}
}

func TestDuplicateRegistrationErrors(t *testing.T) {
	t.Run("RegisterType returns error for duplicate", func(t *testing.T) {
		Reset()
		fields := []FieldInfo{{Name: "id", Type: "ID", Nullable: false}}

		if err := RegisterType("User", fields, ""); err != nil {
			t.Fatalf("first registration should succeed, got: %v", err)
		}
		err := RegisterType("User", fields, "duplicate")
		if err == nil {
			t.Fatal("expected error for duplicate type registration, got nil")
		}
		if !strings.Contains(err.Error(), "already registered") {
			t.Errorf("error should mention 'already registered', got: %v", err)
		}
	})

	t.Run("RegisterQuery returns error for duplicate", func(t *testing.T) {
		Reset()
		def := QueryDefinition{Name: "getUser", ReturnType: "User", ReturnsList: false, Nullable: true}

		if err := RegisterQuery(def); err != nil {
			t.Fatalf("first registration should succeed, got: %v", err)
		}
		err := RegisterQuery(def)
		if err == nil {
			t.Fatal("expected error for duplicate query registration, got nil")
		}
		if !strings.Contains(err.Error(), "already registered") {
			t.Errorf("error should mention 'already registered', got: %v", err)
		}
	})

	t.Run("RegisterMutation returns error for duplicate", func(t *testing.T) {
		Reset()
		def := MutationDefinition{Name: "createUser", ReturnType: "User", ReturnsList: false, Nullable: false}

		if err := RegisterMutation(def); err != nil {
			t.Fatalf("first registration should succeed, got: %v", err)
		}
		err := RegisterMutation(def)
		if err == nil {
			t.Fatal("expected error for duplicate mutation registration, got nil")
		}
		if !strings.Contains(err.Error(), "already registered") {
			t.Errorf("error should mention 'already registered', got: %v", err)
		}
	})

	t.Run("RegisterSubscription returns error for duplicate", func(t *testing.T) {
		Reset()
		def := SubscriptionDefinition{Name: "orderCreated", ReturnType: "Order", Arguments: []ArgumentDefinition{}}

		if err := RegisterSubscription(def); err != nil {
			t.Fatalf("first registration should succeed, got: %v", err)
		}
		err := RegisterSubscription(def)
		if err == nil {
			t.Fatal("expected error for duplicate subscription registration, got nil")
		}
		if !strings.Contains(err.Error(), "already registered") {
			t.Errorf("error should mention 'already registered', got: %v", err)
		}
	})
}
