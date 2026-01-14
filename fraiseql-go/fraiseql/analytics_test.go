package fraiseql

import (
	"testing"
)

func TestNewFactTable(t *testing.T) {
	ftc := NewFactTable("sales")
	if ftc.name != "sales" {
		t.Errorf("expected name 'sales', got %q", ftc.name)
	}
	if ftc.tableName != "" {
		t.Errorf("expected empty tableName initially, got %q", ftc.tableName)
	}
}

func TestFactTableBuilder(t *testing.T) {
	// Clear registry for test
	defer Reset()

	ftc := NewFactTable("sales").
		TableName("tf_sales").
		Measure("revenue", "sum", "avg").
		Measure("quantity", "sum", "count").
		Dimension("category", "data->>'category'", "text").
		Dimension("region", "data->>'region'", "text").
		Description("Sales fact table for analytics")

	ftc.Register()

	if ftc.tableName != "tf_sales" {
		t.Errorf("expected tableName 'tf_sales', got %q", ftc.tableName)
	}

	if len(ftc.measures) != 2 {
		t.Errorf("expected 2 measures, got %d", len(ftc.measures))
	}

	if len(ftc.dimensions) != 2 {
		t.Errorf("expected 2 dimensions, got %d", len(ftc.dimensions))
	}

	// Verify measure details
	if ftc.measures[0].Name != "revenue" {
		t.Errorf("expected first measure name 'revenue', got %q", ftc.measures[0].Name)
	}

	if len(ftc.measures[0].Aggregates) != 2 {
		t.Errorf("expected 2 aggregates for revenue, got %d", len(ftc.measures[0].Aggregates))
	}

	// Verify dimension details
	if ftc.dimensions[0].Name != "category" {
		t.Errorf("expected first dimension name 'category', got %q", ftc.dimensions[0].Name)
	}

	if ftc.dimensions[0].DataType != "text" {
		t.Errorf("expected dimension data type 'text', got %q", ftc.dimensions[0].DataType)
	}
}

func TestAggregateQueryBuilder(t *testing.T) {
	// Clear registry for test
	defer Reset()

	aqc := NewAggregateQueryConfig("salesByCategory").
		FactTableName("sales").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Sales aggregated by category")

	if aqc.name != "salesByCategory" {
		t.Errorf("expected name 'salesByCategory', got %q", aqc.name)
	}

	if aqc.factTable != "sales" {
		t.Errorf("expected fact table 'sales', got %q", aqc.factTable)
	}

	if !aqc.autoGroupBy {
		t.Error("expected autoGroupBy to be true")
	}

	if !aqc.autoAggregates {
		t.Error("expected autoAggregates to be true")
	}
}

func TestFactTableRegistration(t *testing.T) {
	// Clear registry for test
	defer Reset()

	// Create and register a fact table
	NewFactTable("sales").
		TableName("tf_sales").
		Measure("revenue", "sum").
		Measure("quantity", "count").
		Dimension("category", "data->>'category'", "text").
		Register()

	// Verify it was registered
	schema := GetSchema()
	if len(schema.FactTables) != 1 {
		t.Errorf("expected 1 fact table in schema, got %d", len(schema.FactTables))
	}

	if schema.FactTables[0].Name != "sales" {
		t.Errorf("expected fact table name 'sales', got %q", schema.FactTables[0].Name)
	}

	if schema.FactTables[0].TableName != "tf_sales" {
		t.Errorf("expected table name 'tf_sales', got %q", schema.FactTables[0].TableName)
	}

	if len(schema.FactTables[0].Measures) != 2 {
		t.Errorf("expected 2 measures, got %d", len(schema.FactTables[0].Measures))
	}
}

func TestAggregateQueryRegistration(t *testing.T) {
	// Clear registry for test
	defer Reset()

	// Create and register an aggregate query
	NewAggregateQueryConfig("salesByCategory").
		FactTableName("sales").
		AutoGroupBy(true).
		AutoAggregates(true).
		Register()

	// Verify it was registered
	schema := GetSchema()
	if len(schema.AggregateQueries) != 1 {
		t.Errorf("expected 1 aggregate query in schema, got %d", len(schema.AggregateQueries))
	}

	if schema.AggregateQueries[0].Name != "salesByCategory" {
		t.Errorf("expected query name 'salesByCategory', got %q", schema.AggregateQueries[0].Name)
	}

	if schema.AggregateQueries[0].FactTable != "sales" {
		t.Errorf("expected fact table 'sales', got %q", schema.AggregateQueries[0].FactTable)
	}
}

