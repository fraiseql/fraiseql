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

// FactTableBuilder provides a fluent interface for building fact tables
type FactTableBuilder struct {
	name           string
	tableName      string
	measures       []string
	dimensionPaths []map[string]interface{}
	description    string
}

// NewFactTable creates a new fact table builder
func NewFactTable(name string) *FactTableBuilder {
	return &FactTableBuilder{
		name:           name,
		dimensionPaths: []map[string]interface{}{},
	}
}

// TableName sets the underlying table name
func (fb *FactTableBuilder) TableName(tableName string) *FactTableBuilder {
	fb.tableName = tableName
	return fb
}

// Measures sets the measure columns
func (fb *FactTableBuilder) Measures(measures []string) *FactTableBuilder {
	fb.measures = measures
	return fb
}

// Dimensions sets the dimension paths
func (fb *FactTableBuilder) Dimensions(dimensions []map[string]interface{}) *FactTableBuilder {
	fb.dimensionPaths = dimensions
	return fb
}

// Description sets the description for the fact table
func (fb *FactTableBuilder) Description(desc string) *FactTableBuilder {
	fb.description = desc
	return fb
}

// Register registers the fact table with the global schema registry
func (fb *FactTableBuilder) Register() {
	definition := FactTableDefinition{
		Name:           fb.name,
		TableName:      fb.tableName,
		Measures:       fb.measures,
		DimensionPaths: fb.dimensionPaths,
		Description:    fb.description,
	}
	RegisterFactTable(definition)
}

// AggregateQueryBuilder provides a fluent interface for building aggregate queries
type AggregateQueryBuilder struct {
	name            string
	factTable       string
	autoGroupBy     bool
	autoAggregates  bool
	description     string
	config          map[string]interface{}
}

// NewAggregateQuery creates a new aggregate query builder
func NewAggregateQuery(name string) *AggregateQueryBuilder {
	return &AggregateQueryBuilder{
		name:   name,
		config: make(map[string]interface{}),
	}
}

// FactTable sets the fact table name
func (aqb *AggregateQueryBuilder) FactTable(factTable string) *AggregateQueryBuilder {
	aqb.factTable = factTable
	return aqb
}

// AutoGroupBy sets whether to auto group by
func (aqb *AggregateQueryBuilder) AutoGroupBy(b bool) *AggregateQueryBuilder {
	aqb.autoGroupBy = b
	return aqb
}

// AutoAggregates sets whether to auto aggregate
func (aqb *AggregateQueryBuilder) AutoAggregates(b bool) *AggregateQueryBuilder {
	aqb.autoAggregates = b
	return aqb
}

// Description sets the description for the aggregate query
func (aqb *AggregateQueryBuilder) Description(desc string) *AggregateQueryBuilder {
	aqb.description = desc
	return aqb
}

// Config sets additional configuration
func (aqb *AggregateQueryBuilder) Config(config map[string]interface{}) *AggregateQueryBuilder {
	aqb.config = config
	return aqb
}

// Register registers the aggregate query with the global schema registry
func (aqb *AggregateQueryBuilder) Register() {
	definition := AggregateQueryDefinition{
		Name:           aqb.name,
		FactTable:      aqb.factTable,
		AutoGroupBy:    aqb.autoGroupBy,
		AutoAggregates: aqb.autoAggregates,
		Description:    aqb.description,
	}

	if len(aqb.config) > 0 {
		definition.Config = aqb.config
	}

	RegisterAggregateQuery(definition)
}

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
