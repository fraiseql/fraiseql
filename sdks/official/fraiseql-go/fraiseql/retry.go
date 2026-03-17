package fraiseql

import (
	"errors"
	"math"
	"math/rand"
	"time"
)

// RetryConfig configures automatic request retries.
type RetryConfig struct {
	// MaxAttempts is the total number of attempts (default: 1, no retry).
	MaxAttempts int
	// BaseDelay is the initial delay before the first retry (default: 1s).
	BaseDelay time.Duration
	// MaxDelay is the upper bound on exponential back-off delay (default: 30s).
	MaxDelay time.Duration
	// Jitter adds up to 10% random jitter to each delay when true (default: true).
	Jitter bool
	// RetryOn is a list of predicates that decide whether an error is retryable.
	// Default: retries on NetworkError and TimeoutError.
	RetryOn []func(error) bool
}

// DefaultRetryConfig returns a RetryConfig with no retries (safe default).
func DefaultRetryConfig() RetryConfig {
	return RetryConfig{
		MaxAttempts: 1,
		BaseDelay:   time.Second,
		MaxDelay:    30 * time.Second,
		Jitter:      true,
		RetryOn: []func(error) bool{
			func(err error) bool {
				var netErr *NetworkError
				return errors.As(err, &netErr)
			},
			func(err error) bool {
				var toErr *TimeoutError
				return errors.As(err, &toErr)
			},
		},
	}
}

func (r RetryConfig) shouldRetry(err error) bool {
	for _, pred := range r.RetryOn {
		if pred(err) {
			return true
		}
	}
	return false
}

func (r RetryConfig) delayFor(attempt int) time.Duration {
	delay := time.Duration(float64(r.BaseDelay) * math.Pow(2, float64(attempt)))
	if delay > r.MaxDelay {
		delay = r.MaxDelay
	}
	if r.Jitter {
		// Add up to 10% jitter (non-cryptographic, intentional)
		//nolint:gosec
		jitter := time.Duration(rand.Float64() * float64(delay) * 0.1)
		delay += jitter
	}
	return delay
}
