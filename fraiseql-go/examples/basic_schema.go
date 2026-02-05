package main

import (
	"log"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// User type representing a user in the system
type User struct {
	ID        int    `fraiseql:"id,type=Int"`
	Name      string `fraiseql:"name,type=String"`
	Email     string `fraiseql:"email,type=String"`
	CreatedAt string `fraiseql:"createdAt,type=String"`
	IsActive  bool   `fraiseql:"isActive,type=Boolean"`
}

// Post type representing a blog post
type Post struct {
	ID        int    `fraiseql:"id,type=Int"`
	Title     string `fraiseql:"title,type=String"`
	Content   string `fraiseql:"content,type=String"`
	AuthorID  int    `fraiseql:"authorId,type=Int"`
	Published bool   `fraiseql:"published,type=Boolean"`
	CreatedAt string `fraiseql:"createdAt,type=String"`
}

// Initialize queries and mutations using init() function
func init() {
	// Query: Get all users with pagination
	fraiseql.NewQuery("users").
		ReturnType(User{}).
		ReturnsArray(true).
		Config(map[string]interface{}{
			"sql_source": "v_user",
			"auto_params": map[string]bool{
				"limit":     true,
				"offset":    true,
				"where":     true,
				"order_by":  true,
			},
		}).
		Arg("limit", "Int", 10).
		Arg("offset", "Int", 0).
		Arg("isActive", "Boolean", nil, true).
		Description("Get list of users with pagination").
		Register()

	// Query: Get a single user by ID
	fraiseql.NewQuery("user").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "v_user",
		}).
		Arg("id", "Int", nil).
		Description("Get a single user by ID").
		Register()

	// Query: Get posts with optional filtering
	fraiseql.NewQuery("posts").
		ReturnType(Post{}).
		ReturnsArray(true).
		Config(map[string]interface{}{
			"sql_source": "v_post",
			"auto_params": map[string]bool{
				"limit":     true,
				"offset":    true,
				"where":     true,
				"order_by":  true,
			},
		}).
		Arg("authorId", "Int", nil, true).
		Arg("published", "Boolean", true).
		Arg("limit", "Int", 10).
		Arg("offset", "Int", 0).
		Description("Get list of posts with filtering").
		Register()

	// Mutation: Create a new user
	fraiseql.NewMutation("createUser").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "fn_create_user",
			"operation":  "CREATE",
		}).
		Arg("name", "String", nil).
		Arg("email", "String", nil).
		Description("Create a new user").
		Register()

	// Mutation: Update an existing user
	fraiseql.NewMutation("updateUser").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "fn_update_user",
			"operation":  "UPDATE",
		}).
		Arg("id", "Int", nil).
		Arg("name", "String", nil, true).
		Arg("email", "String", nil, true).
		Arg("isActive", "Boolean", nil, true).
		Description("Update an existing user").
		Register()

	// Mutation: Delete a user
	fraiseql.NewMutation("deleteUser").
		ReturnType(User{}).
		Config(map[string]interface{}{
			"sql_source": "fn_delete_user",
			"operation":  "DELETE",
		}).
		Arg("id", "Int", nil).
		Description("Delete a user").
		Register()

	// Mutation: Create a new blog post
	fraiseql.NewMutation("createPost").
		ReturnType(Post{}).
		Config(map[string]interface{}{
			"sql_source": "fn_create_post",
			"operation":  "CREATE",
		}).
		Arg("title", "String", nil).
		Arg("content", "String", nil).
		Arg("authorId", "Int", nil).
		Description("Create a new blog post").
		Register()

	// Mutation: Publish a post
	fraiseql.NewMutation("publishPost").
		ReturnType(Post{}).
		Config(map[string]interface{}{
			"sql_source": "fn_publish_post",
			"operation":  "UPDATE",
		}).
		Arg("id", "Int", nil).
		Description("Publish a blog post").
		Register()
}

func main() {
	// Register the types
	if err := fraiseql.RegisterTypes(User{}, Post{}); err != nil {
		log.Fatal(err)
	}

	// Export schema to JSON
	if err := fraiseql.ExportSchema("schema.json"); err != nil {
		log.Fatal(err)
	}

	log.Println("\n✅ Schema exported successfully!")
	log.Println("   Next steps:")
	log.Println("   1. Compile schema: fraiseql-cli compile schema.json")
	log.Println("   2. Start server: fraiseql-server --schema schema.compiled.json")
}
