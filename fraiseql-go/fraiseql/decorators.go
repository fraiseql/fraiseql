package fraiseql

import (
	"fmt"
	"reflect"
)

// operationBuilder holds the common fields and shared logic for QueryBuilder and MutationBuilder.
type operationBuilder struct {
	name        string
	returnType  string
	returnsList bool
	nullable    bool
	arguments   []ArgumentDefinition
	description string
	config      map[string]interface{}
}

func (b *operationBuilder) setReturnType(returnType interface{}) {
	switch v := returnType.(type) {
	case string:
		b.returnType = v
	default:
		b.returnType = getTypeName(returnType)
	}
}

func (b *operationBuilder) setReturnsArray(arr bool) {
	b.returnsList = arr
}

func (b *operationBuilder) setNullable(n bool) {
	b.nullable = n
}

func (b *operationBuilder) setConfig(cfg map[string]interface{}) {
	b.config = cfg
}

func (b *operationBuilder) addArg(name string, graphQLType string, defaultValue interface{}, nullable ...bool) {
	isNullable := false
	if len(nullable) > 0 {
		isNullable = nullable[0]
	}
	arg := ArgumentDefinition{
		Name:      name,
		Type:      graphQLType,
		Nullable:  isNullable,
		IsDefault: defaultValue != nil,
	}
	if defaultValue != nil {
		arg.Default = defaultValue
	}
	b.arguments = append(b.arguments, arg)
}

func (b *operationBuilder) setDescription(desc string) {
	b.description = desc
}

// QueryBuilder provides a fluent interface for building GraphQL queries
type QueryBuilder struct {
	operationBuilder
	relay bool
}

// NewQuery creates a new query builder
func NewQuery(name string) *QueryBuilder {
	return &QueryBuilder{
		operationBuilder: operationBuilder{
			name:      name,
			config:    make(map[string]interface{}),
			arguments: []ArgumentDefinition{},
		},
	}
}

// ReturnType sets the return type for the query
func (qb *QueryBuilder) ReturnType(returnType interface{}) *QueryBuilder {
	qb.setReturnType(returnType)
	return qb
}

// ReturnsArray sets whether the query returns an array
func (qb *QueryBuilder) ReturnsArray(b bool) *QueryBuilder {
	qb.setReturnsArray(b)
	return qb
}

// Nullable sets whether the return value can be null
func (qb *QueryBuilder) Nullable(b bool) *QueryBuilder {
	qb.setNullable(b)
	return qb
}

// Config sets the configuration for the query
func (qb *QueryBuilder) Config(config map[string]interface{}) *QueryBuilder {
	qb.setConfig(config)
	return qb
}

// Arg adds an argument to the query
// nullable is a variadic bool (defaults to false if not provided)
func (qb *QueryBuilder) Arg(name string, graphQLType string, defaultValue interface{}, nullable ...bool) *QueryBuilder {
	qb.addArg(name, graphQLType, defaultValue, nullable...)
	return qb
}

// Description sets the description for the query
func (qb *QueryBuilder) Description(desc string) *QueryBuilder {
	qb.setDescription(desc)
	return qb
}

// Relay marks the query as a Relay connection query.
// Requires ReturnsArray(true) and a sql_source in Config.
// The compiler derives the cursor column from the return type name (e.g. User -> pk_user).
func (qb *QueryBuilder) Relay(relay bool) *QueryBuilder {
	qb.relay = relay
	return qb
}

// Register registers the query with the global schema registry.
// Returns an error if a query with the same name is already registered.
func (qb *QueryBuilder) Register() error {
	if qb.relay {
		if !qb.returnsList {
			return fmt.Errorf(
				"query %q: Relay(true) requires ReturnsArray(true); relay connections only apply to list queries",
				qb.name,
			)
		}
		if qb.config["sql_source"] == "" || qb.config["sql_source"] == nil {
			return fmt.Errorf(
				"query %q: Relay(true) requires sql_source to be set via Config; the compiler needs the view name to derive the cursor column",
				qb.name,
			)
		}
	}

	definition := QueryDefinition{
		Name:        qb.name,
		ReturnType:  qb.returnType,
		ReturnsList: qb.returnsList,
		Nullable:    qb.nullable,
		Arguments:   qb.arguments,
		Description: qb.description,
		Relay:       qb.relay,
	}

	if len(qb.config) > 0 {
		remaining := make(map[string]interface{})
		for k, v := range qb.config {
			switch k {
			case "sql_source":
				if s, ok := v.(string); ok {
					definition.SqlSource = s
				}
			default:
				remaining[k] = v
			}
		}
		if len(remaining) > 0 {
			definition.Config = remaining
		}
	}

	return RegisterQuery(definition)
}

// MutationBuilder provides a fluent interface for building GraphQL mutations
type MutationBuilder struct {
	operationBuilder
}

// NewMutation creates a new mutation builder
func NewMutation(name string) *MutationBuilder {
	return &MutationBuilder{operationBuilder{
		name:      name,
		config:    make(map[string]interface{}),
		arguments: []ArgumentDefinition{},
	}}
}

// ReturnType sets the return type for the mutation
func (mb *MutationBuilder) ReturnType(returnType interface{}) *MutationBuilder {
	mb.setReturnType(returnType)
	return mb
}

// ReturnsArray sets whether the mutation returns an array
func (mb *MutationBuilder) ReturnsArray(b bool) *MutationBuilder {
	mb.setReturnsArray(b)
	return mb
}

// Nullable sets whether the return value can be null
func (mb *MutationBuilder) Nullable(b bool) *MutationBuilder {
	mb.setNullable(b)
	return mb
}

// Config sets the configuration for the mutation
func (mb *MutationBuilder) Config(config map[string]interface{}) *MutationBuilder {
	mb.setConfig(config)
	return mb
}

// Arg adds an argument to the mutation
// nullable is a variadic bool (defaults to false if not provided)
func (mb *MutationBuilder) Arg(name string, graphQLType string, defaultValue interface{}, nullable ...bool) *MutationBuilder {
	mb.addArg(name, graphQLType, defaultValue, nullable...)
	return mb
}

// Description sets the description for the mutation
func (mb *MutationBuilder) Description(desc string) *MutationBuilder {
	mb.setDescription(desc)
	return mb
}

// Register registers the mutation with the global schema registry.
// Returns an error if a mutation with the same name is already registered.
func (mb *MutationBuilder) Register() error {
	definition := MutationDefinition{
		Name:        mb.name,
		ReturnType:  mb.returnType,
		ReturnsList: mb.returnsList,
		Nullable:    mb.nullable,
		Arguments:   mb.arguments,
		Description: mb.description,
	}

	if len(mb.config) > 0 {
		remaining := make(map[string]interface{})
		for k, v := range mb.config {
			switch k {
			case "operation":
				if s, ok := v.(string); ok {
					definition.Operation = s
				}
			case "sql_source":
				if s, ok := v.(string); ok {
					definition.SqlSource = s
				}
			default:
				remaining[k] = v
			}
		}
		if len(remaining) > 0 {
			definition.Config = remaining
		}
	}

	return RegisterMutation(definition)
}

// NOTE: FactTableBuilder removed - use analytics.NewFactTable() instead
// The analytics module provides better-structured fact table builders
// with support for Measure and Dimension types.

// NOTE: AggregateQueryBuilder removed - use analytics.NewAggregateQueryConfig() instead
// The analytics module provides better-structured aggregate query builders.

// getTypeName gets the name of a type from a value
func getTypeName(v interface{}) string {
	t := reflect.TypeOf(v)
	if t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t.Kind() == reflect.Struct {
		return t.Name()
	}
	return ""
}
