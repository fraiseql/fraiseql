use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result};
use tracing::info;

use super::{InitConfig, Language};

/// Create the language-specific authoring skeleton for the project.
///
/// # Errors
///
/// Propagates any file-system errors from the language-specific skeleton creator.
pub(super) fn create_authoring_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    match config.language {
        Language::Python => create_python_skeleton(project_dir, config),
        Language::TypeScript => create_typescript_skeleton(project_dir, config),
        Language::Rust => create_rust_skeleton(project_dir, config),
        Language::Java => create_java_skeleton(project_dir, config),
        Language::Kotlin => create_kotlin_skeleton(project_dir, config),
        Language::Go => create_go_skeleton(project_dir, config),
        Language::CSharp => create_csharp_skeleton(project_dir, config),
        Language::Swift => create_swift_skeleton(project_dir, config),
        Language::Scala => create_scala_skeleton(project_dir, config),
        Language::Php => create_php_skeleton(project_dir, config),
    }
}

/// Create the Python authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_python_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"""FraiseQL blog schema definition for {name}."""

import fraiseql


@fraiseql.type(sql_source="v_author")
class Author:
    """Blog author with trinity pattern."""

    pk: int
    id: ID
    identifier: str
    name: str
    email: str
    bio: str | None
    created_at: DateTime
    updated_at: DateTime


@fraiseql.type(sql_source="v_post")
class Post:
    """Blog post with trinity pattern."""

    pk: int
    id: ID
    identifier: str
    title: str
    body: str
    published: bool
    author_id: ID
    created_at: DateTime
    updated_at: DateTime


@fraiseql.type(sql_source="v_comment")
class Comment:
    """Comment on a blog post."""

    pk: int
    id: ID
    body: str
    author_name: str
    post_id: ID
    created_at: DateTime


@fraiseql.type(sql_source="v_tag")
class Tag:
    """Categorization tag for posts."""

    pk: int
    id: ID
    identifier: str
    name: str


@fraiseql.query(return_type=Post, return_array=True, sql_source="v_post")
def posts() -> list[Post]:
    """List all published posts."""
    ...


@fraiseql.query(return_type=Post, sql_source="v_post")
def post(*, id: ID) -> Post:
    """Get post by ID."""
    ...


@fraiseql.query(return_type=Author, return_array=True, sql_source="v_author")
def authors() -> list[Author]:
    """List all authors."""
    ...


@fraiseql.query(return_type=Author, sql_source="v_author")
def author(*, id: ID) -> Author:
    """Get author by ID."""
    ...


@fraiseql.query(return_type=Tag, return_array=True, sql_source="v_tag")
def tags() -> list[Tag]:
    """List all tags."""
    ...
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.py"), content).context("Failed to create schema.py")?;
    info!("Created schema/schema.py");
    Ok(())
}

/// Create the TypeScript authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_typescript_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"/**
 * FraiseQL blog schema definition for {name}.
 */

import {{ type_, query }} from "fraiseql";

export const Author = type_("Author", {{
  sqlSource: "v_author",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    identifier: {{ type: "String", nullable: false }},
    name: {{ type: "String", nullable: false }},
    email: {{ type: "String", nullable: false }},
    bio: {{ type: "String", nullable: true }},
    created_at: {{ type: "DateTime", nullable: false }},
    updated_at: {{ type: "DateTime", nullable: false }},
  }},
}});

export const Post = type_("Post", {{
  sqlSource: "v_post",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    identifier: {{ type: "String", nullable: false }},
    title: {{ type: "String", nullable: false }},
    body: {{ type: "String", nullable: false }},
    published: {{ type: "Boolean", nullable: false }},
    author_id: {{ type: "ID", nullable: false }},
    created_at: {{ type: "DateTime", nullable: false }},
    updated_at: {{ type: "DateTime", nullable: false }},
  }},
}});

export const Comment = type_("Comment", {{
  sqlSource: "v_comment",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    body: {{ type: "String", nullable: false }},
    author_name: {{ type: "String", nullable: false }},
    post_id: {{ type: "ID", nullable: false }},
    created_at: {{ type: "DateTime", nullable: false }},
  }},
}});

export const Tag = type_("Tag", {{
  sqlSource: "v_tag",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    identifier: {{ type: "String", nullable: false }},
    name: {{ type: "String", nullable: false }},
  }},
}});

export const posts = query("posts", {{
  returnType: "Post",
  returnArray: true,
  sqlSource: "v_post",
}});

export const post = query("post", {{
  returnType: "Post",
  returnArray: false,
  sqlSource: "v_post",
  args: [{{ name: "id", type: "ID", required: true }}],
}});

