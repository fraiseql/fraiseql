package fraiseql

import (
	"encoding/json"
	"strings"
	"testing"
)

type orderStatus string

const (
	statusPending   orderStatus = "pending"
	statusInTransit orderStatus = "transit"
)

type priority int

const (
	priorityLow  priority = 1
	priorityHigh priority = 3
)

// #1611: a member is stored as the value its constant has, a string or an integer, and as
// its name when it is given none or its value is its name; any other kind is refused,
// naming the member.
func TestEnumMembersAreStoredAsTheirConstantsValues(t *testing.T) {
	Reset()

	if err := Enum("OrderStatus",
		EnumMember{Name: "PENDING", Value: statusPending},
		EnumMember{Name: "IN_TRANSIT", Value: statusInTransit},
		EnumMember{Name: "SHIPPED"},
		EnumMember{Name: "CANCELLED", Value: "CANCELLED"},
	); err != nil {
		t.Fatal(err)
	}
	if err := Enum("Priority",
		EnumMember{Name: "LOW", Value: priorityLow},
		EnumMember{Name: "HIGH", Value: priorityHigh},
	); err != nil {
		t.Fatal(err)
	}

	got, err := json.Marshal(GetSchema().Enums)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		`{"name":"OrderStatus","values":[{"name":"PENDING","value":"pending"},{"name":"IN_TRANSIT","value":"transit"},{"name":"SHIPPED"},{"name":"CANCELLED"}]}`,
		`{"name":"Priority","values":[{"name":"LOW","value":1},{"name":"HIGH","value":3}]}`,
	} {
		if !strings.Contains(string(got), want) {
			t.Errorf("exported enums %s\nwant %s", got, want)
		}
	}
}

func TestAnEnumMemberValueOfAnotherKindIsRefused(t *testing.T) {
	Reset()

	for _, value := range []any{0.5, true, []string{"a"}} {
		err := Enum("Ratio", EnumMember{Name: "HALF", Value: value})
		if err == nil || !strings.Contains(err.Error(), "Ratio.HALF") {
			t.Errorf("value %#v: want a refusal naming Ratio.HALF, got %v", value, err)
		}
	}
}
