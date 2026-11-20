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

// ObserverDefinition represents a database event observer.
//
// Retry is nil when the author calls no Retry(): the block is then left out, and the
// compiler gives the observer the policy the server's observer runtime runs when it is
// given none.
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
//
// An action is checked where the schema is compiled, against the type the server's
// observer runtime reads: `fraiseql compile` refuses a key the runtime would ignore.
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

// WebhookOption sets one optional key of a webhook action. Pass any number to [Webhook]
// or [WebhookWithEnv]; a webhook given none authors only its URL.
type WebhookOption func(cfg map[string]interface{})

// WebhookMethod sets the HTTP method. The server sends POST when it is not set.
func WebhookMethod(method string) WebhookOption {
	return func(cfg map[string]interface{}) { cfg["method"] = method }
}

// WebhookBodyTemplate sets the request body: `{{ field }}` placeholders name fields of
// the observed entity by their stored key. Without it the server sends the event as JSON.
func WebhookBodyTemplate(template string) WebhookOption {
	return func(cfg map[string]interface{}) { cfg["body_template"] = template }
}

// WebhookSigningSecretEnv signs each request with the HMAC key held in the environment
// variable envVar. The key itself is never authored: an exported schema is a build
// artifact.
func WebhookSigningSecretEnv(envVar string) WebhookOption {
	return func(cfg map[string]interface{}) { cfg["signing_secret_env"] = envVar }
}

// WebhookHeader adds a header whose value is written into the schema. Use it for a value
// that is not a secret: the compiler refuses a credential header written here. A
// credential goes through [WebhookHeaderFromEnv].
func WebhookHeader(name, value string) WebhookOption {
	return func(cfg map[string]interface{}) { headerMap(cfg, "headers")[name] = value }
}

// WebhookHeaderFromEnv adds a header whose value the server reads from the environment
// variable envVar at delivery. Use it for a credential (Authorization, an API key).
func WebhookHeaderFromEnv(name, envVar string) WebhookOption {
	return func(cfg map[string]interface{}) { headerMap(cfg, "headers_env")[name] = envVar }
}

// headerMap is the header map stored under key, created on first use.
func headerMap(cfg map[string]interface{}, key string) map[string]string {
	headers, ok := cfg[key].(map[string]string)
	if !ok {
		headers = map[string]string{}
		cfg[key] = headers
	}
	return headers
}

// Webhook creates a webhook observer action sending to url.
func Webhook(url string, opts ...WebhookOption) ObserverAction {
	return webhook(map[string]interface{}{"url": url}, opts)
}

// WebhookWithEnv creates a webhook observer action whose URL the server reads from the
// environment variable envVar at delivery.
func WebhookWithEnv(envVar string, opts ...WebhookOption) ObserverAction {
	return webhook(map[string]interface{}{"url_env": envVar}, opts)
}

func webhook(cfg map[string]interface{}, opts []WebhookOption) ObserverAction {
	for _, opt := range opts {
		opt(cfg)
	}
	return ObserverAction{Type: "webhook", Config: cfg}
}

const DefaultSlackWebhookEnv = "SLACK_WEBHOOK_URL"

// SlackOption sets where a Slack action posts. With none, [Slack] reads the URL from
// [DefaultSlackWebhookEnv]; with several, the last one applies.
type SlackOption func(cfg map[string]interface{})

// SlackWebhookURL posts to the incoming-webhook url, written into the schema. It
// replaces a destination an earlier option set, [SlackWebhookURLEnv] included: an
// action has one destination.
func SlackWebhookURL(url string) SlackOption {
	return func(cfg map[string]interface{}) {
		delete(cfg, "webhook_url_env")
		cfg["webhook_url"] = url
	}
}

// SlackWebhookURLEnv posts to the incoming-webhook URL the server reads from the
// environment variable envVar at delivery. It replaces a destination an earlier option
// set, [SlackWebhookURL] included: an action has one destination.
func SlackWebhookURLEnv(envVar string) SlackOption {
	return func(cfg map[string]interface{}) {
		delete(cfg, "webhook_url")
		cfg["webhook_url_env"] = envVar
	}
}

// Slack creates a Slack notification observer action.
//
// The message is a template with `{{ field }}` placeholders, emitted as
// `message_template`. The incoming-webhook URL is read from [DefaultSlackWebhookEnv]
// unless an option names another destination; when several do, the last one applies.
func Slack(channel, message string, opts ...SlackOption) ObserverAction {
	cfg := map[string]interface{}{
		"channel":          channel,
		"message_template": message,
		"webhook_url_env":  DefaultSlackWebhookEnv,
	}
	for _, opt := range opts {
		opt(cfg)
	}
	return ObserverAction{Type: "slack", Config: cfg}
}

// EmailAction creates an email observer action.
//
// The body is a template with `{{ field }}` placeholders, emitted as `body_template`.
// The sender is the server's SMTP configuration; an action cannot set one.
func EmailAction(to, subject, body string) ObserverAction {
	return ObserverAction{
		Type: "email",
		Config: map[string]interface{}{
			"to":            to,
			"subject":       subject,
			"body_template": body,
		},
	}
}
