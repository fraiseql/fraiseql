package fraiseql

import "fmt"

// ScalarValidationError is returned when custom scalar validation fails.
//
// It provides context about which scalar failed, in what context, and why.
type ScalarValidationError struct {
	ScalarName string
	Context    string
	Message    string
	Underlying error
}

// Error implements the error interface.
func (e *ScalarValidationError) Error() string {
	return fmt.Sprintf(
		"Scalar \"%s\" validation failed in %s: %s",
		e.ScalarName,
		e.Context,
		e.Message,
	)
}

// Unwrap returns the underlying error for error chain inspection.
func (e *ScalarValidationError) Unwrap() error {
	return e.Underlying
}

// NewScalarValidationError creates a new ScalarValidationError.
func NewScalarValidationError(scalarName, context, message string) *ScalarValidationError {
	return &ScalarValidationError{
		ScalarName: scalarName,
		Context:    context,
		Message:    message,
	}
}

// NewScalarValidationErrorWithCause creates a new ScalarValidationError with an underlying cause.
func NewScalarValidationErrorWithCause(scalarName, context, message string, cause error) *ScalarValidationError {
	return &ScalarValidationError{
		ScalarName: scalarName,
		Context:    context,
		Message:    message,
		Underlying: cause,
	}
}
