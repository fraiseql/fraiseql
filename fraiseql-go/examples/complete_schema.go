package main

import (
	"log"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// User represents a user in the system
type User struct {
	ID        int    `fraiseql:"id,type=Int"`
	Name      string `fraiseql:"name,type=String"`
	Email     string `fraiseql:"email,type=String"`
	CreatedAt string `fraiseql:"created_at,type=String"`
	IsActive  bool   `fraiseql:"is_active,type=Boolean"`
}

// Post represents a blog post
type Post struct {
	ID        int    `fraiseql:"id,type=Int"`
	Title     string `fraiseql:"title,type=String"`
	Content   string `fraiseql:"content,type=String"`
	AuthorID  int    `fraiseql:"author_id,type=Int"`
	Published bool   `fraiseql:"published,type=Boolean"`
	CreatedAt string `fraiseql:"created_at,type=String"`
}

// Revenue represents revenue data for analytics
type Revenue struct {
	ID        int     `fraiseql:"id,type=Int"`
	Amount    float64 `fraiseql:"amount,type=Float"`
	Currency  string  `fraiseql:"currency,type=String"`
	Date      string  `fraiseql:"date,type=String"`
	Category  string  `fraiseql:"category,type=String"`
	Region    string  `fraiseql:"region,type=String"`
}

func init() {
	// Register queries for user management
	fraiseql.NewQuery("users").
		ReturnType(User{}).
		ReturnsArray(true).
		Config(map[string]interface{}{
			"sql_source": "v_user",
			"auto_params": map[string]bool{
				"limit":  true,
				"offset": true,
				"order_by": true,
			},
		}).
		Arg("limit", "Int", 10).
		Arg("offset", "Int", 0).
		Arg("is_active", "Boolean", nil, true).
		Description("Get all users with pagination and filtering").
		Register()

	fraiseql.NewQuery("user").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "v_user",
		}).
		Arg("id", "Int", nil).
		Description("Get a single user by ID").
		Register()

	// Register queries for post management
	fraiseql.NewQuery("posts").
		ReturnType(Post{}).
		ReturnsArray(true).
		Config(map[string]interface{}{
			"sql_source": "v_post",
			"auto_params": map[string]bool{
				"limit":  true,
				"offset": true,
				"order_by": true,
			},
		}).
		Arg("limit", "Int", 10).
		Arg("offset", "Int", 0).
		Arg("author_id", "Int", nil, true).
		Arg("published", "Boolean", nil, true).
		Description("Get posts with pagination and filtering").
		Register()

	fraiseql.NewQuery("post").
		ReturnType(Post{}).
		Config(map[string]interface{}{
			"sql_source": "v_post",
		}).
		Arg("id", "Int", nil).
		Description("Get a single post by ID").
		Register()

	// Register mutations for user management
	fraiseql.NewMutation("createUser").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "fn_create_user",
			"operation": "CREATE",
		}).
		Arg("name", "String", nil).
		Arg("email", "String", nil).
		Description("Create a new user").
		Register()

	fraiseql.NewMutation("updateUser").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "fn_update_user",
			"operation": "UPDATE",
		}).
		Arg("id", "Int", nil).
		Arg("name", "String", nil, true).
		Arg("email", "String", nil, true).
		Arg("is_active", "Boolean", nil, true).
		Description("Update an existing user").
		Register()

	fraiseql.NewMutation("deleteUser").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "fn_delete_user",
			"operation": "DELETE",
		}).
		Arg("id", "Int", nil).
		Description("Delete a user").
		Register()

	// Register mutations for post management
	fraiseql.NewMutation("createPost").
		ReturnType(Post{}).
		Config(map[string]interface{}{
			"sql_source": "fn_create_post",
			"operation": "CREATE",
		}).
		Arg("title", "String", nil).
		Arg("content", "String", nil).
		Arg("author_id", "Int", nil).
		Description("Create a new blog post").
		Register()

	fraiseql.NewMutation("publishPost").
		ReturnType(Post{}).
		Config(map[string]interface{}{
			"sql_source": "fn_publish_post",
			"operation": "UPDATE",
		}).
		Arg("id", "Int", nil).
		Description("Publish a blog post").
		Register()

	fraiseql.NewMutation("deletePost").
		ReturnType(Post{}).
		Config(map[string]interface{}{
			"sql_source": "fn_delete_post",
			"operation": "DELETE",
		}).
		Arg("id", "Int", nil).
		Description("Delete a blog post").
		Register()

	// Register fact tables for analytics
	fraiseql.NewFactTable("revenue").
		TableName("tf_revenue").
		Measure("amount", "sum", "avg", "max", "min").
		Measure("count", "count").
		Dimension("category", "data->>'category'", "text").
		Dimension("region", "data->>'region'", "text").
		Dimension("date", "date_trunc('day', date)::text", "text").
		Description("Revenue fact table for financial analytics").
		Register()

	// Register aggregate queries
	fraiseql.NewAggregateQueryConfig("revenueByCategory").
		FactTableName("revenue").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Revenue aggregated by product category").
		Register()

	fraiseql.NewAggregateQueryConfig("revenueByRegion").
		FactTableName("revenue").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Revenue aggregated by geographic region").
		Register()

	fraiseql.NewAggregateQueryConfig("revenueByDate").
		FactTableName("revenue").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Daily revenue trend analysis").
		Register()

	fraiseql.NewAggregateQueryConfig("revenueByCategoryAndRegion").
		FactTableName("revenue").
		AutoGroupBy(true).
		AutoAggregates(true).
		Description("Revenue by both category and region for detailed analysis").
		Register()
}

func main() {
	// Register the types that back the schema
	if err := fraiseql.RegisterTypes(User{}, Post{}, Revenue{}); err != nil {
		log.Fatal(err)
	}

	// Export schema to JSON
	if err := fraiseql.ExportSchema("schema.json"); err != nil {
		log.Fatal(err)
	}

	log.Println("\n✅ Complete schema exported successfully!")
	log.Println("")
	log.Println("   Schema includes:")
	log.Println("   • 3 types: User, Post, Revenue")
	log.Println("   • 4 queries: users, user, posts, post")
	log.Println("   • 7 mutations: CRUD operations for users and posts")
	log.Println("   • 1 fact table: revenue")
	log.Println("   • 4 aggregate queries: revenue analytics")
	log.Println("")
	log.Println("   Next steps:")
	log.Println("   1. Compile: fraiseql-cli compile schema.json -o schema.compiled.json")
	log.Println("   2. Start server: fraiseql-server --schema schema.compiled.json")
	log.Println("")
}
