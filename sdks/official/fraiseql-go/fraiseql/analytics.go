package fraiseql

import (
	"fmt"
	"sort"
	"strings"
)

// FactTableBuilder provides a fluent interface for building fact table definitions.
type FactTableBuilder struct {
	name        string
	tableName   string
	typeName    string
	rowScope    string
	inject      map[string]string
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

// RowScope declares who keeps one tenant's aggregate or window from reading another's rows
// (#1799). The only mode is RowScopeRelation ("relation"): the relation filters its own
// rows. In a schema that scopes its reads by inject params, a fact table read as such a
// type, or with a column they inject, is refused at compile without it, unless
// InjectParams scopes it.
func (b *FactTableBuilder) RowScope(scope string) *FactTableBuilder {
	b.rowScope = scope
	return b
}

// InjectParams declares filters the server composes into every aggregate and window over
// the table (#1800): each entry maps one of its denormalized filter columns to "jwt:<claim>",
// the value the column must equal. A caller without the claim, or without a token, is
// refused. Without it, the table inherits its linked type's inject params when the type
// declares any. Register refuses an optional claim, a column that is not a denormalized
// filter, and a declaration beside RowScope.
func (b *FactTableBuilder) InjectParams(params map[string]string) *FactTableBuilder {
	b.inject = params
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

// SemiAdditiveMeasure adds a measure reduced per entity and per bucket of over with using
// (one of ReduceLast, ReduceFirst, ReduceAvg, ReduceMin, ReduceMax), then aggregated across
// entities (#1459): a balance, a stock level. over is a denormalized time column; entity
// names the denormalized columns that identify what the value belongs to.
func (b *FactTableBuilder) SemiAdditiveMeasure(name, sqlType string, nullable bool, over, using string, entity ...string) *FactTableBuilder {
	return b.measure(name, sqlType, nullable, &Additivity{Kind: "semi_additive", Over: over, Using: using, Entity: entity})
}

// DeltaMeasure adds a measure whose change within a bucket (last - first) per entity is
// aggregated across entities (#1459): a cumulative counter.
func (b *FactTableBuilder) DeltaMeasure(name, sqlType string, nullable bool, over string, entity ...string) *FactTableBuilder {
	return b.measure(name, sqlType, nullable, &Additivity{Kind: "delta", Over: over, Entity: entity})
}

// NonAdditiveMeasure adds a measure every aggregate over which is refused (#1459): a ratio.
func (b *FactTableBuilder) NonAdditiveMeasure(name, sqlType string, nullable bool) *FactTableBuilder {
	return b.measure(name, sqlType, nullable, &Additivity{Kind: "non_additive"})
}

func (b *FactTableBuilder) measure(name, sqlType string, nullable bool, additivity *Additivity) *FactTableBuilder {
	b.measures = append(b.measures, MeasureDefinition{
		Name:       name,
		SqlType:    sqlType,
		Nullable:   nullable,
		Additivity: additivity,
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
// Returns an error if a fact table with the same name is already registered, or if its
// inject params cannot filter its rows.
func (b *FactTableBuilder) Register() error {
	if err := b.checkInjectParams(); err != nil {
		return err
	}
	var injectParams map[string]interface{}
	if len(b.inject) > 0 {
		injectParams = parseInjectParams(b.inject)
	}
	return RegisterFactTable(FactTableDefinition{
		TableName:    b.tableName,
		TypeName:     b.typeName,
		RowScope:     b.rowScope,
		InjectParams: injectParams,
		Measures:     b.measures,
		Dimensions: DimensionsDefinition{
			Name:  b.name,
			Paths: b.dimensions,
		},
		DenormalizedFilters: b.filters,
	})
}

// checkInjectParams refuses inject params the server cannot honour as filters on the
// table's rows (#1800).
func (b *FactTableBuilder) checkInjectParams() error {
	if len(b.inject) == 0 {
		return nil
	}
	owner := fmt.Sprintf("fact table %q", b.tableName)
	if b.rowScope != "" {
		return fmt.Errorf("%s: RowScope(%q) (the relation filters its own rows) and InjectParams (the server filters them): declare one", owner, b.rowScope)
	}
	filters := make([]string, 0, len(b.filters))
	for _, f := range b.filters {
		filters = append(filters, f.Name)
	}
	sort.Strings(filters)
	columns := make([]string, 0, len(b.inject))
	for column := range b.inject {
		columns = append(columns, column)
	}
	sort.Strings(columns)
	for _, column := range columns {
		claim, ok := strings.CutPrefix(b.inject[column], "jwt:")
		if !ok || claim == "" {
			return fmt.Errorf("%s: inject param %q is %q; it filters rows, and only \"jwt:<claim>\" can", owner, column, b.inject[column])
		}
		if i := sort.SearchStrings(filters, column); i == len(filters) || filters[i] != column {
			return fmt.Errorf("%s: inject param %q is not one of its denormalized filters %v", owner, column, filters)
		}
	}
	return nil
}
