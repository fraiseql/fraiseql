package fraiseql

// ObserverAction represents an action to execute when an observer is triggered
type ObserverAction map[string]interface{}

// RetryConfig represents retry configuration for observer actions
type RetryConfig struct {
	MaxAttempts      int    `json:"max_attempts"`
	BackoffStrategy  string `json:"backoff_strategy"`
	InitialDelayMs   int    `json:"initial_delay_ms"`
	MaxDelayMs       int    `json:"max_delay_ms"`
}

// DefaultRetryConfig provides sensible retry defaults
var DefaultRetryConfig = RetryConfig{
	MaxAttempts:     3,
	BackoffStrategy: "exponential",
	InitialDelayMs:  100,
	MaxDelayMs:      60000,
}

// ObserverDefinition represents an observer definition
type ObserverDefinition struct {
	Name      string           `json:"name"`
	Entity    string           `json:"entity"`
	Event     string           `json:"event"`
	Actions   []ObserverAction `json:"actions"`
	Condition string           `json:"condition,omitempty"`
	Retry     RetryConfig      `json:"retry"`
}

// ObserverBuilder provides a fluent interface for building observers
type ObserverBuilder struct {
	name      string
	entity    string
	event     string
	actions   []ObserverAction
	condition string
	retry     RetryConfig
}

// NewObserver creates a new observer builder
func NewObserver(name string) *ObserverBuilder {
	return &ObserverBuilder{
		name:    name,
		retry:   DefaultRetryConfig,
		actions: []ObserverAction{},
	}
}

// Entity sets the entity type to observe
func (ob *ObserverBuilder) Entity(entity string) *ObserverBuilder {
	ob.entity = entity
	return ob
}

// Event sets the event type (INSERT, UPDATE, DELETE)
func (ob *ObserverBuilder) Event(event string) *ObserverBuilder {
	ob.event = event
	return ob
}

// Action adds an action to execute
func (ob *ObserverBuilder) Action(action ObserverAction) *ObserverBuilder {
	ob.actions = append(ob.actions, action)
	return ob
}

// Actions sets multiple actions at once
func (ob *ObserverBuilder) Actions(actions ...ObserverAction) *ObserverBuilder {
	ob.actions = actions
	return ob
}

// Condition sets the condition expression
func (ob *ObserverBuilder) Condition(condition string) *ObserverBuilder {
	ob.condition = condition
	return ob
}

// Retry sets the retry configuration
func (ob *ObserverBuilder) Retry(retry RetryConfig) *ObserverBuilder {
	ob.retry = retry
	return ob
}

// Register registers the observer with the global schema registry
func (ob *ObserverBuilder) Register() {
	definition := ObserverDefinition{
		Name:      ob.name,
		Entity:    ob.entity,
		Event:     ob.event,
		Actions:   ob.actions,
		Condition: ob.condition,
		Retry:     ob.retry,
	}

	RegisterObserver(definition)
}

// Webhook creates a webhook action
func Webhook(url string, options ...map[string]interface{}) ObserverAction {
	action := ObserverAction{
		"type":    "webhook",
		"headers": map[string]string{"Content-Type": "application/json"},
	}

	if url != "" {
		action["url"] = url
	}

	// Apply options
	if len(options) > 0 {
		opts := options[0]
		if urlEnv, ok := opts["url_env"].(string); ok {
			action["url_env"] = urlEnv
			delete(action, "url") // Use url_env instead
		}
		if headers, ok := opts["headers"].(map[string]string); ok {
			action["headers"] = headers
		}
		if bodyTemplate, ok := opts["body_template"].(string); ok {
			action["body_template"] = bodyTemplate
		}
	}

	return action
}

// WebhookWithEnv creates a webhook action using an environment variable
func WebhookWithEnv(urlEnv string, options ...map[string]interface{}) ObserverAction {
	opts := map[string]interface{}{"url_env": urlEnv}
	if len(options) > 0 {
		for k, v := range options[0] {
			opts[k] = v
		}
	}
	return Webhook("", opts)
}

// Slack creates a Slack notification action
func Slack(channel string, message string, options ...map[string]interface{}) ObserverAction {
	action := ObserverAction{
		"type":             "slack",
		"channel":          channel,
		"message":          message,
		"webhook_url_env":  "SLACK_WEBHOOK_URL",
	}

	// Apply options
	if len(options) > 0 {
		opts := options[0]
		if webhookURL, ok := opts["webhook_url"].(string); ok {
			action["webhook_url"] = webhookURL
			delete(action, "webhook_url_env")
		}
		if webhookURLEnv, ok := opts["webhook_url_env"].(string); ok {
			action["webhook_url_env"] = webhookURLEnv
		}
	}

	return action
}

// EmailAction creates an email action
// Note: Named EmailAction to avoid conflict with Email scalar type
func EmailAction(to string, subject string, body string, options ...map[string]interface{}) ObserverAction {
	action := ObserverAction{
		"type":    "email",
		"to":      to,
		"subject": subject,
		"body":    body,
	}

	// Apply options
	if len(options) > 0 {
		opts := options[0]
		if fromEmail, ok := opts["from_email"].(string); ok {
			action["from"] = fromEmail
		}
	}

	return action
}
