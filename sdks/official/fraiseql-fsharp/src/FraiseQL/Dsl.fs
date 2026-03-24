namespace FraiseQL

/// Computation expression DSL for idiomatic F# schema authoring.
///
/// Use the <c>fraiseql { }</c> builder to define an <see cref="IntermediateSchema"/>
/// without touching global state. Nested builders (<c>TypeCEBuilder</c>,
/// <c>QueryCEBuilder</c>, <c>MutationCEBuilder</c>, <c>FieldBuilder</c>) accumulate
/// definitions via custom operations and return strongly-typed records.
///
/// Example:
/// <code>
/// let schema =
///     fraiseql {
///         type' "Author" (TypeCEBuilder("Author") {
///             sqlSource "v_author"
///             field "id" "ID" { nullable false }
///             field "name" "String" { nullable false }
///         })
///         query "authors" (QueryCEBuilder("authors") {
///             returnType "Author"
///             returnsList true
///             sqlSource "v_author"
///         })
///     }
/// </code>
module Dsl =

    // -------------------------------------------------------------------------
    // FieldBuilder
    // -------------------------------------------------------------------------

    /// Internal accumulator for <see cref="FieldBuilder"/>.
    type FieldState =
        {
            name: string
            type_: string
            nullable: bool
            description: string option
            scope: string option
        }

    /// Computation expression builder for a single <see cref="FieldDefinition"/>.
    ///
    /// Usage: <c>FieldBuilder("id", "ID") { nullable false }</c>
    type FieldBuilder(name: string, gqlType: string) =

        member _.Yield(_: unit) : FieldState =
            {
                name = name
                type_ = gqlType
                nullable = true
                description = None
                scope = None
            }

        member this.Zero() : FieldState = this.Yield(())

        member _.Delay(f: unit -> FieldState) = f ()

        member _.Run(s: FieldState) : FieldDefinition =
            {
                name = s.name
                type_ = s.type_
                nullable = s.nullable
                description = s.description
                scope = s.scope
            }

        /// Sets whether this field may be null.
        [<CustomOperation("nullable")>]
        member _.Nullable(s: FieldState, v: bool) : FieldState = { s with nullable = v }

        /// Sets the optional human-readable description.
        [<CustomOperation("description")>]
        member _.Description(s: FieldState, v: string) : FieldState =
            { s with description = Some v }

        /// Sets the single scope required to read this field.
        [<CustomOperation("scope")>]
        member _.Scope(s: FieldState, v: string) : FieldState = { s with scope = Some v }

    // -------------------------------------------------------------------------
    // TypeCEBuilder
    // -------------------------------------------------------------------------

    /// Internal accumulator for <see cref="TypeCEBuilder"/>.
    type TypeAccState =
        {
            name: string
            sqlSource: string
            description: string option
            fields: FieldDefinition list
            isInput: bool
            relay: bool
            isError: bool
            tenantScoped: bool
            keyFields: string[]
            extendsType: bool
        }

    /// Computation expression builder for a <see cref="TypeDefinition"/>.
    ///
    /// Usage: <c>TypeCEBuilder("Author") { sqlSource "v_author"; field "id" "ID" { nullable false } }</c>
    type TypeCEBuilder(name: string) =

        member _.Yield(_: unit) : TypeAccState =
            {
                name = name
                sqlSource = ""
                description = None
                fields = []
                isInput = false
                relay = false
                isError = false
                tenantScoped = false
                keyFields = [||]
                extendsType = false
            }

        member this.Zero() : TypeAccState = this.Yield(())

        member _.Delay(f: unit -> TypeAccState) = f ()

        member _.Run(s: TypeAccState) : TypeDefinition =
            {
                name = s.name
                sql_source = s.sqlSource
                description = s.description
                fields = s.fields
                is_input = s.isInput
                relay = s.relay
                is_error = s.isError
                tenant_scoped = s.tenantScoped
                key_fields = s.keyFields
                extends_type = s.extendsType
            }

        /// Sets the SQL view backing this type.
        [<CustomOperation("sqlSource")>]
        member _.SqlSource(s: TypeAccState, v: string) : TypeAccState = { s with sqlSource = v }

        /// Sets the optional human-readable description.
        [<CustomOperation("description")>]
        member _.Description(s: TypeAccState, v: string) : TypeAccState =
            { s with description = Some v }

        /// Marks this type as a GraphQL input type.
        [<CustomOperation("isInput")>]
        member _.IsInput(s: TypeAccState, v: bool) : TypeAccState = { s with isInput = v }

        /// Marks this type for Relay cursor pagination.
        [<CustomOperation("relay")>]
        member _.Relay(s: TypeAccState, v: bool) : TypeAccState = { s with relay = v }

        /// Marks this type as a mutation error response.
        [<CustomOperation("isError")>]
        member _.IsError(s: TypeAccState, v: bool) : TypeAccState = { s with isError = v }

        /// Marks this type as tenant-scoped for multi-tenant schemas.
        [<CustomOperation("tenantScoped")>]
        member _.TenantScoped(s: TypeAccState, v: bool) : TypeAccState = { s with tenantScoped = v }

        /// Sets federation key fields for entity resolution.
        [<CustomOperation("keyFields")>]
        member _.KeyFields(s: TypeAccState, v: string[]) : TypeAccState = { s with keyFields = v }

        /// Marks this type as extending a type defined in another subgraph.
        [<CustomOperation("extendsType")>]
        member _.ExtendsType(s: TypeAccState, v: bool) : TypeAccState = { s with extendsType = v }

        /// Adds a <see cref="FieldDefinition"/> to this type.
        [<CustomOperation("field")>]
        member _.Field(s: TypeAccState, fieldDef: FieldDefinition) : TypeAccState =
            { s with fields = s.fields @ [ fieldDef ] }

    // -------------------------------------------------------------------------
    // QueryCEBuilder
    // -------------------------------------------------------------------------

    /// Internal accumulator for <see cref="QueryCEBuilder"/>.
    type QueryCEAccState =
        {
            name: string
            returnType: string
            returnsList: bool
            nullable: bool
            sqlSource: string
            arguments: ArgumentDefinition list
            cacheTtlSeconds: int option
            description: string option
            restPath: string option
            restMethod: string option
        }

    /// Computation expression builder for a <see cref="QueryDefinition"/>.
    ///
    /// Usage:
    /// <code>
    /// QueryCEBuilder("authors") {
    ///     returnType "Author"
    ///     returnsList true
    ///     sqlSource "v_author"
    /// }
    /// </code>
    type QueryCEBuilder(name: string) =

        member _.Yield(_: unit) : QueryCEAccState =
            {
                name = name
                returnType = ""
                returnsList = false
                nullable = false
                sqlSource = ""
                arguments = []
                cacheTtlSeconds = None
                description = None
                restPath = None
                restMethod = None
            }

        member this.Zero() : QueryCEAccState = this.Yield(())

        member _.Delay(f: unit -> QueryCEAccState) = f ()

        member _.Run(s: QueryCEAccState) : QueryDefinition =
            {
                name = s.name
                return_type = s.returnType
                returns_list = s.returnsList
                nullable = s.nullable
                sql_source = s.sqlSource
                arguments = s.arguments
                cache_ttl_seconds = s.cacheTtlSeconds
                description = s.description
                rest_path = s.restPath
                rest_method = s.restMethod
            }

        /// Sets the GraphQL return type.
        [<CustomOperation("returnType")>]
        member _.ReturnType(s: QueryCEAccState, v: string) = { s with returnType = v }

        /// Sets whether this query returns a list.
        [<CustomOperation("returnsList")>]
        member _.ReturnsList(s: QueryCEAccState, v: bool) = { s with returnsList = v }

        /// Sets whether the result may be null.
        [<CustomOperation("nullable")>]
        member _.Nullable(s: QueryCEAccState, v: bool) = { s with nullable = v }

        /// Sets the SQL view or function backing this query.
        [<CustomOperation("sqlSource")>]
        member _.SqlSource(s: QueryCEAccState, v: string) = { s with sqlSource = v }

        /// Sets the cache TTL in seconds.
        [<CustomOperation("cacheTtlSeconds")>]
        member _.CacheTtlSeconds(s: QueryCEAccState, v: int) =
            { s with cacheTtlSeconds = Some v }

        /// Sets the optional human-readable description.
        [<CustomOperation("description")>]
        member _.Description(s: QueryCEAccState, v: string) =
            { s with description = Some v }

        /// Sets the REST endpoint path for this query.
        [<CustomOperation("restPath")>]
        member _.RestPath(s: QueryCEAccState, v: string) = { s with restPath = Some v }

        /// Sets the HTTP method for the REST endpoint.
        [<CustomOperation("restMethod")>]
        member _.RestMethod(s: QueryCEAccState, v: string) = { s with restMethod = Some v }

        /// Adds an argument to this query.
        [<CustomOperation("arg")>]
        member _.Arg(s: QueryCEAccState, name: string, type_: string, isNullable: bool) =
            let a: ArgumentDefinition = { name = name; type_ = type_; nullable = isNullable }
            { s with arguments = s.arguments @ [ a ] }

    // -------------------------------------------------------------------------
    // MutationCEBuilder
    // -------------------------------------------------------------------------

    /// Internal accumulator for <see cref="MutationCEBuilder"/>.
    type MutationCEAccState =
        {
            name: string
            returnType: string
            sqlSource: string
            operation: string
            arguments: ArgumentDefinition list
            description: string option
            restPath: string option
            restMethod: string option
        }

    /// Computation expression builder for a <see cref="MutationDefinition"/>.
    ///
    /// Usage:
    /// <code>
    /// MutationCEBuilder("createAuthor") {
    ///     returnType "Author"
    ///     sqlSource "fn_create_author"
    ///     operation "insert"
    /// }
    /// </code>
    type MutationCEBuilder(name: string) =

        member _.Yield(_: unit) : MutationCEAccState =
            {
                name = name
                returnType = ""
                sqlSource = ""
                operation = "custom"
                arguments = []
                description = None
                restPath = None
                restMethod = None
            }

        member this.Zero() : MutationCEAccState = this.Yield(())

        member _.Delay(f: unit -> MutationCEAccState) = f ()

        member _.Run(s: MutationCEAccState) : MutationDefinition =
            {
                name = s.name
                return_type = s.returnType
                sql_source = s.sqlSource
                operation = s.operation
                arguments = s.arguments
                description = s.description
                rest_path = s.restPath
                rest_method = s.restMethod
            }

        /// Sets the GraphQL return type.
        [<CustomOperation("returnType")>]
        member _.ReturnType(s: MutationCEAccState, v: string) = { s with returnType = v }

        /// Sets the SQL function backing this mutation.
        [<CustomOperation("sqlSource")>]
        member _.SqlSource(s: MutationCEAccState, v: string) = { s with sqlSource = v }

        /// Sets the operation kind: "insert", "update", "delete", or "custom".
        [<CustomOperation("operation")>]
        member _.Operation(s: MutationCEAccState, v: string) = { s with operation = v }

        /// Sets the optional human-readable description.
        [<CustomOperation("description")>]
        member _.Description(s: MutationCEAccState, v: string) =
            { s with description = Some v }

        /// Sets the REST endpoint path for this mutation.
        [<CustomOperation("restPath")>]
        member _.RestPath(s: MutationCEAccState, v: string) = { s with restPath = Some v }

        /// Sets the HTTP method for the REST endpoint.
        [<CustomOperation("restMethod")>]
        member _.RestMethod(s: MutationCEAccState, v: string) = { s with restMethod = Some v }

        /// Adds an argument to this mutation.
        [<CustomOperation("arg")>]
        member _.Arg(s: MutationCEAccState, name: string, type_: string, isNullable: bool) =
            let a: ArgumentDefinition = { name = name; type_ = type_; nullable = isNullable }
            { s with arguments = s.arguments @ [ a ] }

    // -------------------------------------------------------------------------
    // FraiseQLBuilder (outer CE)
    // -------------------------------------------------------------------------

    /// Discriminated union used internally by <see cref="FraiseQLBuilder"/> to accumulate
    /// schema items in a single list before assembling the final <see cref="IntermediateSchema"/>.
    type SchemaItem =
        | TypeItem of TypeDefinition
        | QueryItem of QueryDefinition
        | MutationItem of MutationDefinition

    /// Outer computation expression builder that assembles a complete <see cref="IntermediateSchema"/>.
    ///
    /// Example:
    /// <code>
    /// let schema =
    ///     fraiseql {
    ///         type' "Author" (TypeCEBuilder("Author") { sqlSource "v_author" })
    ///         query "authors" (QueryCEBuilder("authors") { returnType "Author"; sqlSource "v_author"; returnsList true })
    ///         mutation "createAuthor" (MutationCEBuilder("createAuthor") { returnType "Author"; sqlSource "fn_create_author"; operation "insert" })
    ///     }
    /// </code>
    type FraiseQLBuilder() =

        member _.Yield(_: unit) : SchemaItem list = []

        member _.Combine(a: SchemaItem list, b: SchemaItem list) : SchemaItem list = a @ b

        member _.Delay(f: unit -> SchemaItem list) : SchemaItem list = f ()

        member _.Zero() : SchemaItem list = []

        member _.Run(items: SchemaItem list) : IntermediateSchema =
            {
                version = "2.0.0"
                types = items |> List.choose (function TypeItem t -> Some t | _ -> None)
                queries = items |> List.choose (function QueryItem q -> Some q | _ -> None)
                mutations = items |> List.choose (function MutationItem m -> Some m | _ -> None)
            }

        /// Adds a <see cref="TypeDefinition"/> to the schema.
        [<CustomOperation("type'")>]
        member _.TypeOp(state: SchemaItem list, typeDef: TypeDefinition) : SchemaItem list =
            state @ [ TypeItem typeDef ]

        /// Adds a <see cref="QueryDefinition"/> to the schema.
        [<CustomOperation("query")>]
        member _.QueryOp(state: SchemaItem list, queryDef: QueryDefinition) : SchemaItem list =
            state @ [ QueryItem queryDef ]

        /// Adds a <see cref="MutationDefinition"/> to the schema.
        [<CustomOperation("mutation")>]
        member _.MutationOp(state: SchemaItem list, mutDef: MutationDefinition) : SchemaItem list =
            state @ [ MutationItem mutDef ]

/// The singleton <see cref="FraiseQLBuilder"/> instance. Open this module or use
/// <c>open FraiseQL.Dsl</c> to bring <c>fraiseql</c> into scope.
[<AutoOpen>]
module FraiseQLBuilderModule =

    /// The entry point for the <c>fraiseql { }</c> computation expression DSL.
    let fraiseql = Dsl.FraiseQLBuilder()
