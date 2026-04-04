#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{
    Language, SchemaExtractor, csharp::CSharpExtractor, derive_query_name, go::GoExtractor,
    java::JavaExtractor, kotlin::KotlinExtractor, map_type, parse_annotation_params,
    python::PythonExtractor, rust::RustExtractor, scala::ScalaExtractor, strip_class_ref,
    swift::SwiftExtractor, to_snake_case, typescript::TypeScriptExtractor,
};

#[test]
fn test_to_snake_case() {
    assert_eq!(to_snake_case("createdAt"), "created_at");
    assert_eq!(to_snake_case("AuthorName"), "author_name");
    assert_eq!(to_snake_case("postId"), "post_id");
    assert_eq!(to_snake_case("id"), "id");
    assert_eq!(to_snake_case("PK"), "p_k");
    assert_eq!(to_snake_case("pk"), "pk");
}

#[test]
fn test_strip_class_ref() {
    assert_eq!(strip_class_ref("Post.class"), "Post");
    assert_eq!(strip_class_ref("Post.self"), "Post");
    assert_eq!(strip_class_ref("Post::class"), "Post");
    assert_eq!(strip_class_ref("classOf[Post]"), "Post");
    assert_eq!(strip_class_ref("typeof(Post)"), "Post");
    assert_eq!(strip_class_ref("Post"), "Post");
}

#[test]
fn test_parse_annotation_params() {
    let params = parse_annotation_params(r#"sql_source = "v_author", description = "test""#);
    assert_eq!(params.get("sql_source").unwrap(), "v_author");
    assert_eq!(params.get("description").unwrap(), "test");
}

#[test]
fn test_parse_annotation_params_boolean() {
    let params = parse_annotation_params("return_array = true, nullable = false");
    assert_eq!(params.get("return_array").unwrap(), "true");
    assert_eq!(params.get("nullable").unwrap(), "false");
}

#[test]
fn test_parse_annotation_params_class_refs() {
    let params = parse_annotation_params("returnType = Post.class, returnArray = true");
    assert_eq!(params.get("returnType").unwrap(), "Post");
    assert_eq!(params.get("returnArray").unwrap(), "true");

    let params2 = parse_annotation_params("returnType = classOf[Post]");
    assert_eq!(params2.get("returnType").unwrap(), "Post");
}

#[test]
fn test_map_type_python() {
    assert_eq!(map_type(Language::Python, "int"), ("Int".to_string(), false));
    assert_eq!(map_type(Language::Python, "str | None"), ("String".to_string(), true));
    assert_eq!(map_type(Language::Python, "bool"), ("Boolean".to_string(), false));
}

#[test]
fn test_map_type_rust() {
    assert_eq!(map_type(Language::Rust, "i32"), ("Int".to_string(), false));
    assert_eq!(map_type(Language::Rust, "Option<String>"), ("String".to_string(), true));
    assert_eq!(map_type(Language::Rust, "bool"), ("Boolean".to_string(), false));
}

#[test]
fn test_map_type_kotlin() {
    assert_eq!(map_type(Language::Kotlin, "Int"), ("Int".to_string(), false));
    assert_eq!(map_type(Language::Kotlin, "String?"), ("String".to_string(), true));
}

#[test]
fn test_map_type_go() {
    assert_eq!(map_type(Language::Go, "int"), ("Int".to_string(), false));
    assert_eq!(map_type(Language::Go, "*string"), ("String".to_string(), true));
}

#[test]
fn test_map_type_scala() {
    assert_eq!(map_type(Language::Scala, "Option[String]"), ("String".to_string(), true));
    assert_eq!(map_type(Language::Scala, "Int"), ("Int".to_string(), false));
}

#[test]
fn test_derive_query_name() {
    assert_eq!(derive_query_name("Posts"), "posts");
    assert_eq!(derive_query_name("PostById"), "post");
    assert_eq!(derive_query_name("Authors"), "authors");
    assert_eq!(derive_query_name("AuthorById"), "author");
    assert_eq!(derive_query_name("Tags"), "tags");
}

#[test]
fn test_python_extractor() {
    let source = r#"
import fraiseql

@fraiseql.type(sql_source="v_author")
class Author:
    pk: int
    id: ID
    name: str
    bio: str | None

@fraiseql.query(return_type=Author, return_array=True, sql_source="v_author")
def authors() -> list[Author]:
    ...

@fraiseql.query(return_type=Author, sql_source="v_author")
def author(*, id: ID) -> Author:
    ...
"#;
    let result = PythonExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].name, "Author");
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].name, "id");
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].name, "name");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert_eq!(result.types[0].fields[3].name, "bio");
    assert_eq!(result.types[0].fields[3].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert_eq!(result.queries[0].name, "authors");
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].name, "author");
    assert!(!result.queries[1].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].name, "id");
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_typescript_extractor() {
    let source = r#"
import { type_, query } from "fraiseql";

export const Author = type_("Author", {
  sqlSource: "v_author",
  fields: {
    pk: { type: "Int", nullable: false },
    id: { type: "ID", nullable: false },
    name: { type: "String", nullable: false },
    bio: { type: "String", nullable: true },
  },
});

export const authors = query("authors", {
  returnType: "Author",
  returnArray: true,
  sqlSource: "v_author",
});

export const author = query("author", {
  returnType: "Author",
  returnArray: false,
  sqlSource: "v_author",
  args: [{ name: "id", type: "ID", required: true }],
});
"#;
    let result = TypeScriptExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].name, "Author");
    assert_eq!(result.types[0].fields.len(), 4);
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
}

