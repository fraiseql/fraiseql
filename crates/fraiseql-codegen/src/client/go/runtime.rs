//! The `client.go` runtime template.
//!
//! Identical for every generated client and dependency-free: the transport is
//! `net/http` from the standard library. The generator prepends the schema-hash
//! header and the `package` clause; this constant is the body.

/// Contents of the generated `client.go` (without the header or `package` line).
pub(super) const CLIENT_GO: &str = r#"import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

// ResponseError is one entry of a GraphQL response's `errors` array.
type ResponseError struct {
	Message    string         `json:"message"`
	Path       []any          `json:"path,omitempty"`
	Locations  []any          `json:"locations,omitempty"`
	Extensions map[string]any `json:"extensions,omitempty"`
}

// Error is returned when a request fails at the HTTP or GraphQL-errors layer.
type Error struct {
	Message string
	Errors  []ResponseError
}

func (e *Error) Error() string { return e.Message }

// Client executes GraphQL documents against a FraiseQL endpoint. The generated
// operation functions wrap Request and unwrap their single root field.
//
// Construct one with NewClient; the zero value has no endpoint.
type Client struct {
	// Endpoint is the GraphQL URL, e.g. "https://api.example.com/graphql".
	Endpoint string
	// HTTPClient performs the request. NewClient supplies one with a 30s timeout;
	// a nil value falls back to http.DefaultClient.
	HTTPClient *http.Client
	// Headers, when set, is called once per request for extra headers such as an
	// Authorization token.
	Headers func() map[string]string
}

// NewClient returns a Client for endpoint with a default 30s-timeout transport.
func NewClient(endpoint string) *Client {
	return &Client{
		Endpoint:   endpoint,
		HTTPClient: &http.Client{Timeout: 30 * time.Second},
	}
}

type graphqlResponse struct {
	Data   json.RawMessage `json:"data"`
	Errors []ResponseError `json:"errors"`
}

// Request executes document with variables and unmarshals the response's `data`
// payload into out.
//
// It returns an *Error when the response carries GraphQL errors, a non-2xx HTTP
// status, or no `data`; any other failure is wrapped with %w.
func (c *Client) Request(document string, variables map[string]any, out any) error {
	if variables == nil {
		variables = map[string]any{}
	}
	body, err := json.Marshal(map[string]any{"query": document, "variables": variables})
	if err != nil {
		return fmt.Errorf("fraiseql: encoding request: %w", err)
	}

	req, err := http.NewRequest(http.MethodPost, c.Endpoint, bytes.NewReader(body))
	if err != nil {
		return fmt.Errorf("fraiseql: building request: %w", err)
	}
	req.Header.Set("content-type", "application/json")
	req.Header.Set("accept", "application/json")
	if c.Headers != nil {
		for key, value := range c.Headers() {
			req.Header.Set(key, value)
		}
	}

	httpClient := c.HTTPClient
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	resp, err := httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("fraiseql: request failed: %w", err)
	}
	defer func() { _ = resp.Body.Close() }()

	payload, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("fraiseql: reading response: %w", err)
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return &Error{Message: fmt.Sprintf(
			"GraphQL request failed with HTTP %d %s",
			resp.StatusCode,
			http.StatusText(resp.StatusCode),
		)}
	}

	var decoded graphqlResponse
	if err := json.Unmarshal(payload, &decoded); err != nil {
		return fmt.Errorf("fraiseql: decoding response: %w", err)
	}
	if len(decoded.Errors) > 0 {
		return &Error{Message: decoded.Errors[0].Message, Errors: decoded.Errors}
	}
	if len(decoded.Data) == 0 || string(decoded.Data) == "null" {
		return &Error{Message: "GraphQL response contained no data"}
	}
	if err := json.Unmarshal(decoded.Data, out); err != nil {
		return fmt.Errorf("fraiseql: decoding data: %w", err)
	}
	return nil
}
"#;
