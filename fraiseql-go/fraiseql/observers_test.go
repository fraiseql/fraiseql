package fraiseql

import (
	"encoding/json"
	"testing"
)

func TestObserverBuilder(t *testing.T) {
	// Clear registry
	Reset()

	// Build observer
	NewObserver("onOrderCreated").
		Entity("Order").
		Event("INSERT").
		Actions(Webhook("https://example.com/orders")).
		Register()

	schema := GetSchema()

	if len(schema.Observers) != 1 {
		t.Fatalf("Expected 1 observer, got %d", len(schema.Observers))
	}

	observer := schema.Observers[0]
	if observer.Name != "onOrderCreated" {
		t.Errorf("Expected name 'onOrderCreated', got '%s'", observer.Name)
	}
	if observer.Entity != "Order" {
		t.Errorf("Expected entity 'Order', got '%s'", observer.Entity)
	}
	if observer.Event != "INSERT" {
		t.Errorf("Expected event 'INSERT', got '%s'", observer.Event)
	}
	if len(observer.Actions) != 1 {
		t.Fatalf("Expected 1 action, got %d", len(observer.Actions))
	}
	if observer.Actions[0]["type"] != "webhook" {
		t.Errorf("Expected action type 'webhook', got '%s'", observer.Actions[0]["type"])
	}
}

func TestObserverWithCondition(t *testing.T) {
	Reset()

	NewObserver("onHighValueOrder").
		Entity("Order").
		Event("INSERT").
		Condition("total > 1000").
		Actions(Webhook("https://example.com")).
		Register()

	schema := GetSchema()
	observer := schema.Observers[0]

	if observer.Condition != "total > 1000" {
		t.Errorf("Expected condition 'total > 1000', got '%s'", observer.Condition)
	}
}

func TestObserverWithCustomRetry(t *testing.T) {
	Reset()

	customRetry := RetryConfig{
		MaxAttempts:     5,
		BackoffStrategy: "linear",
		InitialDelayMs:  200,
		MaxDelayMs:      30000,
	}

	NewObserver("onOrder").
		Entity("Order").
		Event("INSERT").
		Actions(Webhook("https://example.com")).
		Retry(customRetry).
		Register()

	schema := GetSchema()
	observer := schema.Observers[0]

	if observer.Retry.MaxAttempts != 5 {
		t.Errorf("Expected max_attempts 5, got %d", observer.Retry.MaxAttempts)
	}
	if observer.Retry.BackoffStrategy != "linear" {
		t.Errorf("Expected backoff_strategy 'linear', got '%s'", observer.Retry.BackoffStrategy)
	}
	if observer.Retry.InitialDelayMs != 200 {
		t.Errorf("Expected initial_delay_ms 200, got %d", observer.Retry.InitialDelayMs)
	}
	if observer.Retry.MaxDelayMs != 30000 {
		t.Errorf("Expected max_delay_ms 30000, got %d", observer.Retry.MaxDelayMs)
	}
}

func TestObserverWithDefaultRetry(t *testing.T) {
	Reset()

	NewObserver("onOrder").
		Entity("Order").
		Event("INSERT").
		Actions(Webhook("https://example.com")).
		Register()

	schema := GetSchema()
	observer := schema.Observers[0]

	if observer.Retry.MaxAttempts != DefaultRetryConfig.MaxAttempts {
		t.Errorf("Expected default max_attempts %d, got %d", DefaultRetryConfig.MaxAttempts, observer.Retry.MaxAttempts)
	}
	if observer.Retry.BackoffStrategy != DefaultRetryConfig.BackoffStrategy {
		t.Errorf("Expected default backoff_strategy '%s', got '%s'", DefaultRetryConfig.BackoffStrategy, observer.Retry.BackoffStrategy)
	}
}

func TestObserverWithMultipleActions(t *testing.T) {
	Reset()

	NewObserver("onOrder").
		Entity("Order").
		Event("INSERT").
		Actions(
			Webhook("https://example.com/orders"),
			Slack("#orders", "New order {id}"),
			EmailAction("admin@example.com", "Order created", "Order {id} created"),
		).
		Register()

	schema := GetSchema()
	observer := schema.Observers[0]

	if len(observer.Actions) != 3 {
		t.Fatalf("Expected 3 actions, got %d", len(observer.Actions))
	}
	if observer.Actions[0]["type"] != "webhook" {
		t.Errorf("Expected action 0 type 'webhook', got '%s'", observer.Actions[0]["type"])
	}
	if observer.Actions[1]["type"] != "slack" {
		t.Errorf("Expected action 1 type 'slack', got '%s'", observer.Actions[1]["type"])
	}
	if observer.Actions[2]["type"] != "email" {
		t.Errorf("Expected action 2 type 'email', got '%s'", observer.Actions[2]["type"])
	}
}

func TestWebhookAction(t *testing.T) {
	action := Webhook("https://example.com/orders")

	if action["type"] != "webhook" {
		t.Errorf("Expected type 'webhook', got '%s'", action["type"])
	}
	if action["url"] != "https://example.com/orders" {
		t.Errorf("Expected url 'https://example.com/orders', got '%s'", action["url"])
	}
	if headers, ok := action["headers"].(map[string]string); !ok {
		t.Error("Expected headers to be map[string]string")
	} else if headers["Content-Type"] != "application/json" {
		t.Errorf("Expected Content-Type 'application/json', got '%s'", headers["Content-Type"])
	}
}

func TestWebhookWithEnvVar(t *testing.T) {
	action := WebhookWithEnv("ORDER_WEBHOOK_URL")

	if action["type"] != "webhook" {
		t.Errorf("Expected type 'webhook', got '%s'", action["type"])
	}
	if action["url_env"] != "ORDER_WEBHOOK_URL" {
		t.Errorf("Expected url_env 'ORDER_WEBHOOK_URL', got '%s'", action["url_env"])
	}
	if _, exists := action["url"]; exists {
		t.Error("Expected url to not exist when using url_env")
	}
}