export const authors = query("authors", {{
  returnType: "Author",
  returnArray: true,
  sqlSource: "v_author",
}});

export const author = query("author", {{
  returnType: "Author",
  returnArray: false,
  sqlSource: "v_author",
  args: [{{ name: "id", type: "ID", required: true }}],
}});

export const tagsQuery = query("tags", {{
  returnType: "Tag",
  returnArray: true,
  sqlSource: "v_tag",
}});
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.ts"), content).context("Failed to create schema.ts")?;
    info!("Created schema/schema.ts");
    Ok(())
}

/// Create the Rust authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_rust_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"//! FraiseQL blog schema definition for {name}.

use fraiseql::{{type_, query}};

/// Blog author with trinity pattern.
#[type_(sql_source = "v_author")]
pub struct Author {{
    pub pk: i32,
    pub id: ID,
    pub identifier: String,
    pub name: String,
    pub email: String,
    pub bio: Option<String>,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}}

/// Blog post with trinity pattern.
#[type_(sql_source = "v_post")]
pub struct Post {{
    pub pk: i32,
    pub id: ID,
    pub identifier: String,
    pub title: String,
    pub body: String,
    pub published: bool,
    pub author_id: ID,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}}

/// Comment on a blog post.
#[type_(sql_source = "v_comment")]
pub struct Comment {{
    pub pk: i32,
    pub id: ID,
    pub body: String,
    pub author_name: String,
    pub post_id: ID,
    pub created_at: DateTime,
}}

/// Categorization tag for posts.
#[type_(sql_source = "v_tag")]
pub struct Tag {{
    pub pk: i32,
    pub id: ID,
    pub identifier: String,
    pub name: String,
}}

#[query(return_type = "Post", return_array = true, sql_source = "v_post")]
pub fn posts() -> Vec<Post> {{
    todo!("implement resolver")
}}

#[query(return_type = "Post", sql_source = "v_post")]
pub fn post(id: ID) -> Post {{
    todo!("implement resolver")
}}

#[query(return_type = "Author", return_array = true, sql_source = "v_author")]
pub fn authors() -> Vec<Author> {{
    todo!("implement resolver")
}}

#[query(return_type = "Author", sql_source = "v_author")]
pub fn author(id: ID) -> Author {{
    todo!("implement resolver")
}}

#[query(return_type = "Tag", return_array = true, sql_source = "v_tag")]
pub fn tags() -> Vec<Tag> {{
    todo!("implement resolver")
}}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.rs"), content).context("Failed to create schema.rs")?;
    info!("Created schema/schema.rs");
    Ok(())
}

/// Create the Java authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_java_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema;

import fraiseql.FraiseQL;
import fraiseql.annotations.*;

/// Blog author with trinity pattern.
@Type(sqlSource = "v_author")
public record Author(
    int pk,
    ID id,
    String identifier,
    String name,
    String email,
    @Nullable String bio,
    DateTime createdAt,
    DateTime updatedAt
) {{}}

/// Blog post with trinity pattern.
@Type(sqlSource = "v_post")
public record Post(
    int pk,
    ID id,
    String identifier,
    String title,
    String body,
    boolean published,
    ID authorId,
    DateTime createdAt,
    DateTime updatedAt
) {{}}

/// Comment on a blog post.
@Type(sqlSource = "v_comment")
public record Comment(
    int pk,
    ID id,
    String body,
    String authorName,
    ID postId,
    DateTime createdAt
) {{}}

/// Categorization tag for posts.
@Type(sqlSource = "v_tag")
public record Tag(
    int pk,
    ID id,
    String identifier,
    String name
) {{}}

@Query(returnType = Post.class, returnArray = true, sqlSource = "v_post")
public interface Posts {{}}

@Query(returnType = Post.class, sqlSource = "v_post", args = @Arg(name = "id", type = "ID", required = true))
public interface PostById {{}}

@Query(returnType = Author.class, returnArray = true, sqlSource = "v_author")
public interface Authors {{}}

@Query(returnType = Author.class, sqlSource = "v_author", args = @Arg(name = "id", type = "ID", required = true))
public interface AuthorById {{}}

@Query(returnType = Tag.class, returnArray = true, sqlSource = "v_tag")
public interface Tags {{}}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.java"), content).context("Failed to create schema.java")?;
    info!("Created schema/schema.java");
    Ok(())
}

/// Create the Kotlin authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_kotlin_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema

import fraiseql.*

