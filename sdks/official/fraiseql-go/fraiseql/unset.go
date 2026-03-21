package fraiseql

// Field represents a three-state field value for update mutations:
// Unset (not provided), Null (explicitly null), or Value (has a value).
// This mirrors the UNSET sentinel pattern used in the Python SDK.
type Field[T any] struct {
	value  T
	isSet  bool
	isNull bool
}

// Unset returns a Field in the "not provided" state.
// When serialized, unset fields are omitted entirely from the request.
func Unset[T any]() Field[T] { return Field[T]{} }

// Null returns a Field explicitly set to null.
// When serialized, null fields are included with a null value.
func Null[T any]() Field[T] { return Field[T]{isSet: true, isNull: true} }

// Value returns a Field with the given value.
func Value[T any](v T) Field[T] { return Field[T]{value: v, isSet: true} }

// IsUnset returns true if this field was not provided.
func (f Field[T]) IsUnset() bool { return !f.isSet }

// IsNull returns true if this field was explicitly set to null.
func (f Field[T]) IsNull() bool { return f.isSet && f.isNull }

// IsValue returns true if this field has a concrete value (not unset, not null).
func (f Field[T]) IsValue() bool { return f.isSet && !f.isNull }

// Get returns the value. Panics if the field is unset or null.
func (f Field[T]) Get() T {
	if !f.isSet || f.isNull {
		panic("fraiseql: cannot Get value from unset or null Field")
	}
	return f.value
}

// GetOr returns the value, or the provided default if the field is unset or null.
func (f Field[T]) GetOr(defaultVal T) T {
	if f.isSet && !f.isNull {
		return f.value
	}
	return defaultVal
}