#[test]
fn test_rust_extractor() {
    // This fixture represents the stub code that `fraiseql generate` emits for Rust users.
    // The stub bodies are intentional — FraiseQL Rust SDK functions are authoring
    // constructs (compile-time decorators); their bodies are never called.
    // The extractor must correctly parse this generated stub pattern.
    let source = r#"
use fraiseql::{type_, query};

#[type_(sql_source = "v_author")]
pub struct Author {
    pub pk: i32,
    pub id: ID,
    pub name: String,
    pub bio: Option<String>,
}

#[query(return_type = "Author", return_array = true, sql_source = "v_author")]
pub fn authors() -> Vec<Author> {
    panic!("fraiseql-generated stub body")
}

#[query(return_type = "Author", sql_source = "v_author")]
pub fn author(id: ID) -> Author {
    panic!("fraiseql-generated stub body")
}
"#;
    let result = RustExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].name, "Author");
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].name, "id");
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].name, "name");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_java_extractor() {
    let source = r#"
@Type(sqlSource = "v_author")
public record Author(
    int pk,
    ID id,
    String name,
    @Nullable String bio
) {}

@Query(returnType = Author.class, returnArray = true, sqlSource = "v_author")
public interface Authors {}

@Query(returnType = Author.class, sqlSource = "v_author", args = @Arg(name = "id", type = "ID", required = true))
public interface AuthorById {}
"#;
    let result = JavaExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].name, "Author");
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert_eq!(result.queries[0].name, "authors");
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].name, "author");
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_kotlin_extractor() {
    let source = r#"
@Type(sqlSource = "v_author")
data class Author(
    val pk: Int,
    val id: ID,
    val name: String,
    val bio: String?,
)

@Query(returnType = Author::class, returnArray = true, sqlSource = "v_author")
fun authors(): List<Author> = TODO()

@Query(returnType = Author::class, sqlSource = "v_author")
fun author(id: ID): Author = TODO()
"#;
    let result = KotlinExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_go_extractor() {
    let source = r#"
package schema

import "fraiseql"

// @Type(sqlSource = "v_author")
type Author struct {
	PK   int     `fraiseql:"pk"`
	ID   ID      `fraiseql:"id"`
	Name string  `fraiseql:"name"`
	Bio  *string `fraiseql:"bio"`
}

func init() {
	fraiseql.RegisterQuery("authors", fraiseql.QueryDef{ReturnType: "Author", ReturnArray: true, SQLSource: "v_author"})
	fraiseql.RegisterQuery("author", fraiseql.QueryDef{ReturnType: "Author", SQLSource: "v_author", Args: []fraiseql.Arg{{Name: "id", Type: "ID", Required: true}}})
}
"#;
    let result = GoExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
}

#[test]
fn test_csharp_extractor() {
    let source = r#"
[Type(SqlSource = "v_author")]
public record Author(
    int Pk,
    ID Id,
    string Name,
    string? Bio
);

[Query(ReturnType = typeof(Author), ReturnArray = true, SqlSource = "v_author")]
public static partial class Authors;

[Query(ReturnType = typeof(Author), SqlSource = "v_author", Arg(Name = "id", Type = "ID", Required = true))]
public static partial class AuthorById;
"#;
    let result = CSharpExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert_eq!(result.queries[0].name, "authors");
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].name, "author");
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].name, "id");
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_swift_extractor() {
    let source = r#"
@Type(sqlSource: "v_author")
struct Author {
    let pk: Int
    let id: ID
    let name: String
    let bio: String?
}

@Query(returnType: Author.self, returnArray: true, sqlSource: "v_author")
func authors() -> [Author] { fatalError() }

@Query(returnType: Author.self, sqlSource: "v_author")
func author(id: ID) -> Author { fatalError() }
"#;
    let result = SwiftExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_scala_extractor() {
    let source = r#"
@Type(sqlSource = "v_author")
case class Author(
  pk: Int,
  id: ID,
  name: String,
  bio: Option[String]
)

@Query(returnType = classOf[Author], returnArray = true, sqlSource = "v_author")
def authors(): List[Author] = ???

@Query(returnType = classOf[Author], sqlSource = "v_author")
def author(id: ID): Author = ???
"#;
    let result = ScalaExtractor.extract(source).unwrap();
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].fields.len(), 4);
    assert_eq!(result.types[0].fields[1].field_type, "ID");
    assert_eq!(result.types[0].fields[2].field_type, "String");
    assert!(result.types[0].fields[3].nullable);

    assert_eq!(result.queries.len(), 2);
    assert!(result.queries[0].returns_list);
    assert_eq!(result.queries[1].arguments.len(), 1);
    assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
}

#[test]
fn test_empty_source() {
    let result = PythonExtractor.extract("# no schema here").unwrap();
    assert!(result.types.is_empty());
    assert!(result.queries.is_empty());
}