/// Blog author with trinity pattern.
@Type(sqlSource = "v_author")
data class Author(
    val pk: Int,
    val id: ID,
    val identifier: String,
    val name: String,
    val email: String,
    val bio: String?,
    val createdAt: DateTime,
    val updatedAt: DateTime,
)

/// Blog post with trinity pattern.
@Type(sqlSource = "v_post")
data class Post(
    val pk: Int,
    val id: ID,
    val identifier: String,
    val title: String,
    val body: String,
    val published: Boolean,
    val authorId: ID,
    val createdAt: DateTime,
    val updatedAt: DateTime,
)

/// Comment on a blog post.
@Type(sqlSource = "v_comment")
data class Comment(
    val pk: Int,
    val id: ID,
    val body: String,
    val authorName: String,
    val postId: ID,
    val createdAt: DateTime,
)

/// Categorization tag for posts.
@Type(sqlSource = "v_tag")
data class Tag(
    val pk: Int,
    val id: ID,
    val identifier: String,
    val name: String,
)

@Query(returnType = Post::class, returnArray = true, sqlSource = "v_post")
fun posts(): List<Post> = TODO("Schema definition only")

@Query(returnType = Post::class, sqlSource = "v_post")
fun post(id: ID): Post = TODO("Schema definition only")

@Query(returnType = Author::class, returnArray = true, sqlSource = "v_author")
fun authors(): List<Author> = TODO("Schema definition only")

@Query(returnType = Author::class, sqlSource = "v_author")
fun author(id: ID): Author = TODO("Schema definition only")

@Query(returnType = Tag::class, returnArray = true, sqlSource = "v_tag")
fun tags(): List<Tag> = TODO("Schema definition only")
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.kt"), content).context("Failed to create schema.kt")?;
    info!("Created schema/schema.kt");
    Ok(())
}

/// Create the Go authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_go_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema

import "fraiseql"

// Author - Blog author with trinity pattern.
// @Type(sqlSource = "v_author")
type Author struct {{
	PK         int      `fraiseql:"pk"`
	ID         ID       `fraiseql:"id"`
	Identifier string   `fraiseql:"identifier"`
	Name       string   `fraiseql:"name"`
	Email      string   `fraiseql:"email"`
	Bio        *string  `fraiseql:"bio"`
	CreatedAt  DateTime `fraiseql:"created_at"`
	UpdatedAt  DateTime `fraiseql:"updated_at"`
}}

// Post - Blog post with trinity pattern.
// @Type(sqlSource = "v_post")
type Post struct {{
	PK         int      `fraiseql:"pk"`
	ID         ID       `fraiseql:"id"`
	Identifier string   `fraiseql:"identifier"`
	Title      string   `fraiseql:"title"`
	Body       string   `fraiseql:"body"`
	Published  bool     `fraiseql:"published"`
	AuthorID   ID       `fraiseql:"author_id"`
	CreatedAt  DateTime `fraiseql:"created_at"`
	UpdatedAt  DateTime `fraiseql:"updated_at"`
}}

// Comment - Comment on a blog post.
// @Type(sqlSource = "v_comment")
type Comment struct {{
	PK         int      `fraiseql:"pk"`
	ID         ID       `fraiseql:"id"`
	Body       string   `fraiseql:"body"`
	AuthorName string   `fraiseql:"author_name"`
	PostID     ID       `fraiseql:"post_id"`
	CreatedAt  DateTime `fraiseql:"created_at"`
}}

// Tag - Categorization tag for posts.
// @Type(sqlSource = "v_tag")
type Tag struct {{
	PK         int    `fraiseql:"pk"`
	ID         ID     `fraiseql:"id"`
	Identifier string `fraiseql:"identifier"`
	Name       string `fraiseql:"name"`
}}

// Queries are registered via fraiseql.RegisterQuery().
func init() {{
	fraiseql.RegisterQuery("posts", fraiseql.QueryDef{{ReturnType: "Post", ReturnArray: true, SQLSource: "v_post"}})
	fraiseql.RegisterQuery("post", fraiseql.QueryDef{{ReturnType: "Post", SQLSource: "v_post", Args: []fraiseql.Arg{{{{Name: "id", Type: "ID", Required: true}}}}}})
	fraiseql.RegisterQuery("authors", fraiseql.QueryDef{{ReturnType: "Author", ReturnArray: true, SQLSource: "v_author"}})
	fraiseql.RegisterQuery("author", fraiseql.QueryDef{{ReturnType: "Author", SQLSource: "v_author", Args: []fraiseql.Arg{{{{Name: "id", Type: "ID", Required: true}}}}}})
	fraiseql.RegisterQuery("tags", fraiseql.QueryDef{{ReturnType: "Tag", ReturnArray: true, SQLSource: "v_tag"}})
}}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.go"), content).context("Failed to create schema.go")?;
    info!("Created schema/schema.go");
    Ok(())
}

