package fraiseql

import (
	"encoding/json"
	"fmt"
	"reflect"
	"strings"
	"sync"
)

// ArgumentDefinition represents a GraphQL argument
type ArgumentDefinition struct {
	Name      string      `json:"name"`
	Type      string      `json:"type"`
	Nullable  bool        `json:"nullable"`
	Default   interface{} `json:"default,omitempty"`
	IsDefault bool        `json:"-"` // Track whether default was set
}

// DeprecationInfo carries the deprecation reason for a query or mutation.
type DeprecationInfo struct {
	Reason string `json:"reason"`
}

// TypeDefinition represents a GraphQL type
type TypeDefinition struct {
	Name         string      `json:"name"`
	Fields       []FieldInfo `json:"fields"`
	Description  string      `json:"description,omitempty"`
	Relay        bool        `json:"relay,omitempty"`
	SqlSource    string      `json:"sql_source,omitempty"`
	JsonbColumn  string      `json:"jsonb_column,omitempty"`
	IsError      bool        `json:"is_error,omitempty"`
	RequiresRole string      `json:"requires_role,omitempty"`
	Implements   []string    `json:"implements,omitempty"`
	TenantScoped bool        `json:"tenant_scoped,omitempty"`
	Crud         interface{} `json:"-"` // bool or []string; not serialized, used for CRUD generation
	KeyFields    []string    `json:"key_fields,omitempty"`
	Extends      bool        `json:"extends,omitempty"`
}

// QueryDefinition represents a GraphQL query
type QueryDefinition struct {
	Name              string                 `json:"name"`
	ReturnType        string                 `json:"return_type"`
	ReturnsList       bool                   `json:"returns_list"`
	Nullable          bool                   `json:"nullable"`
	Arguments         []ArgumentDefinition   `json:"arguments"`
	Description       string                 `json:"description,omitempty"`
	SqlSource         string                 `json:"sql_source,omitempty"`
	Relay             bool                   `json:"relay,omitempty"`
	RelayCursorColumn string                 `json:"relay_cursor_column,omitempty"`
	RelayCursorType   string                 `json:"relay_cursor_type,omitempty"`
	InjectParams      map[string]interface{} `json:"inject_params,omitempty"`
	CacheTTLSeconds   *uint64                `json:"cache_ttl_seconds,omitempty"`
	AdditionalViews   []string               `json:"additional_views,omitempty"`
	RequiresRole      string                 `json:"requires_role,omitempty"`
	Deprecation       *DeprecationInfo       `json:"deprecation,omitempty"`
	Rest              *RestAnnotation        `json:"rest,omitempty"`
	Config            map[string]interface{} `json:"config,omitempty"`
}

// RestAnnotation holds REST endpoint metadata for a query or mutation.
type RestAnnotation struct {
	Path   string `json:"path"`
	Method string `json:"method"`
}

// MutationDefinition represents a GraphQL mutation
type MutationDefinition struct {
	Name                 string                 `json:"name"`
	ReturnType           string                 `json:"return_type"`
	ReturnsList          bool                   `json:"returns_list"`
	Nullable             bool                   `json:"nullable"`
	Arguments            []ArgumentDefinition   `json:"arguments"`
	Description          string                 `json:"description,omitempty"`
	Operation            string                 `json:"operation,omitempty"`
	SqlSource            string                 `json:"sql_source,omitempty"`
	InjectParams         map[string]interface{} `json:"inject_params,omitempty"`
	InvalidatesViews     []string               `json:"invalidates_views,omitempty"`
	InvalidatesFactTables []string              `json:"invalidates_fact_tables,omitempty"`
	Deprecation          *DeprecationInfo       `json:"deprecation,omitempty"`
	Rest                *RestAnnotation        `json:"rest,omitempty"`
	Config               map[string]interface{} `json:"config,omitempty"`
}

// FactTableDefinition represents a GraphQL fact table for analytics
type FactTableDefinition struct {
	Name           string                   `json:"name"`
	TableName      string                   `json:"table_name"`
	Measures       []string                 `json:"measures"`
	DimensionPaths []map[string]interface{} `json:"dimension_paths"`
	Description    string                   `json:"description,omitempty"`
}

// AggregateQueryDefinition represents a GraphQL aggregate query
type AggregateQueryDefinition struct {
	Name           string                 `json:"name"`
	FactTable      string                 `json:"fact_table"`
	AutoGroupBy    bool                   `json:"auto_group_by"`
	AutoAggregates bool                   `json:"auto_aggregates"`
	Description    string                 `json:"description,omitempty"`
	Config         map[string]interface{} `json:"config,omitempty"`
}

