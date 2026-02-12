package fraiseql

import "fmt"

// ValidateCustomScalar validates a value with a custom scalar in a given context.
//
// The context parameter must be one of: "serialize", "parseValue", or "parseLiteral".
//
// Example:
//
//	result, err := ValidateCustomScalar(&Email{}, "test@example.com", "parseValue")
//	if err != nil {
//	    // Handle validation error
//	}
//
// Returns a ScalarValidationError if validation fails.
func ValidateCustomScalar(scalar CustomScalar, value interface{}, context string) (interface{}, error) {
	if scalar == nil {
		return nil, NewScalarValidationError("unknown", context, "scalar is nil")
	}

	scalarName := scalar.Name()

	switch context {
	case "serialize":
		result, err := scalar.Serialize(value)
		if err != nil {
			return nil, NewScalarValidationErrorWithCause(scalarName, context, err.Error(), err)
		}
		return result, nil

	case "parseValue":
		result, err := scalar.ParseValue(value)
		if err != nil {
			return nil, NewScalarValidationErrorWithCause(scalarName, context, err.Error(), err)
		}
		return result, nil

	case "parseLiteral":
		result, err := scalar.ParseLiteral(value)
		if err != nil {
			return nil, NewScalarValidationErrorWithCause(scalarName, context, err.Error(), err)
		}
		return result, nil

	default:
		return nil, fmt.Errorf("unknown validation context: %s", context)
	}
}

// ValidateCustomScalarWithDefault validates a value with a custom scalar,
// defaulting the context to "parseValue".
//
// This is a convenience method that calls ValidateCustomScalar with "parseValue" context.
func ValidateCustomScalarWithDefault(scalar CustomScalar, value interface{}) (interface{}, error) {
	return ValidateCustomScalar(scalar, value, "parseValue")
}
