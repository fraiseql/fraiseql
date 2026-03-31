package fraiseql_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// writeJSON is a test helper that writes a JSON-encoded value as the response body.
func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func TestQuerySuccess(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, map[string]any{
			"data": map[string]any{"name": "Alice"},
		})
	}))
	defer srv.Close()

	client := fraiseql.NewClient(fraiseql.ClientConfig{URL: srv.URL})

	var result struct {
		Name string `json:"name"`
	}
	err := client.Query(context.Background(), `{ user { name } }`, nil, &result)
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if result.Name != "Alice" {
		t.Errorf("expected Name=Alice, got %q", result.Name)
	}
}

func TestQueryNullErrorsIsSuccess(t *testing.T) {
	// Servers may return {"data": {...}, "errors": null}; this must NOT be an error.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		// Write raw JSON with explicit null errors field.
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"data":{"id":1},"errors":null}`))
	}))
	defer srv.Close()

	client := fraiseql.NewClient(fraiseql.ClientConfig{URL: srv.URL})

	var result struct {
		ID int `json:"id"`
	}
	err := client.Query(context.Background(), `{ item { id } }`, nil, &result)
	if err != nil {
		t.Fatalf("expected no error for null errors field, got %v", err)
	}
	if result.ID != 1 {
		t.Errorf("expected ID=1, got %d", result.ID)
	}
}

func TestQueryGraphQLError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, map[string]any{
			"data": nil,
			"errors": []map[string]any{
				{"message": "field not found"},
			},
		})
	}))
	defer srv.Close()

	client := fraiseql.NewClient(fraiseql.ClientConfig{URL: srv.URL})

	err := client.Query(context.Background(), `{ missing }`, nil, nil)
	if err == nil {
		t.Fatal("expected error, got nil")
	}

	var gqlErr *fraiseql.GraphQLError
	if !errors.As(err, &gqlErr) {
		t.Fatalf("expected *GraphQLError, got %T: %v", err, err)
	}
	if len(gqlErr.Errors) == 0 {
		t.Fatal("expected at least one error entry")
	}
	if gqlErr.Errors[0].Message != "field not found" {
		t.Errorf("unexpected error message: %q", gqlErr.Errors[0].Message)
	}
}

func TestQueryAuth401(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer srv.Close()

	client := fraiseql.NewClient(fraiseql.ClientConfig{URL: srv.URL})

	err := client.Query(context.Background(), `{ me { id } }`, nil, nil)
	if err == nil {
		t.Fatal("expected error, got nil")
	}

	var authErr *fraiseql.AuthenticationError
	if !errors.As(err, &authErr) {
		t.Fatalf("expected *AuthenticationError, got %T: %v", err, err)
	}
	if authErr.StatusCode != http.StatusUnauthorized {
		t.Errorf("expected StatusCode=401, got %d", authErr.StatusCode)
	}
}

func TestQueryRetry(t *testing.T) {
	var attempts atomic.Int32

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		n := attempts.Add(1)
		if n < 3 {
			// Force a connection close to trigger a NetworkError on the client.
			hj, ok := w.(http.Hijacker)
			if !ok {
				// Fallback: return empty body which causes unmarshal error (not a NetworkError).
				// We still count the attempt.
				w.WriteHeader(http.StatusOK)
				return
			}
			conn, _, _ := hj.Hijack()
			_ = conn.Close()
			return
		}
		writeJSON(w, map[string]any{"data": map[string]any{"ok": true}})
	}))
	defer srv.Close()

	retryCfg := fraiseql.ClientRetryConfig{
		MaxAttempts: 3,
		BaseDelay:   0, // no delay in tests
		MaxDelay:    0,
		Jitter:      false,
		RetryOn: []func(error) bool{
			func(err error) bool {
				var netErr *fraiseql.NetworkError
				return errors.As(err, &netErr)
			},
		},
	}

	client := fraiseql.NewClient(fraiseql.ClientConfig{
		URL:   srv.URL,
		Retry: &retryCfg,
	})

	var result struct {
		OK bool `json:"ok"`
	}
	err := client.Query(context.Background(), `{ status { ok } }`, nil, &result)
	if err != nil {
		t.Fatalf("expected success after retries, got %v", err)
	}
	if n := attempts.Load(); n != 3 {
		t.Errorf("expected 3 attempts, got %d", n)
	}
}
