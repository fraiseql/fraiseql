package fraiseql

import "fmt"

// FunctionPredicate is one `when` conjunct (#597) the dispatcher evaluates on the
// row images before firing a function. Exactly one operator may be set, and
// ChangedTo is UPDATE-only.
type FunctionPredicate struct {
	Field     string      `json:"field"`
	Eq        interface{} `json:"eq,omitempty"`
	ChangedTo interface{} `json:"changed_to,omitempty"`
}

// FunctionRunAs is the least-privilege authority ceiling a function's
// fraiseql_query writes run under (#594). Absent means fail-closed.
type FunctionRunAs struct {
	Roles  []string `json:"roles,omitempty"`
	Scopes []string `json:"scopes,omitempty"`
	Tenant string   `json:"tenant,omitempty"`
}

// FunctionDefinition is a serverless function (#1325) — an out-of-band handler the
// server dispatches on a trigger and runs in a WASM or Deno sandbox.
//
// Name is also the module FILE STEM: the server loads
// <module_dir>/<name>.<ext>, so it is carried verbatim and never recased.
// ModuleDir and DlqStore are deliberately absent — they are deployment settings
// owned by [functions] in fraiseql.toml.
type FunctionDefinition struct {
	Name       string              `json:"name"`
	Trigger    string              `json:"trigger"`
	Runtime    string              `json:"runtime"`
	TimeoutMs  *int                `json:"timeout_ms,omitempty"`
	RunAs      *FunctionRunAs      `json:"run_as,omitempty"`
	When       []FunctionPredicate `json:"when,omitempty"`
	ReRunnable bool                `json:"re_runnable,omitempty"`
	Retry      *RetryConfig        `json:"retry,omitempty"`
}

// FunctionBuilder provides a fluent interface for building function definitions,
// mirroring ObserverBuilder.
type FunctionBuilder struct {
	name       string
	trigger    string
	runtime    string
	timeoutMs  *int
	runAs      *FunctionRunAs
	when       []FunctionPredicate
	reRunnable bool
	retry      *RetryConfig
}

// NewFunction starts building a function definition.
//
// The name is the module file stem as well as the function's identity, so pass it
// exactly as the file is named (`notify_approved` → `notify_approved.ts`).
func NewFunction(name string) *FunctionBuilder {
	return &FunctionBuilder{name: name, runtime: "Deno"}
}

// Trigger sets the trigger string, e.g. "after:mutation:Order:update".
// after:mutation matches the mutation's RETURN TYPE, not its name.
func (b *FunctionBuilder) Trigger(trigger string) *FunctionBuilder {
	b.trigger = trigger
	return b
}

// Runtime sets the sandbox that runs the module: "Deno" (default) or "Wasm".
func (b *FunctionBuilder) Runtime(runtime string) *FunctionBuilder {
	b.runtime = runtime
	return b
}

// TimeoutMs overrides the trigger's default timeout.
func (b *FunctionBuilder) TimeoutMs(ms int) *FunctionBuilder {
	b.timeoutMs = &ms
	return b
}

// RunAs sets the authority ceiling for the function's fraiseql_query writes (#594).
func (b *FunctionBuilder) RunAs(runAs FunctionRunAs) *FunctionBuilder {
	b.runAs = &runAs
	return b
}

// When adds `when` predicates (#597), evaluated before the function fires.
func (b *FunctionBuilder) When(predicates ...FunctionPredicate) *FunctionBuilder {
	b.when = append(b.when, predicates...)
	return b
}

// ReRunnable opts out of durable dispatch for work that is safe to re-run.
func (b *FunctionBuilder) ReRunnable() *FunctionBuilder {
	b.reRunnable = true
	return b
}

// Retry sets a per-function retry policy for durable dispatch.
func (b *FunctionBuilder) Retry(cfg RetryConfig) *FunctionBuilder {
	b.retry = &cfg
	return b
}

// Register registers the function with the global schema registry.
// Returns an error if a function with the same name is already registered.
func (b *FunctionBuilder) Register() error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.functions[b.name]; exists {
		return fmt.Errorf("function %q is already registered; each name must be unique within a schema", b.name)
	}
	reg.functions[b.name] = FunctionDefinition{
		Name:       b.name,
		Trigger:    b.trigger,
		Runtime:    b.runtime,
		TimeoutMs:  b.timeoutMs,
		RunAs:      b.runAs,
		When:       b.when,
		ReRunnable: b.reRunnable,
		Retry:      b.retry,
	}
	return nil
}
