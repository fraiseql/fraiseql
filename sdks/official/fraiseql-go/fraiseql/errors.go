package fraiseql

import "time"

// FraiseQLError is the base error type for all SDK errors.
type FraiseQLError struct {
	Message string
	Cause   error
}

func (e *FraiseQLError) Error() string { return e.Message }

// Unwrap returns the underlying cause of the error, if any.
func (e *FraiseQLError) Unwrap() error { return e.Cause }

// GraphQLError wraps one or more errors from the GraphQL errors array.
type GraphQLError struct {
	FraiseQLError
	Errors []GraphQLErrorEntry
}

// NetworkError is a transport-level error.
type NetworkError struct{ FraiseQLError }

// TimeoutError is returned when the request exceeds the deadline.
type TimeoutError struct{ FraiseQLError }

// AuthenticationError is returned on 401/403 responses.
type AuthenticationError struct {
	FraiseQLError
	StatusCode int
}

// RateLimitError is returned on 429 responses.
type RateLimitError struct {
	FraiseQLError
	RetryAfter time.Duration
}