// SubscriptionDefinition represents a GraphQL subscription
// Subscriptions in FraiseQL are compiled projections of database events.
// They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
type SubscriptionDefinition struct {
	Name        string                 `json:"name"`
	EntityType  string                 `json:"entity_type"`
	Nullable    bool                   `json:"nullable"`
	Arguments   []ArgumentDefinition   `json:"arguments"`
	Description string                 `json:"description,omitempty"`
	Topic       string                 `json:"topic,omitempty"`
	Operation   string                 `json:"operation,omitempty"`
	Config      map[string]interface{} `json:"config,omitempty"`
}

// FederationConfig is the top-level federation block in the exported JSON.
type FederationConfig struct {
	Enabled       bool               `json:"enabled"`
	ServiceName   string             `json:"service_name"`
	ApolloVersion int                `json:"apollo_version"`
	Entities      []FederationEntity `json:"entities"`
}

// FederationEntity represents a single federation entity.
type FederationEntity struct {
	Name      string   `json:"name"`
	KeyFields []string `json:"key_fields"`
}

// Schema represents the complete GraphQL schema
type Schema struct {
	Types            []TypeDefinition           `json:"types"`
	Queries          []QueryDefinition          `json:"queries"`
	Mutations        []MutationDefinition       `json:"mutations"`
	Subscriptions    []SubscriptionDefinition   `json:"subscriptions"`
	FactTables       []FactTableDefinition      `json:"fact_tables,omitempty"`
	AggregateQueries []AggregateQueryDefinition `json:"aggregate_queries,omitempty"`
	CustomScalars    []map[string]interface{}   `json:"custom_scalars,omitempty"`
	Federation       *FederationConfig          `json:"federation,omitempty"`
}

// SchemaRegistry is a singleton registry for collecting types, queries, mutations, and subscriptions
type SchemaRegistry struct {
	mu               sync.RWMutex
	types            map[string]TypeDefinition
	queries          map[string]QueryDefinition
	mutations        map[string]MutationDefinition
	subscriptions    map[string]SubscriptionDefinition
	factTables       map[string]FactTableDefinition
	aggregateQueries map[string]AggregateQueryDefinition

	// inject_defaults: base applies to both queries and mutations;
	// query/mutation-specific maps override base.
	injectDefaults          map[string]string
	injectDefaultsQueries   map[string]string
	injectDefaultsMutations map[string]string
}

// Global registry instance
var registry *SchemaRegistry
var once sync.Once

// getInstance returns the singleton registry
func getInstance() *SchemaRegistry {
	once.Do(func() {
		registry = &SchemaRegistry{
			types:            make(map[string]TypeDefinition),
			queries:          make(map[string]QueryDefinition),
			mutations:        make(map[string]MutationDefinition),
			subscriptions:    make(map[string]SubscriptionDefinition),
			factTables:       make(map[string]FactTableDefinition),
			aggregateQueries: make(map[string]AggregateQueryDefinition),
		}
	})
	return registry
}

// pascalToSnake converts PascalCase to snake_case without any prefix.
// Examples: "OrderItem" → "order_item", "User" → "user".
func pascalToSnake(s string) string {
	result := make([]byte, 0, len(s)+4)
	for i := 0; i < len(s); i++ {
		ch := s[i]
		if i > 0 && ch >= 'A' && ch <= 'Z' {
			result = append(result, '_')
		}
		if ch >= 'A' && ch <= 'Z' {
			result = append(result, ch+32)
		} else {
			result = append(result, ch)
		}
	}
	return string(result)
}

// toSnakeCase converts CamelCase to snake_case.
// Examples: "OrderItem" → "order_item", "User" → "user".
func toSnakeCase(s string) string {
	result := make([]byte, 0, len(s)+4)
	for i := 0; i < len(s); i++ {
		ch := s[i]
		if i > 0 && ch >= 'A' && ch <= 'Z' {
			result = append(result, '_')
		}
		if ch >= 'A' && ch <= 'Z' {
			result = append(result, ch+32) // to lower ASCII
		} else {
			result = append(result, ch)
		}
	}
	return string(result)
}

// RegisterType registers a type with the schema registry.
// sql_source is automatically derived as "v_" + snake_case(name).
// Returns an error if a type with the same name is already registered.
func RegisterType(name string, fields []FieldInfo, description string, relay ...bool) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.types[name]; exists {
		return fmt.Errorf("type %q is already registered; each name must be unique within a schema", name)
	}
	isRelay := len(relay) > 0 && relay[0]
	reg.types[name] = TypeDefinition{
		Name:        name,
		Fields:      fields,
		Description: description,
		Relay:       isRelay,
		SqlSource:   "v_" + toSnakeCase(name),
	}
	return nil
}

