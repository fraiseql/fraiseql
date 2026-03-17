package fraiseql

// GraphQLRequest is the JSON body sent to a GraphQL server.
type GraphQLRequest struct {
	Query     string         `json:"query"`
	Variables map[string]any `json:"variables,omitempty"`
}

// GraphQLResponse is the JSON body received from a GraphQL server.
type GraphQLResponse struct {
	Data   any                 `json:"data"`
	Errors []GraphQLErrorEntry `json:"errors"`
}

// GraphQLErrorEntry is one entry in the GraphQL errors array.
type GraphQLErrorEntry struct {
	Message    string         `json:"message"`
	Locations  []ErrorLocation `json:"locations,omitempty"`
	Path       []any          `json:"path,omitempty"`
	Extensions map[string]any `json:"extensions,omitempty"`
}

// ErrorLocation identifies a position in a GraphQL document.
type ErrorLocation struct {
	Line   int `json:"line"`
	Column int `json:"column"`
}