/// Create the C# authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_csharp_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

using FraiseQL;

namespace Schema;

/// Blog author with trinity pattern.
[Type(SqlSource = "v_author")]
public record Author(
    int Pk,
    ID Id,
    string Identifier,
    string Name,
    string Email,
    string? Bio,
    DateTime CreatedAt,
    DateTime UpdatedAt
);

/// Blog post with trinity pattern.
[Type(SqlSource = "v_post")]
public record Post(
    int Pk,
    ID Id,
    string Identifier,
    string Title,
    string Body,
    bool Published,
    ID AuthorId,
    DateTime CreatedAt,
    DateTime UpdatedAt
);

/// Comment on a blog post.
[Type(SqlSource = "v_comment")]
public record Comment(
    int Pk,
    ID Id,
    string Body,
    string AuthorName,
    ID PostId,
    DateTime CreatedAt
);

/// Categorization tag for posts.
[Type(SqlSource = "v_tag")]
public record Tag(
    int Pk,
    ID Id,
    string Identifier,
    string Name
);

[Query(ReturnType = typeof(Post), ReturnArray = true, SqlSource = "v_post")]
public static partial class Posts;

[Query(ReturnType = typeof(Post), SqlSource = "v_post", Arg(Name = "id", Type = "ID", Required = true))]
public static partial class PostById;

[Query(ReturnType = typeof(Author), ReturnArray = true, SqlSource = "v_author")]
public static partial class Authors;

[Query(ReturnType = typeof(Author), SqlSource = "v_author", Arg(Name = "id", Type = "ID", Required = true))]
public static partial class AuthorById;

[Query(ReturnType = typeof(Tag), ReturnArray = true, SqlSource = "v_tag")]
public static partial class Tags;
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.cs"), content).context("Failed to create schema.cs")?;
    info!("Created schema/schema.cs");
    Ok(())
}

/// Create the Swift authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_swift_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

import FraiseQL

/// Blog author with trinity pattern.
@Type(sqlSource: "v_author")
struct Author {{
    let pk: Int
    let id: ID
    let identifier: String
    let name: String
    let email: String
    let bio: String?
    let createdAt: DateTime
    let updatedAt: DateTime
}}

/// Blog post with trinity pattern.
@Type(sqlSource: "v_post")
struct Post {{
    let pk: Int
    let id: ID
    let identifier: String
    let title: String
    let body: String
    let published: Bool
    let authorId: ID
    let createdAt: DateTime
    let updatedAt: DateTime
}}

/// Comment on a blog post.
@Type(sqlSource: "v_comment")
struct Comment {{
    let pk: Int
    let id: ID
    let body: String
    let authorName: String
    let postId: ID
    let createdAt: DateTime
}}

/// Categorization tag for posts.
@Type(sqlSource: "v_tag")
struct Tag {{
    let pk: Int
    let id: ID
    let identifier: String
    let name: String
}}

@Query(returnType: Post.self, returnArray: true, sqlSource: "v_post")
func posts() -> [Post] {{ fatalError("Schema definition only") }}

@Query(returnType: Post.self, sqlSource: "v_post")
func post(id: ID) -> Post {{ fatalError("Schema definition only") }}

@Query(returnType: Author.self, returnArray: true, sqlSource: "v_author")
func authors() -> [Author] {{ fatalError("Schema definition only") }}

@Query(returnType: Author.self, sqlSource: "v_author")
func author(id: ID) -> Author {{ fatalError("Schema definition only") }}

@Query(returnType: Tag.self, returnArray: true, sqlSource: "v_tag")
func tags() -> [Tag] {{ fatalError("Schema definition only") }}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.swift"), content).context("Failed to create schema.swift")?;
    info!("Created schema/schema.swift");
    Ok(())
}

/// Create the Scala authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_scala_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema

import fraiseql._

/// Blog author with trinity pattern.
@Type(sqlSource = "v_author")
case class Author(
  pk: Int,
  id: ID,
  identifier: String,
  name: String,
  email: String,
  bio: Option[String],
  createdAt: DateTime,
  updatedAt: DateTime
)

/// Blog post with trinity pattern.
@Type(sqlSource = "v_post")
case class Post(
  pk: Int,
  id: ID,
  identifier: String,
  title: String,
  body: String,
  published: Boolean,
  authorId: ID,
  createdAt: DateTime,
  updatedAt: DateTime
)

