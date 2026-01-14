package fraiseql

import "reflect"

// QueryBuilder provides a fluent interface for building GraphQL queries
type QueryBuilder struct {
	name        string
	returnType  string
	returnsList bool
	nullable    bool
	arguments   []ArgumentDefinition
	description string
	config      map[string]interface{}
}

// NewQuery creates a new query builder
func NewQuery(name string) *QueryBuilder {
	return &QueryBuilder{
		name:      name,
		config:    make(map[string]interface{}),
		arguments: []ArgumentDefinition{},
	}
}

// ReturnType sets the return type for the query
func (qb *QueryBuilder) ReturnType(returnType interface{}) *QueryBuilder {
	switch v := returnType.(type) {
	case string:
		qb.returnType = v
	default:
		// Try to get the type name from reflect
		qb.returnType = getTypeName(returnType)
	}
	return qb
}

// ReturnsArray sets whether the query returns an array
func (qb *QueryBuilder) ReturnsArray(b bool) *QueryBuilder {
	qb.returnsList = b
	return qb
}

// Nullable sets whether the return value can be null
func (qb *QueryBuilder) Nullable(b bool) *QueryBuilder {
	qb.nullable = b
	return qb
}

// Config sets the configuration for the query
func (qb *QueryBuilder) Config(config map[string]interface{}) *QueryBuilder {
	qb.config = config
	return qb
}

// Arg adds an argument to the query
// nullable is a variadic bool (defaults to false if not provided)
func (qb *QueryBuilder) Arg(name string, graphQLType string, defaultValue interface{}, nullable ...bool) *QueryBuilder {
	isNullable := false
	if len(nullable) > 0 {
		isNullable = nullable[0]
	}

	arg := ArgumentDefinition{
		Name:        name,
		Type:        graphQLType,
		Nullable:    isNullable,
		IsDefault:   defaultValue != nil,
	}

	if defaultValue != nil {
		arg.Default = defaultValue
	}

	qb.arguments = append(qb.arguments, arg)
	return qb
}

// Description sets the description for the query
func (qb *QueryBuilder) Description(desc string) *QueryBuilder {
	qb.description = desc
	return qb
}

// Register registers the query with the global schema registry
func (qb *QueryBuilder) Register() {
	definition := QueryDefinition{
		Name:        qb.name,
		ReturnType:  qb.returnType,
		ReturnsList: qb.returnsList,
		Nullable:    qb.nullable,
		Arguments:   qb.arguments,
		Description: qb.description,
	}

	// Merge config into the definition
	if len(qb.config) > 0 {
		definition.Config = qb.config
	}

	RegisterQuery(definition)
}

// MutationBuilder provides a fluent interface for building GraphQL mutations
type MutationBuilder struct {
	name        string
	returnType  string
	returnsList bool
	nullable    bool
	arguments   []ArgumentDefinition
	description string
	config      map[string]interface{}
}

// NewMutation creates a new mutation builder
func NewMutation(name string) *MutationBuilder {
	return &MutationBuilder{
		name:      name,
		config:    make(map[string]interface{}),
		arguments: []ArgumentDefinition{},
	}
}

// ReturnType sets the return type for the mutation
func (mb *MutationBuilder) ReturnType(returnType interface{}) *MutationBuilder {
	switch v := returnType.(type) {
	case string:
		mb.returnType = v
	default:
		// Try to get the type name from reflect
		mb.returnType = getTypeName(returnType)
	}
	return mb
}

// ReturnsArray sets whether the mutation returns an array
func (mb *MutationBuilder) ReturnsArray(b bool) *MutationBuilder {
	mb.returnsList = b
	return mb
}

// Nullable sets whether the return value can be null
func (mb *MutationBuilder) Nullable(b bool) *MutationBuilder {
	mb.nullable = b
	return mb
}

// Config sets the configuration for the mutation
func (mb *MutationBuilder) Config(config map[string]interface{}) *MutationBuilder {
	mb.config = config
	return mb
}

// Arg adds an argument to the mutation
// nullable is a variadic bool (defaults to false if not provided)
func (mb *MutationBuilder) Arg(name string, graphQLType string, defaultValue interface{}, nullable ...bool) *MutationBuilder {
	isNullable := false
	if len(nullable) > 0 {
		isNullable = nullable[0]
	}

	arg := ArgumentDefinition{
		Name:        name,
		Type:        graphQLType,
		Nullable:    isNullable,
		IsDefault:   defaultValue != nil,
	}

	if defaultValue != nil {
		arg.Default = defaultValue
	}

	mb.arguments = append(mb.arguments, arg)
	return mb
}

// Description sets the description for the mutation
func (mb *MutationBuilder) Description(desc string) *MutationBuilder {
	mb.description = desc
	return mb
}

// Register registers the mutation with the global schema registry
func (mb *MutationBuilder) Register() {
	definition := MutationDefinition{
		Name:        mb.name,
		ReturnType:  mb.returnType,
		ReturnsList: mb.returnsList,
		Nullable:    mb.nullable,
		Arguments:   mb.arguments,
		Description: mb.description,
	}

	// Merge config into the definition
	if len(mb.config) > 0 {
		definition.Config = mb.config
	}

	RegisterMutation(definition)
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
