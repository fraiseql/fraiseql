package fraiseql

// FactTableBuilder provides a fluent interface for building fact table definitions.
type FactTableBuilder struct {
	name        string
	tableName   string
	typeName    string
	measures    []MeasureDefinition
	dimensions  []DimensionPathDefinition
	filters     []FilterDefinition
	description string
}

// NewFactTable creates a new fact table builder with the given dimension-group name.
func NewFactTable(name string) *FactTableBuilder {
	return &FactTableBuilder{
		name:       name,
		measures:   []MeasureDefinition{},
		dimensions: []DimensionPathDefinition{},
		filters:    []FilterDefinition{},
	}
}

// TableName sets the underlying database table name for this fact table.
func (b *FactTableBuilder) TableName(name string) *FactTableBuilder {
	b.tableName = name
	return b
}

// TypeName links the fact table to the type it is read as. Its measures, denormalized
// filters and dimension paths are then fields of that type, and an aggregate or window over
// the table is gated as a read of it: the type's `requires_scope` and `authorize` fields and
// its `requires_role`. A schema whose linked type lacks one of the names the table declares
// is refused when it loads. An unlinked fact table declares no field gate.
func (b *FactTableBuilder) TypeName(name string) *FactTableBuilder {
	b.typeName = name
	return b
}

// Measure adds a numeric measure column with its SQL type.
//
// Aggregation functions are not declared here: `AutoAggregates` on the aggregate query
// derives them from the measure. This used to take `aggregations ...string` and fuse them
// into `"revenue:sum"` strings, which the compiler cannot deserialize.
func (b *FactTableBuilder) Measure(name, sqlType string, nullable bool) *FactTableBuilder {
	b.measures = append(b.measures, MeasureDefinition{
		Name:     name,
		SqlType:  sqlType,
		Nullable: nullable,
	})
	return b
}

// Dimension adds a dimension with its JSONB path and data type.
func (b *FactTableBuilder) Dimension(name, jsonPath, dataType string) *FactTableBuilder {
	b.dimensions = append(b.dimensions, DimensionPathDefinition{
		Name:     name,
		JsonPath: jsonPath,
		DataType: dataType,
	})
	return b
}

// DenormalizedFilter adds a flat filter column on the fact table.
func (b *FactTableBuilder) DenormalizedFilter(name, sqlType string, indexed bool) *FactTableBuilder {
	b.filters = append(b.filters, FilterDefinition{
		Name:    name,
		SqlType: sqlType,
		Indexed: indexed,
	})
	return b
}

// Description sets a human-readable description for this fact table.
func (b *FactTableBuilder) Description(desc string) *FactTableBuilder {
	b.description = desc
	return b
}

// Register registers the fact table with the global schema registry.
// Returns an error if a fact table with the same name is already registered.
func (b *FactTableBuilder) Register() error {
	return RegisterFactTable(FactTableDefinition{
		TableName: b.tableName,
		TypeName:  b.typeName,
		Measures:  b.measures,
		Dimensions: DimensionsDefinition{
			Name:  b.name,
			Paths: b.dimensions,
		},
		DenormalizedFilters: b.filters,
	})
}
