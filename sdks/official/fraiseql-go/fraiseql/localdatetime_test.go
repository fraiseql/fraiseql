package fraiseql

import (
	"reflect"
	"testing"
	"time"
)

// A time.Time is an instant, and Go has no zone-less date-time of its own, so it stays
// `DateTime`; a date and time with no offset (a PostgreSQL `timestamp`) is declared with
// the SDK's LocalDateTime type, which a field infers as the engine scalar of its name.
func TestLocalDateTimeIsTheMarkerAndTimeStaysDateTime(t *testing.T) {
	tests := []struct {
		goType reflect.Type
		want   string
		null   bool
	}{
		{reflect.TypeOf(LocalDateTime("")), "LocalDateTime", false},
		{reflect.TypeOf((*LocalDateTime)(nil)), "LocalDateTime", true},
		{reflect.TypeOf([]LocalDateTime{}), "[LocalDateTime!]", false},
		{reflect.TypeOf(time.Time{}), "DateTime", false},
	}
	for _, tt := range tests {
		t.Run(tt.goType.String(), func(t *testing.T) {
			got, null, err := goToGraphQLType(tt.goType)
			if err != nil {
				t.Fatalf("goToGraphQLType(%v): %v", tt.goType, err)
			}
			if got != tt.want || null != tt.null {
				t.Errorf("goToGraphQLType(%v) = (%q, %v), want (%q, %v)", tt.goType, got, null, tt.want, tt.null)
			}
		})
	}
}

func TestLocalDateTimeIsAKnownScalarName(t *testing.T) {
	if !IsScalarType("LocalDateTime") {
		t.Error(`IsScalarType("LocalDateTime") = false`)
	}
}
