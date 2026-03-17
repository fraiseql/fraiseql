package tools

import "encoding/json"

// OpenAIFunction is the function definition inside an OpenAI tool.
type OpenAIFunction struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Parameters  json.RawMessage `json:"parameters"`
}

// OpenAITool is the OpenAI API tool definition format.
type OpenAITool struct {
	Type     string         `json:"type"`
	Function OpenAIFunction `json:"function"`
}

// ToOpenAITool creates an OpenAI-compatible tool definition from a ToolOptions.
func ToOpenAITool(opts ToolOptions) OpenAITool {
	params, _ := json.Marshal(opts.InputSchema) //nolint:errcheck
	return OpenAITool{
		Type: "function",
		Function: OpenAIFunction{
			Name:        opts.Name,
			Description: opts.Description,
			Parameters:  params,
		},
	}
}
