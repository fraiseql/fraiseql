//! Unit tests for the generate command.

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::super::csharp::CSharpGenerator;
    use super::super::go_lang::GoGenerator;
    use super::super::java::JavaGenerator;
    use super::super::kotlin::KotlinGenerator;
    use super::super::php::PhpGenerator;
    use super::super::python::PythonGenerator;
    use super::super::rust_lang::RustGenerator;
    use super::super::scala::ScalaGenerator;
    use super::super::swift::SwiftGenerator;
    use super::super::typescript::TypeScriptGenerator;
    use super::super::utils::{
        derive_class_name, infer_sql_source, map_graphql_to_lang, to_camel_case, to_pascal_case,
        wrap_nullable,
    };
    use super::super::SchemaGenerator;
    use super::super::super::init::Language;
    use crate::schema::intermediate::{
        IntermediateArgument, IntermediateEnum, IntermediateEnumValue, IntermediateField,
        IntermediateQuery, IntermediateSchema, IntermediateType,
    };

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("created_at"), "createdAt");
        assert_eq!(to_camel_case("post_id"), "postId");
        assert_eq!(to_camel_case("id"), "id");
        assert_eq!(to_camel_case("name"), "name");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("created_at"), "CreatedAt");
        assert_eq!(to_pascal_case("post_id"), "PostId");
        assert_eq!(to_pascal_case("id"), "Id");
        assert_eq!(to_pascal_case("name"), "Name");
    }

    #[test]
    fn test_map_graphql_to_lang_python() {
        assert_eq!(map_graphql_to_lang(Language::Python, "Int"), "int");
        assert_eq!(map_graphql_to_lang(Language::Python, "String"), "str");
        assert_eq!(map_graphql_to_lang(Language::Python, "Boolean"), "bool");
        assert_eq!(map_graphql_to_lang(Language::Python, "Float"), "float");
        assert_eq!(map_graphql_to_lang(Language::Python, "ID"), "ID");
    }

    #[test]
    fn test_map_graphql_to_lang_rust() {
        assert_eq!(map_graphql_to_lang(Language::Rust, "Int"), "i32");
        assert_eq!(map_graphql_to_lang(Language::Rust, "String"), "String");
        assert_eq!(map_graphql_to_lang(Language::Rust, "Boolean"), "bool");
        assert_eq!(map_graphql_to_lang(Language::Rust, "Float"), "f64");
    }

    #[test]
    fn test_map_graphql_to_lang_go() {
        assert_eq!(map_graphql_to_lang(Language::Go, "Int"), "int");
        assert_eq!(map_graphql_to_lang(Language::Go, "String"), "string");
        assert_eq!(map_graphql_to_lang(Language::Go, "Boolean"), "bool");
        assert_eq!(map_graphql_to_lang(Language::Go, "Float"), "float64");
    }

    #[test]
    fn test_wrap_nullable() {
        assert_eq!(wrap_nullable(Language::Python, "str"), "str | None");
        assert_eq!(wrap_nullable(Language::Rust, "String"), "Option<String>");
        assert_eq!(wrap_nullable(Language::Kotlin, "String"), "String?");
        assert_eq!(wrap_nullable(Language::Swift, "String"), "String?");
        assert_eq!(wrap_nullable(Language::CSharp, "string"), "string?");
        assert_eq!(wrap_nullable(Language::Go, "string"), "*string");
        assert_eq!(wrap_nullable(Language::Scala, "String"), "Option[String]");
    }

    #[test]
    fn test_derive_class_name() {
        let list_query = IntermediateQuery {
            name:         "authors".to_string(),
            return_type:  "Author".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            description:  None,
            sql_source:   None,
            auto_params:  None,
            deprecated:   None,
            jsonb_column: None,
            relay: false,
             inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
            relay_cursor_type: None,
        };
        assert_eq!(derive_class_name(&list_query), "Authors");

        let single_query = IntermediateQuery {
            name:         "author".to_string(),
            return_type:  "Author".to_string(),
            returns_list: false,
            nullable:     false,
            arguments:    vec![IntermediateArgument {
                name:       "id".to_string(),
                arg_type:   "ID".to_string(),
                nullable:   false,
                default:    None,
                deprecated: None,
            }],
            description:  None,
            sql_source:   None,
            auto_params:  None,
            deprecated:   None,
            jsonb_column: None,
            relay: false,
             inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
            relay_cursor_type: None,
        };
        assert_eq!(derive_class_name(&single_query), "AuthorById");
    }

    #[test]
    fn test_infer_sql_source() {
        assert_eq!(infer_sql_source("Author"), "v_author");
        assert_eq!(infer_sql_source("BlogPost"), "v_blog_post");
        assert_eq!(infer_sql_source("User"), "v_user");
    }

    fn sample_schema() -> IntermediateSchema {
        IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name:        "Author".to_string(),
                fields:      vec![
                    IntermediateField {
                        name:           "pk".to_string(),
                        field_type:     "Int".to_string(),
                        nullable:       false,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                    IntermediateField {
                        name:           "id".to_string(),
                        field_type:     "ID".to_string(),
                        nullable:       false,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                    IntermediateField {
                        name:           "name".to_string(),
                        field_type:     "String".to_string(),
                        nullable:       false,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                    IntermediateField {
                        name:           "bio".to_string(),
                        field_type:     "String".to_string(),
                        nullable:       true,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                ],
                description: None,
                implements:  Vec::new(),
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            queries: vec![
                IntermediateQuery {
                    name:         "authors".to_string(),
                    return_type:  "Author".to_string(),
                    returns_list: true,
                    nullable:     false,
                    arguments:    vec![],
                    description:  None,
                    sql_source:   Some("v_author".to_string()),
                    auto_params:  None,
                    deprecated:   None,
                    jsonb_column: None,
                    relay: false,
                     inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                    relay_cursor_type: None,
                },
                IntermediateQuery {
                    name:         "author".to_string(),
                    return_type:  "Author".to_string(),
                    returns_list: false,
                    nullable:     false,
                    arguments:    vec![IntermediateArgument {
                        name:       "id".to_string(),
                        arg_type:   "ID".to_string(),
                        nullable:   false,
                        default:    None,
                        deprecated: None,
                    }],
                    description:  None,
                    sql_source:   Some("v_author".to_string()),
                    auto_params:  None,
                    deprecated:   None,
                    jsonb_column: None,
                    relay: false,
                     inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                    relay_cursor_type: None,
                },
            ],
            enums: vec![IntermediateEnum {
                name:        "Status".to_string(),
                values:      vec![
                    IntermediateEnumValue {
                        name:        "ACTIVE".to_string(),
                        description: None,
                        deprecated:  None,
                    },
                    IntermediateEnumValue {
                        name:        "INACTIVE".to_string(),
                        description: None,
                        deprecated:  None,
                    },
                ],
                description: None,
            }],
            ..IntermediateSchema::default()
        }
    }

    #[test]
    fn test_python_generator() {
        let schema = sample_schema();
        let code = PythonGenerator.generate(&schema);
        assert!(code.contains("import fraiseql"));
        assert!(code.contains("@fraiseql.type(sql_source=\"v_author\")"));
        assert!(code.contains("class Author:"));
        assert!(code.contains("    pk: int"));
        assert!(code.contains("    id: ID"));
        assert!(code.contains("    name: str"));
        assert!(code.contains("    bio: str | None"));
        assert!(code.contains(
            "@fraiseql.query(return_type=Author, return_array=True, sql_source=\"v_author\")"
        ));
        assert!(code.contains("def authors() -> list[Author]:"));
        assert!(code.contains("def author(*, id: ID) -> Author:"));
    }

    #[test]
    fn test_typescript_generator() {
        let schema = sample_schema();
        let code = TypeScriptGenerator.generate(&schema);
        assert!(code.contains("import { type_, query } from \"fraiseql\""));
        assert!(code.contains("type_(\"Author\""));
        assert!(code.contains("pk: { type: \"Int\", nullable: false }"));
        assert!(code.contains("bio: { type: \"String\", nullable: true }"));
        assert!(code.contains("query(\"authors\""));
        assert!(code.contains("returnArray: true"));
        assert!(code.contains("{ name: \"id\", type: \"ID\", required: true }"));
    }

    #[test]
    fn test_rust_generator() {
        let schema = sample_schema();
        let code = RustGenerator.generate(&schema);
        assert!(code.contains("use fraiseql::{type_, query}"));
        assert!(code.contains("#[type_(sql_source = \"v_author\")]"));
        assert!(code.contains("pub struct Author {"));
        assert!(code.contains("    pub pk: i32,"));
        assert!(code.contains("    pub id: ID,"));
        assert!(code.contains("    pub name: String,"));
        assert!(code.contains("    pub bio: Option<String>,"));
        assert!(code.contains("#[query(return_type = \"Author\", return_array = true"));
        assert!(code.contains("pub fn authors() -> Vec<Author>"));
        assert!(code.contains("pub fn author(id: ID) -> Author"));
    }

    #[test]
    fn test_kotlin_generator() {
        let schema = sample_schema();
        let code = KotlinGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource = \"v_author\")"));
        assert!(code.contains("data class Author("));
        assert!(code.contains("    val pk: Int,"));
        assert!(code.contains("    val id: ID,"));
        assert!(code.contains("    val name: String,"));
        assert!(code.contains("    val bio: String?,"));
        assert!(code.contains("@Query(returnType = Author::class"));
        assert!(code.contains("fun authors(): List<Author> = TODO()"));
        assert!(code.contains("fun author(id: ID): Author = TODO()"));
    }

    #[test]
    fn test_swift_generator() {
        let schema = sample_schema();
        let code = SwiftGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource: \"v_author\")"));
        assert!(code.contains("struct Author {"));
        assert!(code.contains("    let pk: Int"));
        assert!(code.contains("    let id: ID"));
        assert!(code.contains("    let name: String"));
        assert!(code.contains("    let bio: String?"));
        assert!(code.contains("@Query(returnType: Author.self"));
        assert!(code.contains("func authors() -> [Author]"));
        assert!(code.contains("func author(id: ID) -> Author"));
    }

    #[test]
    fn test_scala_generator() {
        let schema = sample_schema();
        let code = ScalaGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource = \"v_author\")"));
        assert!(code.contains("case class Author("));
        assert!(code.contains("  pk: Int,"));
        assert!(code.contains("  id: ID,"));
        assert!(code.contains("  name: String,"));
        assert!(code.contains("  bio: Option[String]"));
        assert!(code.contains("@Query(returnType = classOf[Author]"));
        assert!(code.contains("def authors(): List[Author] = ???"));
        assert!(code.contains("def author(id: ID): Author = ???"));
    }

    #[test]
    fn test_java_generator() {
        let schema = sample_schema();
        let code = JavaGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource = \"v_author\")"));
        assert!(code.contains("public record Author("));
        assert!(code.contains("    int pk,"));
        assert!(code.contains("    ID id,"));
        assert!(code.contains("    String name,"));
        assert!(code.contains("    @Nullable String bio"));
        assert!(code.contains("@Query(returnType = Author.class, returnArray = true"));
        assert!(code.contains("public interface Authors {}"));
        assert!(code.contains("@Arg(name = \"id\", type = \"ID\", required = true)"));
        assert!(code.contains("public interface AuthorById {}"));
    }

    #[test]
    fn test_go_generator() {
        let schema = sample_schema();
        let code = GoGenerator.generate(&schema);
        assert!(code.contains("package schema"));
        assert!(code.contains("import \"fraiseql\""));
        assert!(code.contains("// @Type(sqlSource = \"v_author\")"));
        assert!(code.contains("type Author struct {"));
        assert!(code.contains("\tPk int `fraiseql:\"pk\"`"));
        assert!(code.contains("\tId ID `fraiseql:\"id\"`"));
        assert!(code.contains("\tName string `fraiseql:\"name\"`"));
        assert!(code.contains("\tBio *string `fraiseql:\"bio\"`"));
        assert!(code.contains("func init() {"));
        assert!(code.contains("RegisterQuery(\"authors\""));
        assert!(code.contains("ReturnArray: true"));
        assert!(code.contains("{Name: \"id\", Type: \"ID\", Required: true}"));
    }

    #[test]
    fn test_csharp_generator() {
        let schema = sample_schema();
        let code = CSharpGenerator.generate(&schema);
        assert!(code.contains("[Type(SqlSource = \"v_author\")]"));
        assert!(code.contains("public record Author("));
        assert!(code.contains("    int Pk,"));
        assert!(code.contains("    ID Id,"));
        assert!(code.contains("    string Name,"));
        assert!(code.contains("    string? Bio"));
        assert!(code.contains("[Query(ReturnType = typeof(Author), ReturnArray = true"));
        assert!(code.contains("public static partial class Authors;"));
        assert!(code.contains("Arg(Name = \"id\", Type = \"ID\", Required = true)"));
        assert!(code.contains("public static partial class AuthorById;"));
    }

    #[test]
    fn test_php_generator() {
        let schema = sample_schema();
        let code = PhpGenerator.generate(&schema);
        assert!(code.starts_with("<?php\n"), "PHP file must start with <?php");
        assert!(code.contains("declare(strict_types=1);"));
        assert!(code.contains("use FraiseQL\\Attributes\\GraphQLType;"));
        assert!(code.contains("use FraiseQL\\Attributes\\GraphQLField;"));
        assert!(code.contains("#[GraphQLType(name: 'Author', sqlSource: 'v_author')]"));
        assert!(code.contains("final class Author"));
        assert!(code.contains("    #[GraphQLField(type: 'Int')]\n    public int $pk;"));
        assert!(code.contains("    #[GraphQLField(type: 'ID')]\n    public string $id;"));
        assert!(code.contains("    #[GraphQLField(type: 'String', nullable: true)]\n    public ?string $bio;"));
        assert!(code.contains("returnArray: true"));
        assert!(code.contains("#[Query(returnType: 'Author::class', sqlSource: 'v_author',"));
        assert!(code.contains("new Arg(name: 'id', type: 'ID')"));
        assert!(code.contains("function authors(): void {}"));
        assert!(code.contains("function authorById(): void {}"));
    }

    #[test]
    fn test_empty_schema() {
        let schema = IntermediateSchema::default();
        let code = PythonGenerator.generate(&schema);
        assert!(code.contains("import fraiseql"));
        assert!(!code.contains("class "));
        assert!(!code.contains("def "));
    }
}
