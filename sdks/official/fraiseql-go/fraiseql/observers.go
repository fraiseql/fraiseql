package fraiseql

import "fmt"

// ObserverAction represents a single action to execute when an observer fires.
type ObserverAction struct {
	Type   string                 `json:"type"`
	Config map[string]interface{} `json:"config"`
}

// RetryConfig controls retry behaviour for observer actions.
type RetryConfig struct {
	MaxAttempts     int    `json:"max_attempts"`
	BackoffStrategy string `json:"backoff_strategy"`
	InitialDelayMs  int    `json:"initial_delay_ms"`
	MaxDelayMs      int    `json:"max_delay_ms"`
}

// ObserverDefinition represents a database event observer.
type ObserverDefinition struct {
	Name      string           `json:"name"`
	Entity    string           `json:"entity"`
	Event     string           `json:"event"`
	Condition string           `json:"condition,omitempty"`
	Actions   []ObserverAction `json:"actions"`
	Retry     *RetryConfig     `json:"retry,omitempty"`
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
	reg.observers[b.name] = ObserverDefinition{
		Name:      b.name,
		Entity:    b.entity,
		Event:     b.event,
		Condition: b.condition,
		Actions:   b.actions,
		Retry:     b.retry,
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
