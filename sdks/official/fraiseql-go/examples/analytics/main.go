package main

import (
	"log"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// Sale represents a fact table for sales analytics
type Sale struct {
	ID        int     `fraiseql:"id,type=Int"`
	Revenue   float64 `fraiseql:"revenue,type=Float"`
	Quantity  int     `fraiseql:"quantity,type=Int"`
	Cost      float64 `fraiseql:"cost,type=Float"`
	Category  string  `fraiseql:"category,type=String"`
	Region    string  `fraiseql:"region,type=String"`
	OccurredAt string `fraiseql:"occurredAt,type=String"`
}

// Event represents a fact table for event analytics
type Event struct {
	ID        int    `fraiseql:"id,type=Int"`
	EventType string `fraiseql:"eventType,type=String"`
	Duration  int    `fraiseql:"duration,type=Int"`
	UserID    string `fraiseql:"userId,type=String"`
	OccurredAt string `fraiseql:"occurredAt,type=String"`
}

func init() {
	// Register fact tables for analytics
	fraiseql.NewFactTable("sales").
		TableName("tf_sales").
		Measure("revenue", "sum", "avg", "max", "min").
		Measure("quantity", "sum", "count", "avg").
		Measure("cost", "sum", "avg").
		Dimension("category", "data->>'category'", "text").
		Dimension("region", "data->>'region'", "text").
		Dimension("year_month", "date_trunc('month', occurred_at)::text", "text").
		Description("Sales fact table for OLAP analysis").
		Register()

	fraiseql.NewFactTable("events").
		TableName("tf_events").
		Measure("event_count", "count").
		Measure("duration", "avg", "sum", "max", "min").
		Dimension("event_type", "event_type", "text").
		Dimension("user_id", "user_id", "text").
		Dimension("date", "date_trunc('day', occurred_at)::text", "text").
		Description("Events fact table for user behavior analysis").
		Register()

	// Register aggregate queries that use the fact tables
	fraiseql.NewAggregateQueryConfig("salesByCategory").
		FactTableName("sales").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Sales aggregated by category with all metrics").
		Register()

	fraiseql.NewAggregateQueryConfig("salesByRegion").
		FactTableName("sales").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Sales aggregated by region").
		Register()

	fraiseql.NewAggregateQueryConfig("salesByMonthAndCategory").
		FactTableName("sales").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Sales aggregated by month and category for trend analysis").
		Register()

	fraiseql.NewAggregateQueryConfig("eventsByType").
		FactTableName("events").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Events aggregated by event type").
		Register()

	fraiseql.NewAggregateQueryConfig("userActivity").
		FactTableName("events").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("User activity analysis from events fact table").
		Register()

	fraiseql.NewAggregateQueryConfig("dailyEventMetrics").
		FactTableName("events").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Daily event metrics and statistics").
		Register()
}

func main() {
	// Register the types that back the fact tables
	if err := fraiseql.RegisterTypes(Sale{}, Event{}); err != nil {
		log.Fatal(err)
	}

	// Export schema to JSON
	if err := fraiseql.ExportSchema("schema.json"); err != nil {
		log.Fatal(err)
	}

	log.Println("\n✅ Analytics schema exported successfully!")
	log.Println("   Fact Tables: 2 (sales, events)")
	log.Println("   Aggregate Queries: 6")
	log.Println("")
	log.Println("   Next steps:")
	log.Println("   1. Compile schema: fraiseql-cli compile schema.json")
	log.Println("   2. Start server: fraiseql-server --schema schema.compiled.json")
	log.Println("")
	log.Println("   Analytics queries available:")
	log.Println("   - salesByCategory: Sales metrics by product category")
	log.Println("   - salesByRegion: Sales metrics by geographic region")
	log.Println("   - salesByMonthAndCategory: Time-series sales analysis")
	log.Println("   - eventsByType: Event distribution by type")
	log.Println("   - userActivity: User engagement metrics")
	log.Println("   - dailyEventMetrics: Daily event statistics")
}
