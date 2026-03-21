package fraiseql

import (
	"fmt"
	"reflect"
	"strings"
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

// parseInjectParams converts {"param": "jwt:claim"} to {"param": {"source": "jwt", "claim": "claim"}}.
func parseInjectParams(params map[string]string) map[string]interface{} {
	result := make(map[string]interface{}, len(params))
	for k, v := range params {
		parts := strings.SplitN(v, ":", 2)
		if len(parts) == 2 {
			result[k] = map[string]interface{}{
				"source": parts[0],
				"claim":  parts[1],
			}
		}
	}
	return result
}

// QueryBuilder provides a fluent interface for building GraphQL queries
type QueryBuilder struct {
	operationBuilder
	relay             bool
	relayCursorColumn string
	relayCursorType   string
	injectParams      map[string]interface{}
	cacheTTLSeconds   *uint64
	additionalViews   []string
	requiresRole      string
	deprecation       *DeprecationInfo
	restPath          string
	restMethod        string
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

// SqlSource sets the SQL view name for this query.
func (qb *QueryBuilder) SqlSource(source string) *QueryBuilder {
	qb.config["sql_source"] = source
	return qb
}

// Relay marks the query as a Relay connection query.
// Requires ReturnsArray(true) and a sql_source set via Config or SqlSource.
// The compiler derives the cursor column from the return type name (e.g. User -> pk_user).
func (qb *QueryBuilder) Relay(relay bool) *QueryBuilder {
	qb.relay = relay
	return qb
}

// RelayCursorColumn sets the column used as the Relay pagination cursor.
func (qb *QueryBuilder) RelayCursorColumn(col string) *QueryBuilder {
	qb.relayCursorColumn = col
	return qb
}

// RelayCursorType sets the type of the relay cursor column ("int64" or "uuid").
func (qb *QueryBuilder) RelayCursorType(cursorType string) *QueryBuilder {
	qb.relayCursorType = cursorType
	return qb
}

// InjectParams sets server-side JWT claim injections for this query.
// Each entry maps a parameter name to a source string of the form "jwt:<claim>".
func (qb *QueryBuilder) InjectParams(params map[string]string) *QueryBuilder {
	if len(params) > 0 {
		qb.injectParams = parseInjectParams(params)
	}
	return qb
}

// CacheTTLSeconds sets the cache TTL in seconds for query results.
// A value of 0 disables caching for this query explicitly.
func (qb *QueryBuilder) CacheTTLSeconds(ttl uint64) *QueryBuilder {
	qb.cacheTTLSeconds = &ttl
	return qb
}

// AdditionalViews lists extra views that should be invalidated when this query's
// backing view changes. Used by the cache invalidation system.
func (qb *QueryBuilder) AdditionalViews(views []string) *QueryBuilder {
	qb.additionalViews = views
	return qb
}

// RequiresRole restricts this query to callers who hold the given role.
func (qb *QueryBuilder) RequiresRole(role string) *QueryBuilder {
	qb.requiresRole = role
	return qb
}

// Deprecated marks this query as deprecated with the given reason.
func (qb *QueryBuilder) Deprecated(reason string) *QueryBuilder {
	qb.deprecation = &DeprecationInfo{Reason: reason}
	return qb
}

// RestPath sets the REST endpoint path for this query.
func (qb *QueryBuilder) RestPath(path string) *QueryBuilder {
	qb.restPath = path
	return qb
}

// RestMethod sets the HTTP method for the REST endpoint.
// Defaults to GET for queries. Must be one of: GET, POST, PUT, PATCH, DELETE.
func (qb *QueryBuilder) RestMethod(method string) *QueryBuilder {
	qb.restMethod = method
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

	var rest *RestAnnotation
	if qb.restPath != "" {
		method := strings.ToUpper(qb.restMethod)
		if method == "" {
			method = "GET"
		}
		rest = &RestAnnotation{Path: qb.restPath, Method: method}
	}

	definition := QueryDefinition{
		Name:              qb.name,
		ReturnType:        qb.returnType,
		ReturnsList:       qb.returnsList,
		Nullable:          qb.nullable,
		Arguments:         qb.arguments,
		Description:       qb.description,
		Relay:             qb.relay,
		RelayCursorColumn: qb.relayCursorColumn,
		RelayCursorType:   qb.relayCursorType,
		InjectParams:      qb.injectParams,
		CacheTTLSeconds:   qb.cacheTTLSeconds,
		AdditionalViews:   qb.additionalViews,
		RequiresRole:      qb.requiresRole,
		Deprecation:       qb.deprecation,
		Rest:              rest,
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
	injectParams          map[string]interface{}
	invalidatesViews      []string
	invalidatesFactTables []string
	deprecation           *DeprecationInfo
	restPath              string
	restMethod            string
}

// NewMutation creates a new mutation builder
func NewMutation(name string) *MutationBuilder {
	return &MutationBuilder{
		operationBuilder: operationBuilder{
			name:      name,
			config:    make(map[string]interface{}),
			arguments: []ArgumentDefinition{},
		},
	}
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

// SqlSource sets the SQL function name for this mutation.
func (mb *MutationBuilder) SqlSource(source string) *MutationBuilder {
	mb.config["sql_source"] = source
	return mb
}

// Operation sets the DML operation type for this mutation ("insert", "update", "delete").
func (mb *MutationBuilder) Operation(op string) *MutationBuilder {
	mb.config["operation"] = op
	return mb
}

// InjectParams sets server-side JWT claim injections for this mutation.
// Each entry maps a parameter name to a source string of the form "jwt:<claim>".
func (mb *MutationBuilder) InjectParams(params map[string]string) *MutationBuilder {
	if len(params) > 0 {
		mb.injectParams = parseInjectParams(params)
	}
	return mb
}

// InvalidatesViews lists views whose cached results should be invalidated when this
// mutation runs successfully.
func (mb *MutationBuilder) InvalidatesViews(views []string) *MutationBuilder {
	mb.invalidatesViews = views
	return mb
}

// InvalidatesFactTables lists fact tables whose cached aggregates should be
// invalidated when this mutation runs successfully.
func (mb *MutationBuilder) InvalidatesFactTables(tables []string) *MutationBuilder {
	mb.invalidatesFactTables = tables
	return mb
}

// Deprecated marks this mutation as deprecated with the given reason.
func (mb *MutationBuilder) Deprecated(reason string) *MutationBuilder {
	mb.deprecation = &DeprecationInfo{Reason: reason}
	return mb
}

// RestPath sets the REST endpoint path for this mutation.
func (mb *MutationBuilder) RestPath(path string) *MutationBuilder {
	mb.restPath = path
	return mb
}

// RestMethod sets the HTTP method for the REST endpoint.
// Defaults to POST for mutations. Must be one of: GET, POST, PUT, PATCH, DELETE.
func (mb *MutationBuilder) RestMethod(method string) *MutationBuilder {
	mb.restMethod = method
	return mb
}

// Register registers the mutation with the global schema registry.
// Returns an error if a mutation with the same name is already registered.
func (mb *MutationBuilder) Register() error {
	var rest *RestAnnotation
	if mb.restPath != "" {
		method := strings.ToUpper(mb.restMethod)
		if method == "" {
			method = "POST"
		}
		rest = &RestAnnotation{Path: mb.restPath, Method: method}
	}

	definition := MutationDefinition{
		Name:                 mb.name,
		ReturnType:           mb.returnType,
		ReturnsList:          mb.returnsList,
		Nullable:             mb.nullable,
		Arguments:            mb.arguments,
		Description:          mb.description,
		InjectParams:         mb.injectParams,
		InvalidatesViews:     mb.invalidatesViews,
		InvalidatesFactTables: mb.invalidatesFactTables,
		Deprecation:          mb.deprecation,
		Rest:                 rest,
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