// RegisterErrorType registers a GraphQL error type with the schema registry.
// Error types are used to return structured error responses from mutations.
// Returns an error if a type with the same name is already registered.
func RegisterErrorType(name string, fields []FieldInfo, description string) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.types[name]; exists {
		return fmt.Errorf("type %q is already registered; each name must be unique within a schema", name)
	}
	reg.types[name] = TypeDefinition{
		Name:        name,
		Fields:      fields,
		Description: description,
		IsError:     true,
		SqlSource:   "v_" + toSnakeCase(name),
	}
	return nil
}

// RegisterTypeAdvanced registers a type using the full TypeDefinition struct.
// This allows setting TenantScoped, Crud, and other advanced fields.
// If SqlSource is empty, it is automatically derived as "v_" + snake_case(name).
// If Crud is set (bool true or []string of operations), CRUD queries/mutations
// are auto-generated for the type.
// Returns an error if a type with the same name is already registered.
func RegisterTypeAdvanced(def TypeDefinition) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.types[def.Name]; exists {
		return fmt.Errorf("type %q is already registered; each name must be unique within a schema", def.Name)
	}
	if def.SqlSource == "" {
		def.SqlSource = "v_" + toSnakeCase(def.Name)
	}
	reg.types[def.Name] = def

	// Generate CRUD operations if requested
	if def.Crud != nil {
		reg.mu.Unlock()
		err := generateCrudOperations(def.Name, def.Fields, def.Crud, def.SqlSource)
		reg.mu.Lock()
		if err != nil {
			return fmt.Errorf("CRUD generation for type %q failed: %w", def.Name, err)
		}
	}
	return nil
}

// SetInjectDefaults configures default inject_params that are merged into every
// query and/or mutation at schema export time. The base map applies to both
// queries and mutations. The queries and mutations maps override base for their
// respective operation types. Pass nil for any map you don't need.
func SetInjectDefaults(base, queries, mutations map[string]string) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.injectDefaults = base
	reg.injectDefaultsQueries = queries
	reg.injectDefaultsMutations = mutations
}

// RegisterQuery registers a query with the schema registry.
// Returns an error if a query with the same name is already registered.
func RegisterQuery(definition QueryDefinition) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.queries[definition.Name]; exists {
		return fmt.Errorf("query %q is already registered; each name must be unique within a schema", definition.Name)
	}
	reg.queries[definition.Name] = definition
	return nil
}

// RegisterMutation registers a mutation with the schema registry.
// Returns an error if a mutation with the same name is already registered.
func RegisterMutation(definition MutationDefinition) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.mutations[definition.Name]; exists {
		return fmt.Errorf("mutation %q is already registered; each name must be unique within a schema", definition.Name)
	}
	reg.mutations[definition.Name] = definition
	return nil
}

// RegisterFactTable registers a fact table with the schema registry.
// Returns an error if a fact table with the same name is already registered.
func RegisterFactTable(definition FactTableDefinition) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.factTables[definition.Name]; exists {
		return fmt.Errorf("fact table %q is already registered; each name must be unique within a schema", definition.Name)
	}
	reg.factTables[definition.Name] = definition
	return nil
}

// RegisterAggregateQuery registers an aggregate query with the schema registry.
// Returns an error if an aggregate query with the same name is already registered.
func RegisterAggregateQuery(definition AggregateQueryDefinition) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.aggregateQueries[definition.Name]; exists {
		return fmt.Errorf("aggregate query %q is already registered; each name must be unique within a schema", definition.Name)
	}
	reg.aggregateQueries[definition.Name] = definition
	return nil
}

// RegisterSubscription registers a subscription with the schema registry.
// Subscriptions in FraiseQL are compiled projections of database events.
// They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
// Returns an error if a subscription with the same name is already registered.
func RegisterSubscription(definition SubscriptionDefinition) error {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.subscriptions[definition.Name]; exists {
		return fmt.Errorf("subscription %q is already registered; each name must be unique within a schema", definition.Name)
	}
	reg.subscriptions[definition.Name] = definition
	return nil
}

// GetRegistry returns the singleton registry instance
func GetRegistry() *SchemaRegistry {
	return getInstance()
}