func TestComplexAnalyticsSchema(t *testing.T) {
	// Clear registry for test
	defer Reset()

	// Create a complex analytics schema with multiple fact tables and queries
	NewFactTable("sales").
		TableName("tf_sales").
		Measure("revenue", "sum", "avg", "max").
		Measure("quantity", "sum", "count", "min").
		Measure("cost", "sum", "avg").
		Dimension("category", "data->>'category'", "text").
		Dimension("region", "data->>'region'", "text").
		Dimension("date", "data->>'date'", "date").
		Description("Sales fact table").
		Register()

	NewFactTable("events").
		TableName("tf_events").
		Measure("event_count", "count").
		Measure("duration", "avg", "sum").
		Dimension("event_type", "event_type", "text").
		Dimension("user_id", "user_id", "integer").
		Description("Events fact table").
		Register()

	NewAggregateQueryConfig("salesByCategory").
		FactTableName("sales").
		AutoGroupBy(true).
		AutoAggregates(true).
		Register()

	NewAggregateQueryConfig("eventsByType").
		FactTableName("events").
		AutoGroupBy(true).
		AutoAggregates(true).
		Register()

	schema := GetSchema()

	// Verify fact tables
	if len(schema.FactTables) != 2 {
		t.Errorf("expected 2 fact tables, got %d", len(schema.FactTables))
	}

	// Verify aggregate queries
	if len(schema.AggregateQueries) != 2 {
		t.Errorf("expected 2 aggregate queries, got %d", len(schema.AggregateQueries))
	}

	// Verify first fact table details
	salesTable := schema.FactTables[0]
	if len(salesTable.Measures) != 3 {
		t.Errorf("expected 3 measures in sales table, got %d", len(salesTable.Measures))
	}

	if len(salesTable.DimensionPaths) != 3 {
		t.Errorf("expected 3 dimensions in sales table, got %d", len(salesTable.DimensionPaths))
	}
}

func TestMeasureDefinition(t *testing.T) {
	measure := MeasureDefinition{
		Name:       "revenue",
		Aggregates: []string{"sum", "avg", "max"},
	}

	if measure.Name != "revenue" {
		t.Errorf("expected name 'revenue', got %q", measure.Name)
	}

	if len(measure.Aggregates) != 3 {
		t.Errorf("expected 3 aggregates, got %d", len(measure.Aggregates))
	}
}

func TestDimensionDefinition(t *testing.T) {
	dimension := Dimension{
		Name:     "category",
		JSONPath: "data->>'category'",
		DataType: "text",
	}

	if dimension.Name != "category" {
		t.Errorf("expected name 'category', got %q", dimension.Name)
	}

	if dimension.JSONPath != "data->>'category'" {
		t.Errorf("expected json_path \"data->>'category'\", got %q", dimension.JSONPath)
	}

	if dimension.DataType != "text" {
		t.Errorf("expected data_type 'text', got %q", dimension.DataType)
	}
}

func TestExtractMeasureNames(t *testing.T) {
	measures := []MeasureDefinition{
		{Name: "revenue", Aggregates: []string{"sum"}},
		{Name: "quantity", Aggregates: []string{"count"}},
		{Name: "cost", Aggregates: []string{"avg"}},
	}

	names := extractMeasureNames(measures)

	if len(names) != 3 {
		t.Errorf("expected 3 names, got %d", len(names))
	}

	expected := []string{"revenue", "quantity", "cost"}
	for i, name := range names {
		if name != expected[i] {
			t.Errorf("expected name %q at index %d, got %q", expected[i], i, name)
		}
	}
}

func TestConvertDimensionsToMap(t *testing.T) {
	dimensions := []Dimension{
		{Name: "category", JSONPath: "data->>'category'", DataType: "text"},
		{Name: "region", JSONPath: "data->>'region'", DataType: "text"},
	}

	result := convertDimensionsToMap(dimensions)

	if len(result) != 2 {
		t.Errorf("expected 2 dimensions, got %d", len(result))
	}

	// Verify first dimension
	if result[0]["name"] != "category" {
		t.Errorf("expected name 'category', got %v", result[0]["name"])
	}

	if result[0]["json_path"] != "data->>'category'" {
		t.Errorf("expected json_path \"data->>'category'\", got %v", result[0]["json_path"])
	}

	if result[0]["data_type"] != "text" {
		t.Errorf("expected data_type 'text', got %v", result[0]["data_type"])
	}
}
