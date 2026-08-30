package fraiseql

import (
	"encoding/json"
	"fmt"
	"reflect"
	"sort"
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
}

// QueryDefinition represents a GraphQL query
// RestAnnotation describes a REST endpoint mapping for a query or mutation.
type RestAnnotation struct {
	Path   string `json:"path"`
	Method string `json:"method"`
}

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
	RequiresActor     []string               `json:"requires_actor,omitempty"`
	Deprecation       *DeprecationInfo       `json:"deprecation,omitempty"`
	Rest              *RestAnnotation        `json:"rest,omitempty"`
	// Config is an SDK-internal bag of builder settings and is never serialized: the
	// compiler has no `config` key and, denying unknown fields, rejects the whole
	// schema when it sees one. The only setting that ever reached it was
	// `sql_source_dispatch`, whose builders were removed in #926 — the compiler
	// implements no dynamic source selection, so the setting could never do anything.
	// One query per source is the supported pattern.
	Config map[string]interface{} `json:"-"`
}

// MutationDefinition represents a GraphQL mutation
type MutationDefinition struct {
	Name                  string                 `json:"name"`
	ReturnType            string                 `json:"return_type"`
	ReturnsList           bool                   `json:"returns_list"`
	Nullable              bool                   `json:"nullable"`
	Arguments             []ArgumentDefinition   `json:"arguments"`
	Description           string                 `json:"description,omitempty"`
	Operation             string                 `json:"operation,omitempty"`
	SqlSource             string                 `json:"sql_source,omitempty"`
	InjectParams          map[string]interface{} `json:"inject_params,omitempty"`
	InvalidatesViews      []string               `json:"invalidates_views,omitempty"`
	InvalidatesFactTables []string               `json:"invalidates_fact_tables,omitempty"`
	Cascade               bool                   `json:"cascade,omitempty"`
	RequiresRole          string                 `json:"requires_role,omitempty"`
	RequiresActor         []string               `json:"requires_actor,omitempty"`
	Deprecation           *DeprecationInfo       `json:"deprecation,omitempty"`
	Rest                  *RestAnnotation        `json:"rest,omitempty"`
	// See QueryDefinition.Config — an SDK-internal bag, never serialized.
	Config map[string]interface{} `json:"-"`
}

// MeasureDefinition is one numeric measure column on a fact table.
//
// The compiler reads `measures` as a list of objects. This used to be `[]string` carrying
// `"revenue:sum"`-shaped entries — a measure name fused with an aggregation function —
// which fails deserialization outright, so no Go analytics export could be compiled. The
// aggregation functions are not part of the declaration: `auto_aggregates` on the
// aggregate query derives them.
type MeasureDefinition struct {
	Name     string `json:"name"`
	SqlType  string `json:"sql_type"`
	Nullable bool   `json:"nullable"`
}

// DimensionPathDefinition locates one dimension inside the fact table's JSONB.
type DimensionPathDefinition struct {
	Name     string `json:"name"`
	JsonPath string `json:"json_path"`
	DataType string `json:"data_type"`
}

// DimensionsDefinition is the fact table's named group of dimension paths.
type DimensionsDefinition struct {
	Name  string                    `json:"name"`
	Paths []DimensionPathDefinition `json:"paths"`
}

// FilterDefinition is a denormalized filter column on the fact table.
type FilterDefinition struct {
	Name    string `json:"name"`
	SqlType string `json:"sql_type"`
	Indexed bool   `json:"indexed"`
}

// FactTableDefinition represents a GraphQL fact table for analytics.
//
// The shape mirrors the compiler's `IntermediateFactTable`: `table_name`, object
// `measures`, a `dimensions` group and `denormalized_filters`. It previously carried a
// `name` the compiler does not read and `dimension_paths` where the compiler reads a
// `dimensions` object, on top of the `measures` mismatch above.
type FactTableDefinition struct {
	TableName           string               `json:"table_name"`
	Measures            []MeasureDefinition  `json:"measures"`
	Dimensions          DimensionsDefinition `json:"dimensions"`
	DenormalizedFilters []FilterDefinition   `json:"denormalized_filters"`
}

// SubscriptionFilterCondition maps one subscription argument onto a JSON path in the
// event payload.
type SubscriptionFilterCondition struct {
	Argument string `json:"argument"`
	Path     string `json:"path"`
}

// SubscriptionFilter narrows which events a subscription delivers.
type SubscriptionFilter struct {
	Conditions []SubscriptionFilterCondition `json:"conditions"`
}

// SubscriptionDefinition represents a GraphQL subscription
// Subscriptions in FraiseQL are compiled projections of database events.
// They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
//
// This is the compiler's IntermediateSubscription, member for member. It used to carry
// EntityType/Nullable/Operation and a free-form Config, none of which that struct has —
// and it denies unknown fields, so a document declaring any subscription was refused
// whole at `fraiseql compile` (#1024). EntityType survives as the authoring spelling on
// the builder; the field emitted here is ReturnType.
type SubscriptionDefinition struct {
	Name        string               `json:"name"`
	ReturnType  string               `json:"return_type"`
	Arguments   []ArgumentDefinition `json:"arguments"`
	Description string               `json:"description,omitempty"`
	Topic       string               `json:"topic,omitempty"`
	Filter      *SubscriptionFilter  `json:"filter,omitempty"`
	Fields      []string             `json:"fields,omitempty"`
	Deprecated  *DeprecationInfo     `json:"deprecated,omitempty"`
}

