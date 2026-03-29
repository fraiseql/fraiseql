package fraiseql

import (
	"fmt"
	"strings"
)

// generateCrudOperations generates standard CRUD queries and mutations for a type.
// The crud parameter is either bool (true = all operations) or []string of specific
// operations to generate: "read", "create", "update", "delete".
//
// Generated operations follow FraiseQL conventions:
//   - Read:   query <snake> (get by PK) + query <snake>s (list with auto_params)
//   - Create: mutation create_<snake> with all fields as arguments
//   - Update: mutation update_<snake> with PK required, other fields nullable
//   - Delete: mutation delete_<snake> with PK only
func generateCrudOperations(typeName string, fields []FieldInfo, crud interface{}, sqlSource string) error {
	ops := parseCrudOps(crud)
	if len(ops) == 0 {
		return nil
	}

	snake := pascalToSnake(typeName)
	view := sqlSource
	if view == "" {
		view = "v_" + snake
	}

	var pkField FieldInfo
	if len(fields) > 0 {
		pkField = fields[0]
	} else {
		return fmt.Errorf("type %q has no fields; cannot generate CRUD operations", typeName)
	}

	if ops["read"] {
		if err := generateReadOps(typeName, snake, view, pkField); err != nil {
			return err
		}
	}
	if ops["create"] {
		if err := generateCreateOp(typeName, snake, fields); err != nil {
			return err
		}
	}
	if ops["update"] {
		if err := generateUpdateOp(typeName, snake, pkField, fields); err != nil {
			return err
		}
	}
	if ops["delete"] {
		if err := generateDeleteOp(typeName, snake, pkField); err != nil {
			return err
		}
	}
	return nil
}

// parseCrudOps normalises the crud parameter into a set of operation names.
func parseCrudOps(crud interface{}) map[string]bool {
	switch v := crud.(type) {
	case bool:
		if v {
			return map[string]bool{"read": true, "create": true, "update": true, "delete": true}
		}
	case []string:
		ops := make(map[string]bool, len(v))
		for _, op := range v {
			ops[op] = true
		}
		return ops
	}
	return nil
}

// pluralize applies basic English pluralization rules to a snake_case name.
//
// Rules (ordered):
//  1. Already ends in 's' (but not 'ss') → no change (e.g. 'statistics')
//  2. Ends in 'ss', 'sh', 'ch', 'x', 'z' → append 'es'
//  3. Ends in consonant + 'y' → replace 'y' with 'ies'
//  4. Default → append 's'
func pluralize(name string) string {
	if strings.HasSuffix(name, "s") && !strings.HasSuffix(name, "ss") {
		return name
	}
	for _, suffix := range []string{"ss", "sh", "ch", "x", "z"} {
		if strings.HasSuffix(name, suffix) {
			return name + "es"
		}
	}
	if len(name) >= 2 && name[len(name)-1] == 'y' && !strings.ContainsRune("aeiou", rune(name[len(name)-2])) {
		return name[:len(name)-1] + "ies"
	}
	return name + "s"
}

func generateReadOps(typeName, snake, view string, pkField FieldInfo) error {
	// Get-by-ID query
	err := RegisterQuery(QueryDefinition{
		Name:        snake,
		ReturnType:  typeName,
		ReturnsList: false,
		Nullable:    true,
		Arguments: []ArgumentDefinition{
			{Name: pkField.Name, Type: pkField.Type, Nullable: false},
		},
		Description: "Get " + typeName + " by ID.",
		SqlSource:   view,
	})
	if err != nil {
		return err
	}

	// List query with auto_params
	return RegisterQuery(QueryDefinition{
		Name:        pluralize(snake),
		ReturnType:  typeName,
		ReturnsList: true,
		Nullable:    false,
		Arguments:   []ArgumentDefinition{},
		Description: "List " + typeName + " records.",
		SqlSource:   view,
		Config: map[string]interface{}{
			"auto_params": map[string]interface{}{
				"where": true, "order_by": true, "limit": true, "offset": true,
			},
		},
	})
}

func generateCreateOp(typeName, snake string, fields []FieldInfo) error {
	args := make([]ArgumentDefinition, len(fields))
	for i, f := range fields {
		args[i] = ArgumentDefinition{Name: f.Name, Type: f.Type, Nullable: f.Nullable}
	}
	return RegisterMutation(MutationDefinition{
		Name:        "create_" + snake,
		ReturnType:  typeName,
		ReturnsList: false,
		Nullable:    false,
		Arguments:   args,
		Description: "Create a new " + typeName + ".",
		SqlSource:   "fn_create_" + snake,
		Operation:   "INSERT",
	})
}

func generateUpdateOp(typeName, snake string, pkField FieldInfo, fields []FieldInfo) error {
	args := []ArgumentDefinition{
		{Name: pkField.Name, Type: pkField.Type, Nullable: false},
	}
	for _, f := range fields[1:] {
		args = append(args, ArgumentDefinition{Name: f.Name, Type: f.Type, Nullable: true})
	}
	return RegisterMutation(MutationDefinition{
		Name:        "update_" + snake,
		ReturnType:  typeName,
		ReturnsList: false,
		Nullable:    true,
		Arguments:   args,
		Description: "Update an existing " + typeName + ".",
		SqlSource:   "fn_update_" + snake,
		Operation:   "UPDATE",
	})
}

func generateDeleteOp(typeName, snake string, pkField FieldInfo) error {
	return RegisterMutation(MutationDefinition{
		Name:        "delete_" + snake,
		ReturnType:  typeName,
		ReturnsList: false,
		Nullable:    false,
		Arguments: []ArgumentDefinition{
			{Name: pkField.Name, Type: pkField.Type, Nullable: false},
		},
		Description: "Delete a " + typeName + ".",
		SqlSource:   "fn_delete_" + snake,
		Operation:   "DELETE",
	})
}
