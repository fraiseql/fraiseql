package fraiseql

import (
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