/// Comment on a blog post.
@Type(sqlSource = "v_comment")
case class Comment(
  pk: Int,
  id: ID,
  body: String,
  authorName: String,
  postId: ID,
  createdAt: DateTime
)

/// Categorization tag for posts.
@Type(sqlSource = "v_tag")
case class Tag(
  pk: Int,
  id: ID,
  identifier: String,
  name: String
)

@Query(returnType = classOf[Post], returnArray = true, sqlSource = "v_post")
def posts(): List[Post] = ???

@Query(returnType = classOf[Post], sqlSource = "v_post")
def post(id: ID): Post = ???

@Query(returnType = classOf[Author], returnArray = true, sqlSource = "v_author")
def authors(): List[Author] = ???

@Query(returnType = classOf[Author], sqlSource = "v_author")
def author(id: ID): Author = ???

@Query(returnType = classOf[Tag], returnArray = true, sqlSource = "v_tag")
def tags(): List[Tag] = ???
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.scala"), content).context("Failed to create schema.scala")?;
    info!("Created schema/schema.scala");
    Ok(())
}

/// Create the PHP authoring skeleton under `project_dir/schema/`.
///
/// # Errors
///
/// Returns an error if the schema directory or skeleton files cannot be created.
pub(super) fn create_php_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r"<?php

declare(strict_types=1);

// FraiseQL blog schema definition for {name}.

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;

/** Blog author with trinity pattern. */
#[GraphQLType(name: 'Author', sqlSource: 'v_author')]
final class Author
{{
    #[GraphQLField(type: 'Int')]
    public int $pk;

    #[GraphQLField(type: 'ID')]
    public string $id;

    #[GraphQLField(type: 'String')]
    public string $identifier;

    #[GraphQLField(type: 'String')]
    public string $name;

    #[GraphQLField(type: 'String')]
    public string $email;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $bio;

    #[GraphQLField(type: 'DateTime')]
    public string $createdAt;

    #[GraphQLField(type: 'DateTime')]
    public string $updatedAt;
}}

/** Blog post with trinity pattern. */
#[GraphQLType(name: 'Post', sqlSource: 'v_post')]
final class Post
{{
    #[GraphQLField(type: 'Int')]
    public int $pk;

    #[GraphQLField(type: 'ID')]
    public string $id;

    #[GraphQLField(type: 'String')]
    public string $identifier;

    #[GraphQLField(type: 'String')]
    public string $title;

    #[GraphQLField(type: 'String')]
    public string $body;

    #[GraphQLField(type: 'Boolean')]
    public bool $published;

    #[GraphQLField(type: 'ID')]
    public string $authorId;

    #[GraphQLField(type: 'DateTime')]
    public string $createdAt;

    #[GraphQLField(type: 'DateTime')]
    public string $updatedAt;
}}

/** Comment on a blog post. */
#[GraphQLType(name: 'Comment', sqlSource: 'v_comment')]
final class Comment
{{
    #[GraphQLField(type: 'Int')]
    public int $pk;

    #[GraphQLField(type: 'ID')]
    public string $id;

    #[GraphQLField(type: 'String')]
    public string $body;

    #[GraphQLField(type: 'String')]
    public string $authorName;

    #[GraphQLField(type: 'ID')]
    public string $postId;

    #[GraphQLField(type: 'DateTime')]
    public string $createdAt;
}}

/** Categorization tag for posts. */
#[GraphQLType(name: 'Tag', sqlSource: 'v_tag')]
final class Tag
{{
    #[GraphQLField(type: 'Int')]
    public int $pk;

    #[GraphQLField(type: 'ID')]
    public string $id;

    #[GraphQLField(type: 'String')]
    public string $identifier;

    #[GraphQLField(type: 'String')]
    public string $name;
}}
",
        name = config.project_name,
    );

    fs::write(dir.join("schema.php"), content).context("Failed to create schema/schema.php")?;
    info!("Created schema/schema.php");
    Ok(())
}

/// Initialise a git repository in `project_dir`.
///
/// # Errors
///
/// Returns an error if `git init` cannot be spawned or exits with a non-zero status.
pub(super) fn init_git(project_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["init"])
        .current_dir(project_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => {
            info!("Initialized git repository");
            Ok(())
        },
        Ok(_) => {
            // git init failed but non-fatal
            eprintln!("Warning: git init failed. You can initialize git manually.");
            Ok(())
        },
        Err(_) => {
            eprintln!("Warning: git not found. Skipping repository initialization.");
            Ok(())
        },
    }
}
