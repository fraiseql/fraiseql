namespace FraiseQL

open System.Text.Json.Serialization

/// pgvector configuration for a vector field, emitted as the `vector_config` object
/// the compiler reads.
///
/// The compiler refuses a `Vector`, `BitVector`, `HalfVector` or `SparseVector` field
/// that carries no configuration, so this is what makes those types authorable.
///
/// Which combinations of field type, metric and index exist is pgvector's business and
/// the compiler's: it holds the operator-class table — `ivfflat` has no class for a
/// sparse vector at all, and none for jaccard — and refuses a schema that asks for one
/// that does not, naming the alternative. This SDK carries no second copy of that table;
/// a copy is what drifts.
[<CLIMutable>]
type VectorConfig =
    {
        /// Vector width: float components for `Vector`, `HalfVector` and `SparseVector`,
        /// **bits** for `BitVector`. It sizes the column, and a query vector of a
        /// different width is refused rather than silently padded.
        dimensions: int
        /// One of `VectorIndex.hnsw` (the default), `ivfFlat` or `none`.
        index_type: string
        /// One of `VectorMetric.cosine` (the default), `l2`, `innerProduct`, `hamming`
        /// or `jaccard`.
        distance_metric: string
    }

/// The index a pgvector column is searched through.
module VectorIndex =
    /// Hierarchical Navigable Small World index — the default.
    [<Literal>]
    let hnsw = "hnsw"

    /// Inverted-file index: smaller and faster to build, slower to query.
    [<Literal>]
    let ivfFlat = "ivf_flat"

    /// No index — exact search.
    [<Literal>]
    let none = "none"

/// The distance metric a vector search orders by.
module VectorMetric =
    /// Cosine distance — the default, and what most text embeddings want.
    [<Literal>]
    let cosine = "cosine"

    /// Euclidean distance.
    [<Literal>]
    let l2 = "l2"

    /// Negative inner product.
    [<Literal>]
    let innerProduct = "inner_product"

    /// Differing bits — `BitVector` only.
    [<Literal>]
    let hamming = "hamming"

    /// Set overlap normalised by set size — `BitVector` only.
    [<Literal>]
    let jaccard = "jaccard"

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
        ///
        /// Serialised as `requires_scope` — the key the compiler reads. It was emitted as
        /// `scope`, which binds to nothing, so a field the author gated compiled with no
        /// scope at all and was served to callers holding none (#807).
        [<JsonPropertyName("requires_scope")>]
        scope: string option
        /// When true, this field is server-computed and excluded from CRUD input types.
        /// Computed fields remain visible in query results.
        ///
        /// Authoring-time only, and therefore not serialised: `CrudGenerator` reads it to
        /// decide which fields to omit from the input types it generates, and that runs
        /// before export. `IntermediateField` has no `computed` member, so emitting it
        /// produced a `schema.json` the compiler refuses outright.
        [<JsonIgnore>]
        computed: bool
        /// pgvector configuration on a `Vector` / `BitVector` / `HalfVector` /
        /// `SparseVector` field. The compiler refuses such a field without one, so
        /// dropping it here would not be a silent loss — it would make the four pgvector
        /// field types unauthorable in F#.
        vector_config: VectorConfig option
        /// On a `Float` field, the vector field whose `nearest` search distance this
        /// field carries. Selecting it on a query that did not run that search is
        /// refused, not answered with null.
        vector_distance: string option
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
    }

/// Optional REST endpoint annotation for a query or mutation.
[<CLIMutable>]
type RestConfig =
    {
        /// The HTTP method: "GET", "POST", "PUT", "PATCH", or "DELETE".
        method: string
        /// The URL path template, e.g. "/users/:id".
        path: string
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
        /// Optional REST endpoint annotation.
        rest: RestConfig option
        /// Server-injected parameters, keyed by SQL parameter name, values `"jwt:<claim>"`.
        ///
        /// Not exposed as GraphQL arguments — this is how a query is scoped to the
        /// caller's tenant. There was no field to carry one, so no F#-authored query
        /// could compile with a tenant predicate.
        inject_params: Map<string, string> option
        /// Role required to execute this query and to see it in introspection.
        requires_role: string option
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
        /// Optional REST endpoint annotation.
        rest: RestConfig option
        /// When true, this mutation uses cascade delete/update semantics.
        cascade: bool option
        /// Server-injected parameters, keyed by SQL parameter name, values `"jwt:<claim>"`.
        inject_params: Map<string, string> option
        /// Role required to execute this mutation.
        requires_role: string option
        /// Views whose cached query results must be invalidated after this mutation
        /// succeeds. Without them a write and the cached reads of what it wrote have no
        /// connection, and the new row stays invisible for the reader's whole TTL.
        invalidates_views: string list option
        /// Fact tables whose cached aggregates must be invalidated after this mutation.
        /// Unlike views there is no inference fallback.
        invalidates_fact_tables: string list option
    }

/// Represents a GraphQL input object type used as a mutation argument.
[<CLIMutable>]
type InputTypeDefinition =
    {
        /// The GraphQL input type name (e.g., "CreateUserInput").
        name: string
        /// The fields of this input type.
        fields: ArgumentDefinition list
        /// Optional human-readable description for introspection.
        description: string option
    }

/// A single member of a GraphQL enum.
[<CLIMutable>]
type EnumValueDefinition =
    {
        /// The member name, as it appears in a GraphQL document.
        name: string
    }

/// Represents a GraphQL enum type.
[<CLIMutable>]
type EnumDefinition =
    {
        /// The enum type name (e.g., "OrderStatus").
        name: string
        /// The enum members, in declaration order.
        values: EnumValueDefinition list
        /// Optional human-readable description for introspection.
        description: string option
    }

/// The root schema record serialized to schema.json.
[<CLIMutable>]
type IntermediateSchema =
    {
        /// Schema format version.
        version: string
        /// All GraphQL types defined in this schema.
        types: TypeDefinition list
        /// All GraphQL input types defined in this schema.
        input_types: InputTypeDefinition list
        /// All GraphQL enum types defined in this schema.
        enums: EnumDefinition list
        /// All GraphQL queries defined in this schema.
        queries: QueryDefinition list
        /// All GraphQL mutations defined in this schema.
        mutations: MutationDefinition list
    }

/// Discriminated union of all GraphQL scalar types.
type GraphQLScalar =
    | GqlInt
    | GqlFloat
    | GqlString
    | GqlBoolean
    | GqlId
    | GqlDateTime
    | GqlCustom of string
