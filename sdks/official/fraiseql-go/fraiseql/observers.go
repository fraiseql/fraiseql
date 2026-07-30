package fraiseql

import (
	"encoding/json"
	"fmt"
)

// ObserverAction represents a single action to execute when an observer fires.
//
// Config is kept as a separate field for ergonomics but is **flattened** on the wire:
// the compiler reads an action's settings from the action object itself
// (`{"type": "webhook", "url": "..."}`), not from a nested `config`. Nesting them meant
// the validator saw a webhook action with no `url`, so every shipped Go observer example
// failed with `webhook action must have 'url' or 'url_env'` — describing an omission the
// author had not made.
type ObserverAction struct {
	Type   string                 `json:"type"`
	Config map[string]interface{} `json:"-"`
}

// MarshalJSON flattens Config's entries alongside `type`.
//
// A config key named `type` is refused rather than allowed to overwrite the action kind:
// silently replacing it would produce an action of a different type than the author wrote.
func (a ObserverAction) MarshalJSON() ([]byte, error) {
	flat := make(map[string]interface{}, len(a.Config)+1)
	for k, v := range a.Config {
		if k == "type" {
			return nil, fmt.Errorf("observer action config may not contain a %q key; it is the action kind", "type")
		}
		flat[k] = v
	}
	flat["type"] = a.Type
	return json.Marshal(flat)
}

// RetryConfig controls retry behaviour for observer actions.
type RetryConfig struct {
	MaxAttempts     int    `json:"max_attempts"`
	BackoffStrategy string `json:"backoff_strategy"`
	InitialDelayMs  int    `json:"initial_delay_ms"`
	MaxDelayMs      int    `json:"max_delay_ms"`
}

// DefaultRetryConfig is what an observer that does not call Retry() is compiled with.
//
// `IntermediateObserver.retry` is a required field with no serde default, so omitting the
// block failed the compile with `missing field `retry`` — and the builder omitted it
// whenever the author had not set one, which is the common case. Every shipped Go
// observer example was uncompilable for this reason alone.
func DefaultRetryConfig() RetryConfig {
	return RetryConfig{
		MaxAttempts:     3,
		BackoffStrategy: "exponential",
		InitialDelayMs:  1000,
		MaxDelayMs:      30000,
	}
}

// ObserverDefinition represents a database event observer.
//
// Retry is a value, not a pointer, and carries no `omitempty`: the compiler requires the
// block, so an absent one is a compile error rather than a default.
type ObserverDefinition struct {
	Name      string           `json:"name"`
	Entity    string           `json:"entity"`
	Event     string           `json:"event"`
	Condition string           `json:"condition,omitempty"`
	Actions   []ObserverAction `json:"actions"`
	Retry     RetryConfig      `json:"retry"`
}

// ObserverBuilder provides a fluent interface for building observer definitions.
type ObserverBuilder struct {
	name      string
	entity    string
	event     string
	condition string
	actions   []ObserverAction
	retry     *RetryConfig
}

// NewObserver creates a new observer builder with the given name.
func NewObserver(name string) *ObserverBuilder {
	return &ObserverBuilder{
		name:    name,
		actions: []ObserverAction{},
	}
}

// Entity sets the entity type this observer watches.
func (b *ObserverBuilder) Entity(entity string) *ObserverBuilder {
	b.entity = entity
	return b
}

// Event sets the database event that triggers this observer (INSERT, UPDATE, DELETE).
func (b *ObserverBuilder) Event(event string) *ObserverBuilder {
	b.event = event
	return b
}

// Condition sets an optional filter expression for the observer.
func (b *ObserverBuilder) Condition(cond string) *ObserverBuilder {
	b.condition = cond
	return b
}

// Actions appends one or more actions to execute when the observer fires.
func (b *ObserverBuilder) Actions(actions ...ObserverAction) *ObserverBuilder {
	b.actions = append(b.actions, actions...)
	return b
}

// Action appends a single action to execute when the observer fires.
func (b *ObserverBuilder) Action(action ObserverAction) *ObserverBuilder {
	b.actions = append(b.actions, action)
	return b
}

// Retry sets the retry configuration for this observer's actions.
func (b *ObserverBuilder) Retry(cfg RetryConfig) *ObserverBuilder {
	b.retry = &cfg
	return b
}

// Register registers the observer with the global schema registry.
// Returns an error if an observer with the same name is already registered.
func (b *ObserverBuilder) Register() error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.observers[b.name]; exists {
		return fmt.Errorf("observer %q is already registered; each name must be unique within a schema", b.name)
	}
	retry := DefaultRetryConfig()
	if b.retry != nil {
		retry = *b.retry
	}
	reg.observers[b.name] = ObserverDefinition{
		Name:      b.name,
		Entity:    b.entity,
		Event:     b.event,
		Condition: b.condition,
		Actions:   b.actions,
		Retry:     retry,
	}
	return nil
}

// Webhook creates a webhook observer action.
// The first argument is the URL. An optional second argument provides extra
// configuration (headers, body_template, etc.).
func Webhook(url string, opts ...map[string]interface{}) ObserverAction {
	cfg := map[string]interface{}{
		"url": url,
	}
	if len(opts) > 0 {
		for k, v := range opts[0] {
			cfg[k] = v
		}
	}
	return ObserverAction{Type: "webhook", Config: cfg}
}

// WebhookWithEnv creates a webhook observer action whose URL is read from
// the named environment variable at runtime.
func WebhookWithEnv(envVar string) ObserverAction {
	return ObserverAction{
		Type: "webhook",
		Config: map[string]interface{}{
			"url_env": envVar,
		},
	}
}

// Slack creates a Slack notification observer action.
func Slack(channel, message string) ObserverAction {
	return ObserverAction{
		Type: "slack",
		Config: map[string]interface{}{
			"channel": channel,
			"message": message,
		},
	}
}

// EmailAction creates an email observer action.
// An optional fourth argument provides extra configuration (from_email, etc.).
func EmailAction(to, subject, body string, opts ...map[string]interface{}) ObserverAction {
	cfg := map[string]interface{}{
		"to":      to,
		"subject": subject,
		"body":    body,
	}
	if len(opts) > 0 {
		for k, v := range opts[0] {
			cfg[k] = v
		}
	}
	return ObserverAction{Type: "email", Config: cfg}
}
