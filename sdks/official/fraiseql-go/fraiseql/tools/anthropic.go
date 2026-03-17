// Package tools provides AI framework tool format conversions for FraiseQL queries.
package tools

import (
	"context"
	"encoding/json"
)

// ToolOptions describes a FraiseQL query as an AI tool.
type ToolOptions struct {
	// Name is the tool name as shown to the AI model.
	Name string
	// Description explains what the tool does.
	Description string
	// Query is the GraphQL query string.
	Query string
	// InputSchema is a JSON Schema object describing the query variables.
	InputSchema map[string]any
	// Execute executes the tool with the given arguments and returns the result.
	Execute func(ctx context.Context, input map[string]any) (any, error)
}

// AnthropicTool is the Anthropic API tool definition format.
type AnthropicTool struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	InputSchema json.RawMessage `json:"input_schema"`
}

// ToAnthropicTool creates an Anthropic-compatible tool definition from a ToolOptions.
func ToAnthropicTool(opts ToolOptions) AnthropicTool {
	schema, _ := json.Marshal(opts.InputSchema) //nolint:errcheck
	return AnthropicTool{
		Name:        opts.Name,
		Description: opts.Description,
		InputSchema: schema,
	}
}
