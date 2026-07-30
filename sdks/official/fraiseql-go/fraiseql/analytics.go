package fraiseql

// FactTableBuilder provides a fluent interface for building fact table definitions.
type FactTableBuilder struct {
	name        string
	tableName   string
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
		Measures:  b.measures,
		Dimensions: DimensionsDefinition{
			Name:  b.name,
			Paths: b.dimensions,
		},
		DenormalizedFilters: b.filters,
	})
}

// AggregateQueryBuilder provides a fluent interface for building aggregate query definitions.
type AggregateQueryBuilder struct {
	name           string
	factTableName  string
	autoGroupBy    bool
	autoAggregates bool
	description    string
	config         map[string]interface{}
}

// NewAggregateQueryConfig creates a new aggregate query builder with the given name.
func NewAggregateQueryConfig(name string) *AggregateQueryBuilder {
	return &AggregateQueryBuilder{
		name:   name,
		config: make(map[string]interface{}),
	}
}

// FactTableName sets the name of the fact table this aggregate query operates on.
func (b *AggregateQueryBuilder) FactTableName(name string) *AggregateQueryBuilder {
	b.factTableName = name
	return b
}

// AutoGroupBy enables automatic GROUP BY inference from the fact table dimensions.
func (b *AggregateQueryBuilder) AutoGroupBy(enabled bool) *AggregateQueryBuilder {
	b.autoGroupBy = enabled
	return b
}

// AutoAggregates enables automatic aggregate function generation from the fact table measures.
func (b *AggregateQueryBuilder) AutoAggregates(enabled bool) *AggregateQueryBuilder {
	b.autoAggregates = enabled
	return b
}

// Description sets a human-readable description for this aggregate query.
func (b *AggregateQueryBuilder) Description(desc string) *AggregateQueryBuilder {
	b.description = desc
	return b
}

// Register registers the aggregate query with the global schema registry.
// Returns an error if an aggregate query with the same name is already registered.
func (b *AggregateQueryBuilder) Register() error {
	return RegisterAggregateQuery(AggregateQueryDefinition{
		Name:           b.name,
		FactTable:      b.factTableName,
		AutoGroupBy:    b.autoGroupBy,
		AutoAggregates: b.autoAggregates,
		Description:    b.description,
		Config:         b.config,
	})
}
