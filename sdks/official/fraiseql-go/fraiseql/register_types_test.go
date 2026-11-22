package fraiseql

import (
	"reflect"
	"slices"
	"strings"
	"testing"
)

// RegisterTypes (#2275): a type's fields are exported in the struct's declaration order,
// the same on every run, and a type it cannot register is an error, not a skip.

type orderedInvoice struct {
	ID        string   `fraiseql:"id"`
	Number    string   `fraiseql:"number"`
	Customer  string   `fraiseql:"customer"`
	Total     float64  `fraiseql:"total"`
	Paid      bool     `fraiseql:"paid"`
	Notes     *string  `fraiseql:"notes"`
	Lines     []string `fraiseql:"lines"`
	Reference string   `fraiseql:"reference"`
}

func registeredFieldNames(t *testing.T, typeName string) []string {
	t.Helper()
	for _, def := range GetSchema().Types {
		if def.Name == typeName {
			names := make([]string, 0, len(def.Fields))
			for _, f := range def.Fields {
				names = append(names, f.Name)
			}
			return names
		}
	}
	t.Fatalf("type %q is not registered", typeName)
	return nil
}

func TestRegisterTypesKeepsDeclarationOrder(t *testing.T) {
	want := []string{"id", "number", "customer", "total", "paid", "notes", "lines", "reference"}
	// Go randomizes map iteration: under a map, 50 runs of 8 fields all in one order is
	// a chance of (1/8!)^50.
	for run := range 50 {
		Reset()
		if err := RegisterTypes(orderedInvoice{}); err != nil {
			t.Fatalf("RegisterTypes: %v", err)
		}
		if got := registeredFieldNames(t, "orderedInvoice"); !slices.Equal(got, want) {
			t.Fatalf("run %d: fields %v, want %v", run, got, want)
		}
	}
	Reset()
}

func TestExtractFieldsKeepsDeclarationOrder(t *testing.T) {
	want := []string{"id", "number", "customer", "total", "paid", "notes", "lines", "reference"}
	for run := range 50 {
		fields, err := ExtractFields(reflect.TypeOf(orderedInvoice{}))
		if err != nil {
			t.Fatalf("ExtractFields: %v", err)
		}
		got := make([]string, 0, len(fields))
		for _, f := range fields {
			got = append(got, f.Name)
		}
		if !slices.Equal(got, want) {
			t.Fatalf("run %d: fields %v, want %v", run, got, want)
		}
	}
}

type registeredTwice struct {
	ID string `fraiseql:"id"`
}

func TestRegisterTypesReturnsADuplicateTypeError(t *testing.T) {
	Reset()
	defer Reset()
	err := RegisterTypes(registeredTwice{}, registeredTwice{})
	if err == nil || !strings.Contains(err.Error(), "already registered") ||
		!strings.Contains(err.Error(), "registeredTwice") {
		t.Errorf("RegisterTypes(twice) error = %v, want the type's \"already registered\"", err)
	}
}

type localizedCount struct {
	Count int `fraiseql:"count,localized=true"`
}

func TestRegisterTypesReturnsAFieldDeclarationError(t *testing.T) {
	Reset()
	defer Reset()
	err := RegisterTypes(localizedCount{})
	if err == nil || !strings.Contains(err.Error(), "only a String can be localized") {
		t.Errorf("RegisterTypes(localized Int) error = %v, want the localized refusal", err)
	}
	if slices.ContainsFunc(GetSchema().Types, func(d TypeDefinition) bool { return d.Name == "localizedCount" }) {
		t.Error("a refused type was registered")
	}
}

type twoEmails struct {
	Email   string `fraiseql:"email"`
	Contact string `fraiseql:"email"`
}

// Two struct fields publishing one name had collapsed into one, silently, before compile
// could see the duplicate.
func TestExtractFieldsRefusesADuplicatePublishedName(t *testing.T) {
	_, err := ExtractFields(reflect.TypeOf(twoEmails{}))
	if err == nil || !strings.Contains(err.Error(), "twoEmails.Email") ||
		!strings.Contains(err.Error(), "twoEmails.Contact") ||
		!strings.Contains(err.Error(), `"email"`) {
		t.Errorf("ExtractFields(two fields named email) error = %v, want one naming both", err)
	}
}
