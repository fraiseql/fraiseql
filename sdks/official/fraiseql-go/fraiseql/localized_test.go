package fraiseql

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"
)

// `localized` authoring (#1527): a String field, input field or mutation argument stored
// as a locale map. The compiler reads `"localized": true`; these tests follow the
// declaration to the exported JSON, and refuse it on anything but a String, as the Python
// and TypeScript SDKs do.

func exportedSchema(t *testing.T) map[string]any {
	t.Helper()
	data, err := json.Marshal(GetSchema())
	if err != nil {
		t.Fatalf("marshal schema: %v", err)
	}
	var schema map[string]any
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("unmarshal schema: %v", err)
	}
	return schema
}

// entry returns the object named `name` in the section's array, and that object's array
// under `key`, indexed by name.
func entry(t *testing.T, schema map[string]any, section, name, key string) map[string]map[string]any {
	t.Helper()
	items, _ := schema[section].([]any)
	for _, item := range items {
		object := item.(map[string]any)
		if object["name"] != name {
			continue
		}
		byName := map[string]map[string]any{}
		children, _ := object[key].([]any)
		for _, child := range children {
			c := child.(map[string]any)
			byName[c["name"].(string)] = c
		}
		return byName
	}
	t.Fatalf("%s %q is absent from the exported schema", section, name)
	return nil
}

func TestALocalizedFieldIsExportedAsLocalized(t *testing.T) {
	Reset()
	defer Reset()

	if err := RegisterType("Product", []FieldInfo{
		{Name: "id", Type: "ID"},
		{Name: "name", Type: "String", Nullable: true, Localized: true},
		{Name: "sku", Type: "String"},
	}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}
	if err := RegisterInputType("CreateProductInput", []FieldInfo{
		{Name: "name", Type: "String", Localized: true},
	}, ""); err != nil {
		t.Fatalf("RegisterInputType: %v", err)
	}

	schema := exportedSchema(t)
	fields := entry(t, schema, "types", "Product", "fields")
	if fields["name"]["localized"] != true {
		t.Errorf("Product.name: %v", fields["name"])
	}
	if _, present := fields["sku"]["localized"]; present {
		t.Errorf("an unlocalized field carries no key: %v", fields["sku"])
	}
	inputs := entry(t, schema, "input_types", "CreateProductInput", "fields")
	if inputs["name"]["localized"] != true {
		t.Errorf("CreateProductInput.name: %v", inputs["name"])
	}
}

func TestALocalizedMutationArgumentIsExportedAsALocalizedString(t *testing.T) {
	Reset()
	defer Reset()

	if err := NewMutation("createProduct").
		ReturnType("Product").SqlSource("fn_create_product").Operation("insert").
		Arg("sku", "String", nil, false).
		LocalizedArg("name", true).
		Register(); err != nil {
		t.Fatalf("Register: %v", err)
	}

	args := entry(t, exportedSchema(t), "mutations", "createProduct", "arguments")
	want := map[string]any{"name": "name", "type": "String", "nullable": true, "localized": true}
	if !reflect.DeepEqual(args["name"], want) {
		t.Errorf("createProduct(name): %v, want %v", args["name"], want)
	}
	if _, present := args["sku"]["localized"]; present {
		t.Errorf("an unlocalized argument carries no key: %v", args["sku"])
	}
}

// Every registration path refuses `localized` on a type that is not a String, naming the
// field: a locale map holds strings.
func TestLocalizedIsRefusedOnANonString(t *testing.T) {
	notAString := []FieldInfo{{Name: "price", Type: "Float", Localized: true}}
	for site, register := range map[string]func() error{
		"RegisterType":      func() error { return RegisterType("Product", notAString, "") },
		"RegisterInputType": func() error { return RegisterInputType("ProductInput", notAString, "") },
		"RegisterErrorType": func() error { return RegisterErrorType("PriceError", notAString, "") },
	} {
		Reset()
		err := register()
		if err == nil || !strings.Contains(err.Error(), `"price"`) ||
			!strings.Contains(err.Error(), "only a String can be localized") {
			t.Errorf("%s: %v", site, err)
		}
	}
	Reset()
}

func TestTheStructTagDeclaresLocalized(t *testing.T) {
	info, err := parseFieldTag("name,type=String,localized=true", "Name", reflect.TypeOf(""))
	if err != nil {
		t.Fatalf("parseFieldTag: %v", err)
	}
	if !info.Localized {
		t.Errorf("localized=true was not read: %+v", info)
	}
}
