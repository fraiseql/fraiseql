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
	Name        string       `json:"name"`
	Fields      []FieldInfo  `json:"fields"`
	Description string       `json:"description,omitempty"`
}

// QueryDefinition represents a GraphQL query
type QueryDefinition struct {
	Name        string                 `json:"name"`
	ReturnType  string                 `json:"return_type"`
	ReturnsList bool                   `json:"returns_list"`
	Nullable    bool                   `json:"nullable"`
	Arguments   []ArgumentDefinition   `json:"arguments"`
	Description string                 `json:"description,omitempty"`
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
	Name             string                 `json:"name"`
	FactTable        string                 `json:"fact_table"`
	AutoGroupBy      bool                   `json:"auto_group_by"`
	AutoAggregates   bool                   `json:"auto_aggregates"`
	Description      string                 `json:"description,omitempty"`
	Config           map[string]interface{} `json:"config,omitempty"`
}

// Schema represents the complete GraphQL schema
type Schema struct {
	Types           []TypeDefinition        `json:"types"`
	Queries         []QueryDefinition       `json:"queries"`
	Mutations       []MutationDefinition    `json:"mutations"`
	FactTables      []FactTableDefinition   `json:"fact_tables,omitempty"`
	AggregateQueries []AggregateQueryDefinition `json:"aggregate_queries,omitempty"`
}

// SchemaRegistry is a singleton registry for collecting types, queries, mutations
type SchemaRegistry struct {
	mu                 sync.RWMutex
	types              map[string]TypeDefinition
	queries            map[string]QueryDefinition
	mutations          map[string]MutationDefinition
	factTables         map[string]FactTableDefinition
	aggregateQueries   map[string]AggregateQueryDefinition
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
			factTables:       make(map[string]FactTableDefinition),
			aggregateQueries: make(map[string]AggregateQueryDefinition),
		}
	})
	return registry
}

// RegisterType registers a type with the schema registry
func RegisterType(name string, fields []FieldInfo, description string) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.types[name] = TypeDefinition{
		Name:        name,
		Fields:      fields,
		Description: description,
	}
}

// RegisterQuery registers a query with the schema registry
func RegisterQuery(definition QueryDefinition) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.queries[definition.Name] = definition
}

// RegisterMutation registers a mutation with the schema registry
func RegisterMutation(definition MutationDefinition) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.mutations[definition.Name] = definition
}

// RegisterFactTable registers a fact table with the schema registry
func RegisterFactTable(definition FactTableDefinition) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.factTables[definition.Name] = definition
}

// RegisterAggregateQuery registers an aggregate query with the schema registry
func RegisterAggregateQuery(definition AggregateQueryDefinition) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.aggregateQueries[definition.Name] = definition
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

	for _, factTable := range reg.factTables {
		schema.FactTables = append(schema.FactTables, factTable)
	}

	for _, aggregateQuery := range reg.aggregateQueries {
		schema.AggregateQueries = append(schema.AggregateQueries, aggregateQuery)
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
	reg.factTables = make(map[string]FactTableDefinition)
	reg.aggregateQueries = make(map[string]AggregateQueryDefinition)
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
