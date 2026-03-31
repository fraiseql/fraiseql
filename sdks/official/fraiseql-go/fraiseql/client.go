package fraiseql

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// ClientConfig holds configuration for a FraiseQL client.
type ClientConfig struct {
	// URL is the FraiseQL server endpoint (e.g., "http://localhost:8000/graphql").
	URL string
	// Authorization is a static "Bearer <token>" value sent with every request.
	Authorization string
	// TokenFn provides a dynamic token per request (overrides Authorization if set).
	TokenFn func() string
	// Timeout is the per-request timeout. Default: 30s.
	Timeout time.Duration
	// Retry configures automatic retries. Default: MaxAttempts=1 (no retry).
	Retry *ClientRetryConfig
	// HTTPClient is an injectable HTTP client for testing. Default: http.DefaultClient.
	HTTPClient *http.Client
}

// Client is a FraiseQL HTTP client.
type Client struct {
	config ClientConfig
	http   *http.Client
}

// NewClient creates a new FraiseQL client with the given configuration.
func NewClient(config ClientConfig) *Client {
	c := &Client{config: config}
	if config.HTTPClient != nil {
		c.http = config.HTTPClient
	} else {
		c.http = &http.Client{
			Timeout: 30 * time.Second,
		}
	}
	if config.Timeout > 0 {
		c.http.Timeout = config.Timeout
	}
	return c
}

// ExecuteInput holds the inputs for an Execute call.
type ExecuteInput struct {
	Query         string
	Variables     map[string]any
	OperationName string
}

// Query executes a GraphQL query and unmarshals the data section into v.
func (c *Client) Query(ctx context.Context, query string, variables map[string]any, v any) error {
	return c.execute(ctx, ExecuteInput{Query: query, Variables: variables}, v)
}

// Mutate executes a GraphQL mutation and unmarshals the data section into v.
func (c *Client) Mutate(ctx context.Context, mutation string, variables map[string]any, v any) error {
	return c.execute(ctx, ExecuteInput{Query: mutation, Variables: variables}, v)
}

// Execute executes a GraphQL operation with full control over the request inputs.
func (c *Client) Execute(ctx context.Context, input ExecuteInput, v any) error {
	return c.execute(ctx, input, v)
}

func (c *Client) execute(ctx context.Context, input ExecuteInput, v any) error {
	attempts := 1
	retry := c.config.Retry
	if retry != nil && retry.MaxAttempts > 1 {
		attempts = retry.MaxAttempts
	}

	var lastErr error
	for i := 0; i < attempts; i++ {
		if i > 0 && retry != nil {
			if !retry.shouldRetry(lastErr) {
				return lastErr
			}
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-time.After(retry.delayFor(i - 1)):
			}
		}

		lastErr = c.doRequest(ctx, input, v)
		if lastErr == nil {
			return nil
		}
	}
	return lastErr
}

func (c *Client) doRequest(ctx context.Context, input ExecuteInput, v any) error {
	body, err := json.Marshal(GraphQLRequest{Query: input.Query, Variables: input.Variables, OperationName: input.OperationName})
	if err != nil {
		return &NetworkError{FraiseQLError: FraiseQLError{Message: fmt.Sprintf("marshal request: %v", err)}}
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.config.URL, bytes.NewReader(body))
	if err != nil {
		return &NetworkError{FraiseQLError: FraiseQLError{Message: fmt.Sprintf("create request: %v", err)}}
	}
	req.Header.Set("Content-Type", "application/json")

	token := c.config.Authorization
	if c.config.TokenFn != nil {
		token = c.config.TokenFn()
	}
	if token != "" {
		req.Header.Set("Authorization", token)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		if strings.Contains(err.Error(), "deadline exceeded") || strings.Contains(err.Error(), "timeout") {
			return &TimeoutError{FraiseQLError: FraiseQLError{Message: err.Error(), Cause: err}}
		}
		return &NetworkError{FraiseQLError: FraiseQLError{Message: err.Error(), Cause: err}}
	}
	defer resp.Body.Close() //nolint:errcheck

	switch resp.StatusCode {
	case http.StatusUnauthorized, http.StatusForbidden:
		return &AuthenticationError{
			FraiseQLError: FraiseQLError{Message: fmt.Sprintf("authentication failed (HTTP %d)", resp.StatusCode)},
			StatusCode:    resp.StatusCode,
		}
	case http.StatusTooManyRequests:
		retryAfter, _ := strconv.Atoi(resp.Header.Get("Retry-After"))
		return &RateLimitError{
			FraiseQLError: FraiseQLError{Message: "rate limit exceeded"},
			RetryAfter:    time.Duration(retryAfter) * time.Second,
		}
	}

	rawBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return &NetworkError{FraiseQLError: FraiseQLError{Message: fmt.Sprintf("read response: %v", err)}}
	}

	var gqlResp GraphQLResponse
	if err := json.Unmarshal(rawBody, &gqlResp); err != nil {
		return &NetworkError{FraiseQLError: FraiseQLError{Message: fmt.Sprintf("unmarshal response: %v", err)}}
	}

	// null errors = success (cross-SDK invariant)
	if len(gqlResp.Errors) > 0 {
		return &GraphQLError{
			FraiseQLError: FraiseQLError{Message: gqlResp.Errors[0].Message},
			Errors:        gqlResp.Errors,
		}
	}

	if v == nil || gqlResp.Data == nil {
		return nil
	}
	// Re-marshal the data portion into v.
	dataBytes, err := json.Marshal(gqlResp.Data)
	if err != nil {
		return &NetworkError{FraiseQLError: FraiseQLError{Message: fmt.Sprintf("marshal data: %v", err)}}
	}
	return json.Unmarshal(dataBytes, v)
}
