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
			EntityType:  "Order",
			Nullable:    false,
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
		if sub.EntityType != "Order" {
			t.Errorf("expected entity type 'Order', got %q", sub.EntityType)
		}
		if sub.Nullable {
			t.Error("expected nullable to be false")
		}
	})

	t.Run("subscription with topic", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:        "orderCreated",
			EntityType:  "Order",
			Nullable:    false,
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

	t.Run("subscription with operation filter", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:        "userUpdated",
			EntityType:  "User",
			Nullable:    false,
			Arguments:   []ArgumentDefinition{},
			Operation:   "UPDATE",
			Description: "Subscribe to user updates",
		})

		schema := GetSchema()
		sub := schema.Subscriptions[0]
		if sub.Operation != "UPDATE" {
			t.Errorf("expected operation 'UPDATE', got %q", sub.Operation)
		}
	})

	t.Run("subscription with arguments", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:       "orderStatusChanged",
			EntityType: "Order",
			Nullable:   false,
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

	t.Run("nullable subscription", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:        "userDeleted",
			EntityType:  "User",
			Nullable:    true,
			Arguments:   []ArgumentDefinition{},
			Description: "Subscribe to user deletions",
		})

		schema := GetSchema()
		sub := schema.Subscriptions[0]
		if !sub.Nullable {
			t.Error("expected nullable to be true")
		}
	})

	t.Run("multiple subscriptions", func(t *testing.T) {
		Reset()

		RegisterSubscription(SubscriptionDefinition{
			Name:       "orderCreated",
			EntityType: "Order",
			Nullable:   false,
			Arguments:  []ArgumentDefinition{},
		})

		RegisterSubscription(SubscriptionDefinition{
			Name:       "orderUpdated",
			EntityType: "Order",
			Nullable:   false,
			Arguments:  []ArgumentDefinition{},
		})

		RegisterSubscription(SubscriptionDefinition{
			Name:       "userCreated",
			EntityType: "User",
			Nullable:   false,
			Arguments:  []ArgumentDefinition{},
		})

		schema := GetSchema()
		if len(schema.Subscriptions) != 3 {
			t.Errorf("expected 3 subscriptions, got %d", len(schema.Subscriptions))
		}
	})
}

func TestResetClearsSubscriptions(t *testing.T) {
	Reset()

	RegisterSubscription(SubscriptionDefinition{
		Name:       "orderCreated",
		EntityType: "Order",
		Nullable:   false,
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
		EntityType:  "Order",
		Nullable:    false,
		Arguments:   []ArgumentDefinition{},
		Topic:       "orders",
		Operation:   "CREATE",
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

func TestMutationUnknownConfigKeysPreserved(t *testing.T) {
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

	var schema map[string]interface{}
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("Unmarshal failed: %v", err)
	}

	mutations := schema["mutations"].([]interface{})
	mut := mutations[0].(map[string]interface{})

	if got, ok := mut["operation"].(string); !ok || got != "create" {
		t.Errorf("expected top-level operation='create', got %v", mut["operation"])
	}
	config, hasConfig := mut["config"].(map[string]interface{})
	if !hasConfig {
		t.Fatal("expected 'config' key for unknown keys, but not found")
	}
	if config["custom_flag"] != true {
		t.Errorf("expected config.custom_flag=true, got %v", config["custom_flag"])
	}
}

func TestRESTAnnotationOnQuery(t *testing.T) {
	Reset()

	err := NewQuery("getUser").
		ReturnType("User").
		SqlSource("v_user").
		Rest("/users/{id}", "GET").
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

	rest, ok := qry["rest"].(map[string]interface{})
	if !ok {
		t.Fatalf("expected 'rest' key in query JSON, got: %v", qry)
	}
	if got, ok := rest["path"].(string); !ok || got != "/users/{id}" {
		t.Errorf("expected rest.path='/users/{id}', got %v", rest["path"])
	}
	if got, ok := rest["method"].(string); !ok || got != "GET" {
		t.Errorf("expected rest.method='GET', got %v", rest["method"])
	}
}

func TestRESTAnnotationOnMutation(t *testing.T) {
	Reset()

	err := NewMutation("createUser").
		ReturnType("User").
		SqlSource("fn_create_user").
		Rest("/users", "POST").
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

	rest, ok := mut["rest"].(map[string]interface{})
	if !ok {
		t.Fatalf("expected 'rest' key in mutation JSON, got: %v", mut)
	}
	if got, ok := rest["path"].(string); !ok || got != "/users" {
		t.Errorf("expected rest.path='/users', got %v", rest["path"])
	}
	if got, ok := rest["method"].(string); !ok || got != "POST" {
		t.Errorf("expected rest.method='POST', got %v", rest["method"])
	}
}

func TestNoRESTAnnotationOmitsField(t *testing.T) {
	Reset()

	err := NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		SqlSource("v_user").
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

	queries := schema["queries"].([]interface{})
	qry := queries[0].(map[string]interface{})

	if _, hasRest := qry["rest"]; hasRest {
		t.Error("expected no 'rest' key when REST annotation not set, but found one")
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
		def := SubscriptionDefinition{Name: "orderCreated", EntityType: "Order", Nullable: false, Arguments: []ArgumentDefinition{}}

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
