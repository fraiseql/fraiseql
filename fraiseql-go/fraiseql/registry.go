package fraiseql

import (
	"encoding/json"
	"fmt"
	"reflect"
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

// TypeDefinition represents a GraphQL type
type TypeDefinition struct {
	Name        string      `json:"name"`
	Fields      []FieldInfo `json:"fields"`
	Description string      `json:"description,omitempty"`
}

// QueryDefinition represents a GraphQL query
type QueryDefinition struct {
	Name        string                 `json:"name"`
	ReturnType  string                 `json:"return_type"`
	ReturnsList bool                   `json:"returns_list"`
	Nullable    bool                   `json:"nullable"`
	Arguments   []ArgumentDefinition   `json:"arguments"`
	Description string                 `json:"description,omitempty"`
	SqlSource   string                 `json:"sql_source,omitempty"`
	Config      map[string]interface{} `json:"config,omitempty"`
}

// MutationDefinition represents a GraphQL mutation
type MutationDefinition struct {
	Name        string                 `json:"name"`
	ReturnType  string                 `json:"return_type"`
	ReturnsList bool                   `json:"returns_list"`
	Nullable    bool                   `json:"nullable"`
	Arguments   []ArgumentDefinition   `json:"arguments"`
	Description string                 `json:"description,omitempty"`
	Operation   string                 `json:"operation,omitempty"`
	SqlSource   string                 `json:"sql_source,omitempty"`
	Config      map[string]interface{} `json:"config,omitempty"`
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

// Schema represents the complete GraphQL schema
type Schema struct {
	Types            []TypeDefinition           `json:"types"`
	Queries          []QueryDefinition          `json:"queries"`
	Mutations        []MutationDefinition       `json:"mutations"`
	Subscriptions    []SubscriptionDefinition   `json:"subscriptions"`
	FactTables       []FactTableDefinition      `json:"fact_tables,omitempty"`
	AggregateQueries []AggregateQueryDefinition `json:"aggregate_queries,omitempty"`
	CustomScalars    []map[string]interface{}   `json:"custom_scalars,omitempty"`
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

// RegisterType registers a type with the schema registry.
// Returns an error if a type with the same name is already registered.
func RegisterType(name string, fields []FieldInfo, description string) error {
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
	}
	return nil
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

	for _, queryDef := range reg.queries {
		schema.Queries = append(schema.Queries, queryDef)
	}

	for _, mutationDef := range reg.mutations {
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

	// Also clear custom scalars
	ClearCustomScalars()
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