// EnumValueDefinition represents a single value in a GraphQL enum.
type EnumValueDefinition struct {
	Name string `json:"name"`
}

// EnumDefinition represents a GraphQL enum type.
type EnumDefinition struct {
	Name   string                `json:"name"`
	Values []EnumValueDefinition `json:"values"`
}

// InputObjectDefinition represents a GraphQL input object type (spec §3.10).
//
// Input objects are the only legal type for a mutation argument. Without them a Go
// author had to declare the argument as an output type, producing a schema that
// introspection-driven clients reject and that fails federation composition.
type InputObjectDefinition struct {
	Name        string      `json:"name"`
	Fields      []FieldInfo `json:"fields"`
	Description string      `json:"description,omitempty"`
}

// Schema represents the complete GraphQL schema.
//
// Every slice carries `omitempty`. GetSchema starts from a zero Schema{} and only
// appends, so a category with no registrations stays nil — and a nil slice marshals to
// JSON `null`, not `[]`. The compiler's `#[serde(default)] Vec<T>` covers an *absent*
// key but hands an explicit `null` to Vec::deserialize, which fails the whole compile
// with `invalid type: null, expected a sequence` and no indication of which key is at
// fault. Types/Queries/Mutations/Subscriptions lacked `omitempty` while their siblings
// had it, and subscriptions are the common trigger because almost no schema registers
// one: every shipped Go example printed "✅ Schema exported successfully" and was then
// rejected by the very next command it told the user to run (#850).
type Schema struct {
	Types          []TypeDefinition         `json:"types,omitempty"`
	Enums          []EnumDefinition         `json:"enums,omitempty"`
	InputTypes     []InputObjectDefinition  `json:"input_types,omitempty"`
	Queries        []QueryDefinition        `json:"queries,omitempty"`
	Mutations      []MutationDefinition     `json:"mutations,omitempty"`
	Subscriptions  []SubscriptionDefinition `json:"subscriptions,omitempty"`
	FactTables     []FactTableDefinition    `json:"fact_tables,omitempty"`
	Observers      []ObserverDefinition     `json:"observers,omitempty"`
	CustomScalars  []map[string]interface{} `json:"custom_scalars,omitempty"`
	InjectDefaults *InjectDefaults          `json:"inject_defaults,omitempty"`
}

// InjectDefaults holds the default inject_params loaded from fraiseql.toml.
// Base defaults apply to both queries and mutations; section-specific maps
// override the base for their respective operation type.
type InjectDefaults struct {
	Base      map[string]string `json:"base,omitempty"`
	Queries   map[string]string `json:"queries,omitempty"`
	Mutations map[string]string `json:"mutations,omitempty"`
}

// SchemaRegistry is a singleton registry for collecting types, queries, mutations, and subscriptions
type SchemaRegistry struct {
	mu             sync.RWMutex
	types          map[string]TypeDefinition
	enums          map[string]EnumDefinition
	inputTypes     map[string]InputObjectDefinition
	queries        map[string]QueryDefinition
	mutations      map[string]MutationDefinition
	subscriptions  map[string]SubscriptionDefinition
	factTables     map[string]FactTableDefinition
	observers      map[string]ObserverDefinition
	injectDefaults *InjectDefaults
}

// Global registry instance
var registry *SchemaRegistry
var once sync.Once

