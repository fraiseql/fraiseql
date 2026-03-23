package fraiseql

// FactTableBuilder provides a fluent interface for building fact table definitions.
type FactTableBuilder struct {
	name        string
	tableName   string
	measures    []string
	dimensions  []map[string]interface{}
	description string
}

// NewFactTable creates a new fact table builder with the given logical name.
func NewFactTable(name string) *FactTableBuilder {
	return &FactTableBuilder{
		name:       name,
		measures:   []string{},
		dimensions: []map[string]interface{}{},
	}
}

// TableName sets the underlying database table name for this fact table.
func (b *FactTableBuilder) TableName(name string) *FactTableBuilder {
	b.tableName = name
	return b
}

// Measure adds a named measure with one or more aggregation functions (sum, avg, count, max, min).
func (b *FactTableBuilder) Measure(name string, aggregations ...string) *FactTableBuilder {
	for _, agg := range aggregations {
		b.measures = append(b.measures, name+":"+agg)
	}
	return b
}

// Dimension adds a named dimension with an SQL expression and data type.
func (b *FactTableBuilder) Dimension(name, expression, dataType string) *FactTableBuilder {
	b.dimensions = append(b.dimensions, map[string]interface{}{
		"name":       name,
		"expression": expression,
		"data_type":  dataType,
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
		Name:           b.name,
		TableName:      b.tableName,
		Measures:       b.measures,
		DimensionPaths: b.dimensions,
		Description:    b.description,
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
