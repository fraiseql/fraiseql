namespace FraiseQL

open System.Text.Json.Serialization

/// Represents a single field on a GraphQL type.
[<CLIMutable>]
type FieldDefinition =
    {
        /// The snake_case field name as it appears in schema.json.
        name: string
        /// The GraphQL type string, e.g. "ID", "String", "[Author]".
        [<JsonPropertyName("type")>]
        type_: string
        /// Whether the field may be null in GraphQL responses.
        nullable: bool
        /// Optional human-readable description for introspection.
        description: string option
        /// Optional scope required to read this field.
        scope: string option
    }

/// Represents an argument on a GraphQL query or mutation.
[<CLIMutable>]
type ArgumentDefinition =
    {
        /// The argument name.
        name: string
        /// The GraphQL type string for this argument.
        [<JsonPropertyName("type")>]
        type_: string
        /// Whether this argument is optional.
        nullable: bool
    }

/// Represents a GraphQL object type compiled from a SQL view.
[<CLIMutable>]
type TypeDefinition =
    {
        /// The GraphQL type name (PascalCase).
        name: string
        /// The SQL view name this type reads from.
        sql_source: string
        /// Optional human-readable description for introspection.
        description: string option
        /// The fields exposed by this type.
        fields: FieldDefinition list
        /// True if this type is a GraphQL input type.
        is_input: bool
        /// True if this type participates in Relay cursor pagination.
        relay: bool
        /// True if this type models a mutation error response.
        is_error: bool
        /// True if this type is scoped to a tenant for multi-tenant schemas.
        tenant_scoped: bool
    }

/// Represents a GraphQL query (read operation).
[<CLIMutable>]
type QueryDefinition =
    {
        /// The GraphQL query field name (camelCase).
        name: string
        /// The GraphQL type this query returns.
        return_type: string
        /// True if the query returns a list of items.
        returns_list: bool
        /// True if the query result may be null.
        nullable: bool
        /// The SQL view or function backing this query.
        sql_source: string
        /// Arguments accepted by this query.
        arguments: ArgumentDefinition list
        /// Optional cache TTL in seconds; None means no caching.
        cache_ttl_seconds: int option
        /// Optional human-readable description for introspection.
        description: string option
        /// Optional REST endpoint path for this query.
        rest_path: string option
        /// Optional HTTP method for the REST endpoint.
        rest_method: string option
    }

/// Represents a GraphQL mutation (write operation).
[<CLIMutable>]
type MutationDefinition =
    {
        /// The GraphQL mutation field name (camelCase).
        name: string
        /// The GraphQL type this mutation returns.
        return_type: string
        /// The SQL function backing this mutation.
        sql_source: string
        /// The operation kind: "insert", "update", "delete", or "custom".
        operation: string
        /// Arguments accepted by this mutation.
        arguments: ArgumentDefinition list
        /// Optional human-readable description for introspection.
        description: string option
        /// Optional REST endpoint path for this mutation.
        rest_path: string option
        /// Optional HTTP method for the REST endpoint.
        rest_method: string option
    }

/// The root schema record serialized to schema.json.
[<CLIMutable>]
type IntermediateSchema =
    {
        /// Schema format version.
        version: string
        /// All GraphQL types defined in this schema.
        types: TypeDefinition list
        /// All GraphQL queries defined in this schema.
        queries: QueryDefinition list
        /// All GraphQL mutations defined in this schema.
        mutations: MutationDefinition list
    }

/// Three-state field for update mutation inputs.
/// Distinguishes "not provided" from "explicitly null" from "has value".
[<RequireQualifiedAccess>]
type UpdateField<'T> =
    | Unset
    | Null
    | Value of 'T

module UpdateField =
    let isUnset = function UpdateField.Unset -> true | _ -> false
    let isNull = function UpdateField.Null -> true | _ -> false
    let isValue = function UpdateField.Value _ -> true | _ -> false
    let getValue = function
        | UpdateField.Value v -> v
        | _ -> failwith "Cannot get value from Unset or Null UpdateField"
    let getValueOr defaultVal = function
        | UpdateField.Value v -> v
        | _ -> defaultVal

/// Discriminated union of all GraphQL scalar types.
type GraphQLScalar =
    | GqlInt
    | GqlFloat
    | GqlString
    | GqlBoolean
    | GqlId
    | GqlDateTime
    | GqlCustom of string