// GetSchema returns the complete schema as a Schema struct
func GetSchema() Schema {
	reg := getInstance()
	reg.mu.RLock()
	defer reg.mu.RUnlock()

	schema := Schema{}

	// Convert maps to slices
	for _, typeDef := range reg.types {
		schema.Types = append(schema.Types, typeDef)
	}

	// Build merged inject defaults for queries: base + query-specific
	queryDefaults := mergeStringMaps(reg.injectDefaults, reg.injectDefaultsQueries)
	// Build merged inject defaults for mutations: base + mutation-specific
	mutationDefaults := mergeStringMaps(reg.injectDefaults, reg.injectDefaultsMutations)

	for _, queryDef := range reg.queries {
		queryDef = applyInjectDefaults(queryDef, queryDefaults)
		schema.Queries = append(schema.Queries, queryDef)
	}

	for _, mutationDef := range reg.mutations {
		mutationDef = applyInjectDefaultsMutation(mutationDef, mutationDefaults)
		schema.Mutations = append(schema.Mutations, mutationDef)
	}

	for _, subscriptionDef := range reg.subscriptions {
		schema.Subscriptions = append(schema.Subscriptions, subscriptionDef)
	}

	for _, factTable := range reg.factTables {
		schema.FactTables = append(schema.FactTables, factTable)
	}

	for _, aggregateQuery := range reg.aggregateQueries {
		schema.AggregateQueries = append(schema.AggregateQueries, aggregateQuery)
	}

	// Include custom scalars
	customScalars := GetAllCustomScalars()
	for name := range customScalars {
		schema.CustomScalars = append(schema.CustomScalars, map[string]interface{}{
			"name": name,
		})
	}

	return schema
}

// GetSchemaJSON returns the schema as JSON bytes
func GetSchemaJSON(pretty bool) ([]byte, error) {
	schema := GetSchema()

	if pretty {
		return json.MarshalIndent(schema, "", "  ")
	}
	return json.Marshal(schema)
}

// Reset clears the registry (useful for testing)
func Reset() {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.types = make(map[string]TypeDefinition)
	reg.queries = make(map[string]QueryDefinition)
	reg.mutations = make(map[string]MutationDefinition)
	reg.subscriptions = make(map[string]SubscriptionDefinition)
	reg.factTables = make(map[string]FactTableDefinition)
	reg.aggregateQueries = make(map[string]AggregateQueryDefinition)
	reg.injectDefaults = nil
	reg.injectDefaultsQueries = nil
	reg.injectDefaultsMutations = nil

	// Also clear custom scalars
	ClearCustomScalars()
}

// mergeStringMaps returns a new map with all entries from base, overridden by entries in overlay.
func mergeStringMaps(base, overlay map[string]string) map[string]string {
	if len(base) == 0 && len(overlay) == 0 {
		return nil
	}
	merged := make(map[string]string)
	for k, v := range base {
		merged[k] = v
	}
	for k, v := range overlay {
		merged[k] = v
	}
	return merged
}

// parseInjectParamValue converts "jwt:claim_name" into {"source":"jwt","claim":"claim_name"}.
func parseInjectParamValue(v string) map[string]interface{} {
	parts := strings.SplitN(v, ":", 2)
	if len(parts) == 2 {
		return map[string]interface{}{
			"source": parts[0],
			"claim":  parts[1],
		}
	}
	return map[string]interface{}{"source": v}
}

// applyInjectDefaults merges default inject params into a query definition.
// Existing params on the query take precedence over defaults.
func applyInjectDefaults(q QueryDefinition, defaults map[string]string) QueryDefinition {
	if len(defaults) == 0 {
		return q
	}
	if q.InjectParams == nil {
		q.InjectParams = make(map[string]interface{})
	}
	for k, v := range defaults {
		if _, exists := q.InjectParams[k]; !exists {
			q.InjectParams[k] = parseInjectParamValue(v)
		}
	}
	return q
}

// applyInjectDefaultsMutation merges default inject params into a mutation definition.
// Existing params on the mutation take precedence over defaults.
func applyInjectDefaultsMutation(m MutationDefinition, defaults map[string]string) MutationDefinition {
	if len(defaults) == 0 {
		return m
	}
	if m.InjectParams == nil {
		m.InjectParams = make(map[string]interface{})
	}
	for k, v := range defaults {
		if _, exists := m.InjectParams[k]; !exists {
			m.InjectParams[k] = parseInjectParamValue(v)
		}
	}
	return m
}

// RegisterTypes extracts fields from Go struct types and registers them
func RegisterTypes(types ...interface{}) error {
	for _, t := range types {
		structType := reflect.TypeOf(t)
		if structType.Kind() == reflect.Pointer {
			structType = structType.Elem()
		}

		if structType.Kind() != reflect.Struct {
			return fmt.Errorf("expected struct type, got %v", structType.Kind())
		}

		fields, err := ExtractFields(structType)
		if err != nil {
			return fmt.Errorf("failed to extract fields from %s: %w", structType.Name(), err)
		}

		// Convert map to slice of FieldInfo
		var fieldSlice []FieldInfo
		for _, field := range fields {
			fieldSlice = append(fieldSlice, field)
		}

		RegisterType(structType.Name(), fieldSlice, "")
	}

	return nil
}
