namespace FraiseQL

/// Pipe-friendly builder for constructing <see cref="QueryDefinition"/> values.
///
/// Example:
/// <code>
/// QueryBuilder.query "authors"
/// |> QueryBuilder.returnType "Author"
/// |> QueryBuilder.returnsList true
/// |> QueryBuilder.sqlSource "v_author"
/// |> QueryBuilder.register
/// </code>
module QueryBuilder =

    /// Internal accumulator state for building a <see cref="QueryDefinition"/>.
    type QueryState =
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

    /// Creates a new <see cref="QueryState"/> for the given query name.
    let query (name: string) : QueryState =
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

    /// Sets the GraphQL return type for this query.
    let returnType (t: string) (s: QueryState) : QueryState = { s with returnType = t }

    /// Sets whether this query returns a list of items.
    let returnsList (b: bool) (s: QueryState) : QueryState = { s with returnsList = b }

    /// Sets whether the query result may be null.
    let nullable (b: bool) (s: QueryState) : QueryState = { s with nullable = b }

    /// Sets the SQL view or function backing this query.
    let sqlSource (src: string) (s: QueryState) : QueryState = { s with sqlSource = src }

    /// Sets the optional cache TTL in seconds.
    let cacheTtlSeconds (ttl: int) (s: QueryState) : QueryState =
        { s with cacheTtlSeconds = Some ttl }

    /// Sets the optional human-readable description.
    let description (d: string) (s: QueryState) : QueryState = { s with description = Some d }

    /// Sets the REST path for this query.
    let restPath (p: string) (s: QueryState) : QueryState = { s with restPath = Some p }

    /// Sets the REST HTTP method for this query.
    let restMethod (m: string) (s: QueryState) : QueryState = { s with restMethod = Some m }

    /// Adds an argument to this query.
    let withArgument (name: string) (type_: string) (isNullable: bool) (s: QueryState) : QueryState =
        let arg: ArgumentDefinition = { name = name; type_ = type_; nullable = isNullable }
        { s with arguments = s.arguments @ [ arg ] }

    /// Converts the accumulated state into a <see cref="QueryDefinition"/>.
    /// Raises <see cref="System.InvalidOperationException"/> when required fields are missing.
    let toDefinition (s: QueryState) : QueryDefinition =
        if s.returnType = "" then
            raise (System.InvalidOperationException(sprintf "Query '%s' has no returnType" s.name))

        if s.sqlSource = "" then
            raise (System.InvalidOperationException(sprintf "Query '%s' has no sqlSource" s.name))

        {
            name = s.name
            return_type = s.returnType
            returns_list = s.returnsList
            nullable = s.nullable
            sql_source = s.sqlSource
            arguments = s.arguments
            cache_ttl_seconds = s.cacheTtlSeconds
            description = s.description
            rest =
                s.restPath
                |> Option.map (fun p ->
                    { path = p; method = s.restMethod |> Option.defaultValue "GET" })
        }

    /// Converts the state to a <see cref="QueryDefinition"/> and registers it in <see cref="SchemaRegistry"/>.
    let register (s: QueryState) : unit = SchemaRegistry.registerQuery (toDefinition s)