// getInstance returns the singleton registry
func getInstance() *SchemaRegistry {
	once.Do(func() {
		registry = &SchemaRegistry{
			types:         make(map[string]TypeDefinition),
			enums:         make(map[string]EnumDefinition),
			queries:       make(map[string]QueryDefinition),
			mutations:     make(map[string]MutationDefinition),
			subscriptions: make(map[string]SubscriptionDefinition),
			factTables:    make(map[string]FactTableDefinition),
			observers:     make(map[string]ObserverDefinition),
		}
	})
	return registry
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

// validateVectorFields refuses the vector declarations that are wrong on their own
// terms, before the compiler sees them.
//
// Only these two: a field is either an embedding or the Float reporting how far a
// search's result was from the query vector, and a column has at least one dimension.
// Which metrics a field type admits and which index types have an operator class for
// them depends on pgvector's own tables, and is checked once, in the compiler.
func validateVectorFields(typeName string, fields []FieldInfo) error {
	for _, f := range fields {
		if f.Vector != nil && f.VectorDistance != "" {
			return fmt.Errorf(
				"field %q of type %q declares both a vector config and a vector distance; "+
					"a field is either an embedding or the Float reporting a search's distance, not both",
				f.Name, typeName)
		}
		if f.Vector != nil && f.Vector.Dimensions < 1 {
			return fmt.Errorf(
				"field %q of type %q declares %d vector dimensions; dimensions must be at least 1",
				f.Name, typeName, f.Vector.Dimensions)
		}
	}
	return nil
}

// RegisterType registers a type with the schema registry.
// sql_source is automatically derived as "v_" + snake_case(name).
// Returns an error if a type with the same name is already registered.
func RegisterType(name string, fields []FieldInfo, description string, relay ...bool) error {
	if err := validateVectorFields(name, fields); err != nil {
		return err
	}
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
	if err := validateVectorFields(name, fields); err != nil {
		return err
	}
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

	// Keyed by `table_name`: the compiler's `IntermediateFactTable` has no `name`, and
	// the backing table is what makes two declarations the same fact table.
	if _, exists := reg.factTables[definition.TableName]; exists {
		return fmt.Errorf("fact table %q is already registered; each table must be declared once within a schema", definition.TableName)
	}
	reg.factTables[definition.TableName] = definition
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

// SetInjectDefaults stores default inject_params that are applied to queries
// and mutations at schema export time.
//
// Parameters:
//   - base: defaults applied to both queries and mutations (e.g., {"tenant_id": "jwt:tenant_id"})
//   - queries: query-specific overrides that supplement the base defaults
//   - mutations: mutation-specific overrides that supplement the base defaults
func SetInjectDefaults(base, queries, mutations map[string]string) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.injectDefaults = &InjectDefaults{
		Base:      base,
		Queries:   queries,
		Mutations: mutations,
	}
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

	// Convert maps to slices, in sorted-name order (#929). The registries are
	// maps and Go randomizes map iteration, so an unsorted export changed from
	// run to run — two builds of the same schema produced different artifacts.
	for _, name := range sortedKeys(reg.types) {
		schema.Types = append(schema.Types, reg.types[name])
	}

	for _, name := range sortedKeys(reg.enums) {
		schema.Enums = append(schema.Enums, reg.enums[name])
	}

	for _, name := range sortedKeys(reg.inputTypes) {
		schema.InputTypes = append(schema.InputTypes, reg.inputTypes[name])
	}

	for _, name := range sortedKeys(reg.queries) {
		schema.Queries = append(schema.Queries, reg.queries[name])
	}

	for _, name := range sortedKeys(reg.mutations) {
		schema.Mutations = append(schema.Mutations, reg.mutations[name])
	}

	for _, name := range sortedKeys(reg.subscriptions) {
		schema.Subscriptions = append(schema.Subscriptions, reg.subscriptions[name])
	}

	for _, name := range sortedKeys(reg.factTables) {
		schema.FactTables = append(schema.FactTables, reg.factTables[name])
	}

	for _, name := range sortedKeys(reg.observers) {
		schema.Observers = append(schema.Observers, reg.observers[name])
	}

	if reg.injectDefaults != nil {
		schema.InjectDefaults = reg.injectDefaults
	}

	// Include custom scalars (sorted for the same reproducibility contract)
	customScalars := GetAllCustomScalars()
	for _, name := range sortedKeys(customScalars) {
		schema.CustomScalars = append(schema.CustomScalars, map[string]interface{}{
			"name": name,
		})
	}

	return schema
}

// sortedKeys returns the map's keys in ascending order — the reproducible-export
// contract for every registry category (#929).
func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
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
	reg.enums = make(map[string]EnumDefinition)
	reg.inputTypes = make(map[string]InputObjectDefinition)
	reg.queries = make(map[string]QueryDefinition)
	reg.mutations = make(map[string]MutationDefinition)
	reg.subscriptions = make(map[string]SubscriptionDefinition)
	reg.factTables = make(map[string]FactTableDefinition)
	reg.observers = make(map[string]ObserverDefinition)
	reg.injectDefaults = nil

	// Also clear custom scalars
	ClearCustomScalars()
}

// ClearRegistry clears the registry (alias for Reset, used in tests)
func ClearRegistry() {
	Reset()
}

// Enum registers a GraphQL enum type with the schema registry.
//
// Members are exported in the order given (#929). The previous signature took a
// map[string]string, which had two defects: Go randomizes map iteration, so the
// exported member order changed from run to run (two builds of the same schema
// produced different artifacts, and the SDK conformance gate was a coin flip);
// and the map's values were silently dropped — only the keys were ever
// exported, so any name→value mapping an author wrote did nothing.
func Enum(name string, members ...string) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	enumValues := make([]EnumValueDefinition, 0, len(members))
	for _, memberName := range members {
		enumValues = append(enumValues, EnumValueDefinition{Name: memberName})
	}

	reg.enums[name] = EnumDefinition{
		Name:   name,
		Values: enumValues,
	}
}

// RegisterInputType registers a GraphQL input object type with the schema registry.
// Returns an error if a name is registered twice.
func RegisterInputType(name string, fields []FieldInfo, description string) error {
	if err := validateVectorFields(name, fields); err != nil {
		return err
	}
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	if _, exists := reg.inputTypes[name]; exists {
		return fmt.Errorf("input type %q is already registered; each name must be unique within a schema", name)
	}
	reg.inputTypes[name] = InputObjectDefinition{
		Name:        name,
		Fields:      fields,
		Description: description,
	}
	return nil
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
