package fraiseql_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// A Decimal travels as the string of its exact digits. The server takes a Decimal
// argument as a string such as "1.50", or an exact integer, and refuses a number with a
// fraction. fraiseql.Decimal is a string type, so it is sent and read as one.

// decimalEcho answers with the variables it received and records the raw request body.
func decimalEcho(t *testing.T, sent *[]byte) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Fatalf("read body: %v", err)
		}
		*sent = body
		var req struct {
			Variables json.RawMessage `json:"variables"`
		}
		if err := json.Unmarshal(body, &req); err != nil {
			t.Fatalf("decode body: %v", err)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"data":{"echo":` + string(req.Variables) + `}}`))
	}))
}

func TestDecimalVariableIsSentAndReadAsItsExactDigits(t *testing.T) {
	for _, digits := range []string{
		"1.50",
		"100.00",
		"12345678901234567890.12",
		"123456789012345678901234567890",
		"-0.00",
	} {
		t.Run(digits, func(t *testing.T) {
			var sent []byte
			srv := decimalEcho(t, &sent)
			defer srv.Close()
			client := fraiseql.NewClient(fraiseql.ClientConfig{URL: srv.URL})

			var result struct {
				Echo struct {
					A fraiseql.Decimal `json:"a"`
				} `json:"echo"`
			}
			vars := map[string]any{"a": fraiseql.Decimal(digits)}
			if err := client.Query(context.Background(), `query($a: Decimal) { echo(a: $a) }`, vars, &result); err != nil {
				t.Fatalf("query: %v", err)
			}

			var wire struct {
				Variables map[string]json.RawMessage `json:"variables"`
			}
			if err := json.Unmarshal(sent, &wire); err != nil {
				t.Fatalf("decode sent: %v", err)
			}
			if got, want := string(wire.Variables["a"]), `"`+digits+`"`; got != want {
				t.Errorf("sent %s, want the JSON string %s", got, want)
			}
			if string(result.Echo.A) != digits {
				t.Errorf("read %q, want %q", result.Echo.A, digits)
			}
		})
	}
}

func TestDecimalResponseIntoAPlainStringKeepsItsDigits(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"data":{"amount":"12345678901234567890.12"}}`))
	}))
	defer srv.Close()
	client := fraiseql.NewClient(fraiseql.ClientConfig{URL: srv.URL})

	var result struct {
		Amount string `json:"amount"`
	}
	if err := client.Query(context.Background(), `{ amount }`, nil, &result); err != nil {
		t.Fatalf("query: %v", err)
	}
	if result.Amount != "12345678901234567890.12" {
		t.Errorf("read %q", result.Amount)
	}
}