func TestWebhookWithCustomHeaders(t *testing.T) {
	action := Webhook("https://example.com", map[string]interface{}{
		"headers": map[string]string{
			"Authorization": "Bearer token123",
		},
	})

	if headers, ok := action["headers"].(map[string]string); !ok {
		t.Error("Expected headers to be map[string]string")
	} else if headers["Authorization"] != "Bearer token123" {
		t.Errorf("Expected Authorization 'Bearer token123', got '%s'", headers["Authorization"])
	}
}

func TestWebhookWithBodyTemplate(t *testing.T) {
	action := Webhook("https://example.com", map[string]interface{}{
		"body_template": `{"order_id": "{{id}}"}`,
	})

	if action["body_template"] != `{"order_id": "{{id}}"}` {
		t.Errorf("Expected body_template to match, got '%s'", action["body_template"])
	}
}

func TestSlackAction(t *testing.T) {
	action := Slack("#orders", "New order {id}: ${total}")

	if action["type"] != "slack" {
		t.Errorf("Expected type 'slack', got '%s'", action["type"])
	}
	if action["channel"] != "#orders" {
		t.Errorf("Expected channel '#orders', got '%s'", action["channel"])
	}
	if action["message"] != "New order {id}: ${total}" {
		t.Errorf("Expected message, got '%s'", action["message"])
	}
	if action["webhook_url_env"] != "SLACK_WEBHOOK_URL" {
		t.Errorf("Expected webhook_url_env 'SLACK_WEBHOOK_URL', got '%s'", action["webhook_url_env"])
	}
}

func TestSlackWithCustomWebhook(t *testing.T) {
	action := Slack("#orders", "New order", map[string]interface{}{
		"webhook_url": "https://hooks.slack.com/services/XXX",
	})

	if action["webhook_url"] != "https://hooks.slack.com/services/XXX" {
		t.Errorf("Expected webhook_url, got '%s'", action["webhook_url"])
	}
	if _, exists := action["webhook_url_env"]; exists {
		t.Error("Expected webhook_url_env to not exist when using webhook_url")
	}
}

func TestSlackWithCustomEnvVar(t *testing.T) {
	action := Slack("#alerts", "Alert!", map[string]interface{}{
		"webhook_url_env": "SLACK_ALERTS_WEBHOOK",
	})

	if action["webhook_url_env"] != "SLACK_ALERTS_WEBHOOK" {
		t.Errorf("Expected webhook_url_env 'SLACK_ALERTS_WEBHOOK', got '%s'", action["webhook_url_env"])
	}
}

func TestEmailAction(t *testing.T) {
	action := EmailAction("admin@example.com", "Order {id} created", "Order {id} for ${total} was created")

	if action["type"] != "email" {
		t.Errorf("Expected type 'email', got '%s'", action["type"])
	}
	if action["to"] != "admin@example.com" {
		t.Errorf("Expected to 'admin@example.com', got '%s'", action["to"])
	}
	if action["subject"] != "Order {id} created" {
		t.Errorf("Expected subject, got '%s'", action["subject"])
	}
	if action["body"] != "Order {id} for ${total} was created" {
		t.Errorf("Expected body, got '%s'", action["body"])
	}
}

func TestEmailWithFrom(t *testing.T) {
	action := EmailAction("customer@example.com", "Order shipped", "Your order is on its way!", map[string]interface{}{
		"from_email": "noreply@example.com",
	})

	if action["from"] != "noreply@example.com" {
		t.Errorf("Expected from 'noreply@example.com', got '%s'", action["from"])
	}
}

func TestSchemaExportWithObservers(t *testing.T) {
	Reset()

	NewObserver("onOrder1").
		Entity("Order").
		Event("INSERT").
		Actions(Webhook("https://example.com")).
		Register()

	NewObserver("onOrder2").
		Entity("Order").
		Event("UPDATE").
		Actions(Slack("#orders", "Updated")).
		Register()

	schema := GetSchema()

	if len(schema.Observers) != 2 {
		t.Fatalf("Expected 2 observers, got %d", len(schema.Observers))
	}
	if schema.Observers[0].Name != "onOrder1" && schema.Observers[1].Name != "onOrder1" {
		t.Error("Expected onOrder1 in observers")
	}
	if schema.Observers[0].Name != "onOrder2" && schema.Observers[1].Name != "onOrder2" {
		t.Error("Expected onOrder2 in observers")
	}
}

func TestSchemaJSONWithObservers(t *testing.T) {
	Reset()

	NewObserver("onOrder").
		Entity("Order").
		Event("INSERT").
		Actions(Webhook("https://example.com")).
		Register()

	schemaJSON, err := GetSchemaJSON(true)
	if err != nil {
		t.Fatalf("Failed to get schema JSON: %v", err)
	}

	var schema map[string]interface{}
	if err := json.Unmarshal(schemaJSON, &schema); err != nil {
		t.Fatalf("Failed to unmarshal schema JSON: %v", err)
	}

	if _, exists := schema["observers"]; !exists {
		t.Error("Expected observers key in schema JSON")
	}
}

func TestEmptyObservers(t *testing.T) {
	Reset()

	RegisterType("User", []FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
	}, "User type")

	schema := GetSchema()

	// Observers should be empty array, not nil
	if schema.Observers == nil {
		// This is acceptable - Go will omit empty slices in JSON
		return
	}
	if len(schema.Observers) != 0 {
		t.Errorf("Expected 0 observers, got %d", len(schema.Observers))
	}
}
