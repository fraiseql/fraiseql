package fraiseql

import (
	"encoding/json"
	"reflect"
	"testing"
)

// exportedAction is an action as the exported schema carries it.
func exportedAction(t *testing.T, action ObserverAction) map[string]interface{} {
	t.Helper()
	raw, err := json.Marshal(action)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var out map[string]interface{}
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	return out
}

func TestSlackEmitsTheKeysTheRuntimeReads(t *testing.T) {
	got := exportedAction(t, Slack("#orders", "New order {{ id }}"))
	want := map[string]interface{}{
		"type":             "slack",
		"channel":          "#orders",
		"message_template": "New order {{ id }}",
		"webhook_url_env":  "SLACK_WEBHOOK_URL",
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestSlackWithAnExplicitURLNamesNoDefaultVariable(t *testing.T) {
	got := exportedAction(t, Slack("#orders", "Hi", map[string]interface{}{
		"webhook_url": "https://hooks.slack.com/services/XXX",
	}))
	if got["webhook_url"] != "https://hooks.slack.com/services/XXX" {
		t.Errorf("webhook_url = %v", got["webhook_url"])
	}
	if _, ok := got["webhook_url_env"]; ok {
		t.Errorf("webhook_url_env should be absent, got %v", got["webhook_url_env"])
	}
}

func TestEmailEmitsTheKeysTheRuntimeReads(t *testing.T) {
	got := exportedAction(t, EmailAction("ops@example.com", "New order", "Order {{ id }}"))
	want := map[string]interface{}{
		"type":          "email",
		"to":            "ops@example.com",
		"subject":       "New order",
		"body_template": "Order {{ id }}",
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestWebhookEmitsTheKeysTheRuntimeReads(t *testing.T) {
	got := exportedAction(t, WebhookWithEnv("ORDER_HOOK_URL",
		WebhookMethod("PUT"),
		WebhookSigningSecretEnv("ORDER_HOOK_SECRET"),
	))
	want := map[string]interface{}{
		"type":               "webhook",
		"url_env":            "ORDER_HOOK_URL",
		"method":             "PUT",
		"signing_secret_env": "ORDER_HOOK_SECRET",
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestWebhookCarriesAHeaderValueFromTheEnvironmentByName(t *testing.T) {
	got := exportedAction(t, Webhook("https://example.com/hook",
		WebhookHeaderFromEnv("Authorization", "ORDER_HOOK_TOKEN"),
	))
	want := map[string]interface{}{
		"type":        "webhook",
		"url":         "https://example.com/hook",
		"headers_env": map[string]interface{}{"Authorization": "ORDER_HOOK_TOKEN"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestWebhookOptionsSetEveryKeyTheRuntimeReads(t *testing.T) {
	got := exportedAction(t, Webhook("https://example.com/hook",
		WebhookMethod("PATCH"),
		WebhookBodyTemplate(`{"id": "{{ id }}"}`),
		WebhookSigningSecretEnv("ORDER_HOOK_SECRET"),
		WebhookHeader("X-Source", "orders"),
		WebhookHeader("X-Tenant", "acme"),
		WebhookHeaderFromEnv("Authorization", "ORDER_HOOK_TOKEN"),
		WebhookHeaderFromEnv("X-Api-Key", "ORDER_HOOK_KEY"),
	))
	want := map[string]interface{}{
		"type":               "webhook",
		"url":                "https://example.com/hook",
		"method":             "PATCH",
		"body_template":      `{"id": "{{ id }}"}`,
		"signing_secret_env": "ORDER_HOOK_SECRET",
		"headers":            map[string]interface{}{"X-Source": "orders", "X-Tenant": "acme"},
		"headers_env": map[string]interface{}{
			"Authorization": "ORDER_HOOK_TOKEN",
			"X-Api-Key":     "ORDER_HOOK_KEY",
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

// A webhook given no option authors no optional key: the server applies its own
// defaults, and an SDK that wrote them would author what the author did not.
func TestWebhookWithoutOptionsAuthorsOnlyItsURL(t *testing.T) {
	got := exportedAction(t, WebhookWithEnv("ORDER_HOOK_URL"))
	want := map[string]interface{}{"type": "webhook", "url_env": "ORDER_HOOK_URL"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestRegisteredObserverIsExportedWithItsRetryPolicy(t *testing.T) {
	Reset()
	defer Reset()

	err := NewObserver("order_created").
		Entity("Order").
		Event("INSERT").
		Actions(
			Webhook("https://example.com/orders"),
			Slack("#orders", "New order {{ id }}"),
			EmailAction("ops@example.com", "New order", "Order {{ id }}"),
		).
		Register()
	if err != nil {
		t.Fatalf("Register: %v", err)
	}

	raw, err := ExportSchemaRaw(false)
	if err != nil {
		t.Fatalf("export: %v", err)
	}
	var schema struct {
		Observers []struct {
			Name    string                   `json:"name"`
			Actions []map[string]interface{} `json:"actions"`
			Retry   map[string]interface{}   `json:"retry"`
		} `json:"observers"`
	}
	if err := json.Unmarshal(raw, &schema); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(schema.Observers) != 1 || len(schema.Observers[0].Actions) != 3 {
		t.Fatalf("observers = %+v", schema.Observers)
	}
	wantRetry := map[string]interface{}{
		"max_attempts":     float64(3),
		"backoff_strategy": "exponential",
		"initial_delay_ms": float64(1000),
		"max_delay_ms":     float64(30000),
	}
	if !reflect.DeepEqual(schema.Observers[0].Retry, wantRetry) {
		t.Errorf("retry = %v, want %v", schema.Observers[0].Retry, wantRetry)
	}
}
