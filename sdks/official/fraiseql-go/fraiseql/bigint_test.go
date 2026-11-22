package fraiseql

import (
	"reflect"
	"strings"
	"testing"
	"time"
)

// A 64-bit integer published as `Int` is refused at read time once its value passes 32
// bits, so each Go integer type maps to the narrowest engine scalar that holds every value
// it can take: `BigInt` (a signed 64-bit integer) for int64, uint32 and time.Duration (its
// nanoseconds), `Int` for the narrower ones and for `int`.
func TestIntegerTypesMapToTheScalarThatHoldsTheirRange(t *testing.T) {
	tests := []struct {
		goType reflect.Type
		want   string
		null   bool
	}{
		{reflect.TypeOf(int(0)), "Int", false},
		{reflect.TypeOf(int8(0)), "Int", false},
		{reflect.TypeOf(int16(0)), "Int", false},
		{reflect.TypeOf(int32(0)), "Int", false},
		{reflect.TypeOf(uint8(0)), "Int", false},
		{reflect.TypeOf(uint16(0)), "Int", false},
		{reflect.TypeOf(int64(0)), "BigInt", false},
		{reflect.TypeOf(uint32(0)), "BigInt", false},
		{reflect.TypeOf(time.Duration(0)), "BigInt", false},
		{reflect.TypeOf((*int64)(nil)), "BigInt", true},
		{reflect.TypeOf([]int64{}), "[BigInt!]", false},
		{reflect.TypeOf((*[]*int64)(nil)), "[BigInt]", true},
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

// An unsigned 64-bit integer holds values no signed 64-bit integer does, so no integer
// scalar can carry it: it is refused at authoring, naming the type and the scalar that can.
func TestUnsignedSixtyFourBitIntegersAreRefusedPointingToDecimal(t *testing.T) {
	for _, goType := range []reflect.Type{
		reflect.TypeOf(uint(0)),
		reflect.TypeOf(uint64(0)),
		reflect.TypeOf((*uint64)(nil)),
		reflect.TypeOf([]uint{}),
	} {
		t.Run(goType.String(), func(t *testing.T) {
			got, _, err := goToGraphQLType(goType)
			if err == nil {
				t.Fatalf("goToGraphQLType(%v) = %q, want an error", goType, got)
			}
			for _, part := range []string{strings.TrimLeft(goType.String(), "*[]"), "Decimal"} {
				if !strings.Contains(err.Error(), part) {
					t.Errorf("error %q does not name %q", err, part)
				}
			}
		})
	}
}

// The struct path reaches the same mapping: a field inferred from its Go type, and one
// whose tag names it without a type.
func TestStructFieldsInferBigInt(t *testing.T) {
	type ledgerEntry struct {
		Amount  int64
		Balance *int64 `fraiseql:"balance"`
	}
	fields, err := extractByName(reflect.TypeOf(ledgerEntry{}))
	if err != nil {
		t.Fatalf("ExtractFields: %v", err)
	}
	if got := fields["Amount"]; got.Type != "BigInt" || got.Nullable {
		t.Errorf("Amount = %+v, want a non-null BigInt", got)
	}
	if got := fields["balance"]; got.Type != "BigInt" || !got.Nullable {
		t.Errorf("balance = %+v, want a nullable BigInt", got)
	}

	type unsignedTotal struct {
		Total uint64
	}
	if _, err := ExtractFields(reflect.TypeOf(unsignedTotal{})); err == nil ||
		!strings.Contains(err.Error(), "Decimal") {
		t.Errorf("ExtractFields(uint64 field) error = %v, want one pointing to Decimal", err)
	}

	// The error's advice works: the tag it names declares the field.
	type declaredTotal struct {
		Total uint64 `fraiseql:"total,type=Decimal"`
	}
	fields, err = extractByName(reflect.TypeOf(declaredTotal{}))
	if err != nil || fields["total"].Type != "Decimal" {
		t.Errorf("ExtractFields(uint64 tagged Decimal) = %+v, %v, want a Decimal field", fields, err)
	}
}
