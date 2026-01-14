package fraiseql

// Analytics support for FraiseQL fact tables and aggregate queries
// This module enables high-performance OLAP workloads with fact tables,
// measures, and dimensions.

// Dimension represents a single dimension for a fact table
type Dimension struct {
	Name     string `json:"name"`
	JSONPath string `json:"json_path"`
	DataType string `json:"data_type"`
}

// MeasureDefinition represents a measure in a fact table
type MeasureDefinition struct {
	Name       string `json:"name"`
	Aggregates []string `json:"aggregates,omitempty"` // sum, avg, count, min, max, etc.
}

// FactTable represents a GraphQL fact table for analytics
type FactTable struct {
	Name       string                 `json:"name"`
	TableName  string                 `json:"table_name"`
	Measures   []MeasureDefinition    `json:"measures"`
	Dimensions []Dimension            `json:"dimensions"`
	Description string                `json:"description,omitempty"`
	Config     map[string]interface{} `json:"config,omitempty"`
}

// AggregateQuery represents a GraphQL aggregate query
type AggregateQuery struct {
	Name           string                 `json:"name"`
	FactTable      string                 `json:"fact_table"`
	AutoGroupBy    bool                   `json:"auto_group_by"`
	AutoAggregates bool                   `json:"auto_aggregates"`
	Description    string                 `json:"description,omitempty"`
	Config         map[string]interface{} `json:"config,omitempty"`
}

// RegisterFactTable registers a fact table with the schema registry
func RegisterFactTableDef(definition FactTable) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.factTables[definition.Name] = FactTableDefinition{
		Name:           definition.Name,
		TableName:      definition.TableName,
		Measures:       extractMeasureNames(definition.Measures),
		DimensionPaths: convertDimensionsToMap(definition.Dimensions),
		Description:    definition.Description,
	}
}

// RegisterAggregateQueryDef registers an aggregate query with the schema registry
func RegisterAggregateQueryDef(definition AggregateQuery) {
	reg := getInstance()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	reg.aggregateQueries[definition.Name] = AggregateQueryDefinition{
		Name:           definition.Name,
		FactTable:      definition.FactTable,
		AutoGroupBy:    definition.AutoGroupBy,
		AutoAggregates: definition.AutoAggregates,
		Description:    definition.Description,
	}

	if len(definition.Config) > 0 {
		reg.aggregateQueries[definition.Name] = AggregateQueryDefinition{
			Name:           definition.Name,
			FactTable:      definition.FactTable,
			AutoGroupBy:    definition.AutoGroupBy,
			AutoAggregates: definition.AutoAggregates,
			Description:    definition.Description,
			Config:         definition.Config,
		}
	}
}

// Helper function to extract measure names from MeasureDefinition slice
func extractMeasureNames(measures []MeasureDefinition) []string {
	names := make([]string, len(measures))
	for i, m := range measures {
		names[i] = m.Name
	}
	return names
}

// Helper function to convert Dimension slice to map slice
func convertDimensionsToMap(dimensions []Dimension) []map[string]interface{} {
	result := make([]map[string]interface{}, len(dimensions))
	for i, d := range dimensions {
		result[i] = map[string]interface{}{
			"name":      d.Name,
			"json_path": d.JSONPath,
			"data_type": d.DataType,
		}
	}
	return result
}

// FactTableConfig is a helper struct for building fact table configurations
type FactTableConfig struct {
	name           string
	tableName      string
	measures       []MeasureDefinition
	dimensions     []Dimension
	description    string
	config         map[string]interface{}
}

// NewFactTable creates a new fact table configuration builder
func NewFactTable(name string) *FactTableConfig {
	return &FactTableConfig{
		name:       name,
		measures:   []MeasureDefinition{},
		dimensions: []Dimension{},
		config:     make(map[string]interface{}),
	}
}

// TableName sets the underlying database table name
func (ftc *FactTableConfig) TableName(tableName string) *FactTableConfig {
	ftc.tableName = tableName
	return ftc
}

// Measure adds a measure to the fact table
func (ftc *FactTableConfig) Measure(name string, aggregates ...string) *FactTableConfig {
	measure := MeasureDefinition{
		Name:       name,
		Aggregates: aggregates,
	}
	ftc.measures = append(ftc.measures, measure)
	return ftc
}

// Dimension adds a dimension to the fact table
func (ftc *FactTableConfig) Dimension(name string, jsonPath string, dataType string) *FactTableConfig {
	dimension := Dimension{
		Name:     name,
		JSONPath: jsonPath,
		DataType: dataType,
	}
	ftc.dimensions = append(ftc.dimensions, dimension)
	return ftc
}

// Description sets the fact table description
func (ftc *FactTableConfig) Description(desc string) *FactTableConfig {
	ftc.description = desc
	return ftc
}

// Config sets additional configuration
func (ftc *FactTableConfig) Config(config map[string]interface{}) *FactTableConfig {
	ftc.config = config
	return ftc
}

// Register registers the fact table with the global schema registry
func (ftc *FactTableConfig) Register() {
	definition := FactTable{
		Name:        ftc.name,
		TableName:   ftc.tableName,
		Measures:    ftc.measures,
		Dimensions:  ftc.dimensions,
		Description: ftc.description,
	}

	if len(ftc.config) > 0 {
		definition.Config = ftc.config
	}

	RegisterFactTableDef(definition)
}

// AggregateQueryConfig is a helper struct for building aggregate query configurations
type AggregateQueryConfig struct {
	name            string
	factTable       string
	autoGroupBy     bool
	autoAggregates  bool
	description     string
	config          map[string]interface{}
}

// NewAggregateQuery creates a new aggregate query configuration builder
func NewAggregateQueryConfig(name string) *AggregateQueryConfig {
	return &AggregateQueryConfig{
		name:   name,
		config: make(map[string]interface{}),
	}
}

// FactTableName sets the fact table this query operates on
func (aqc *AggregateQueryConfig) FactTableName(tableName string) *AggregateQueryConfig {
	aqc.factTable = tableName
	return aqc
}

// AutoGroupBy enables automatic GROUP BY behavior
func (aqc *AggregateQueryConfig) AutoGroupBy(enable bool) *AggregateQueryConfig {
	aqc.autoGroupBy = enable
	return aqc
}

// AutoAggregates enables automatic aggregate function generation
func (aqc *AggregateQueryConfig) AutoAggregates(enable bool) *AggregateQueryConfig {
	aqc.autoAggregates = enable
	return aqc
}

// Description sets the aggregate query description
func (aqc *AggregateQueryConfig) Description(desc string) *AggregateQueryConfig {
	aqc.description = desc
	return aqc
}

// Config sets additional configuration
func (aqc *AggregateQueryConfig) Config(config map[string]interface{}) *AggregateQueryConfig {
	aqc.config = config
	return aqc
}

// Register registers the aggregate query with the global schema registry
func (aqc *AggregateQueryConfig) Register() {
	definition := AggregateQuery{
		Name:           aqc.name,
		FactTable:      aqc.factTable,
		AutoGroupBy:    aqc.autoGroupBy,
		AutoAggregates: aqc.autoAggregates,
		Description:    aqc.description,
	}

	if len(aqc.config) > 0 {
		definition.Config = aqc.config
	}

	RegisterAggregateQueryDef(definition)
}
