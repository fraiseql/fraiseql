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
            rest: RestConfig option
            injectParams: Map<string, string>
            requiresRole: string option
            requiresActor: string list option
            paginationOrder: string option
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
            rest = None
            injectParams = Map.empty
            requiresRole = None
            requiresActor = None
            paginationOrder = None
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

    /// Adds an argument to this query.
    let withArgument (name: string) (type_: string) (isNullable: bool) (s: QueryState) : QueryState =
        let arg: ArgumentDefinition = { name = name; type_ = type_; nullable = isNullable }
        { s with arguments = s.arguments @ [ arg ] }

    /// Sets the optional REST endpoint annotation.
    let rest (cfg: RestConfig) (s: QueryState) : QueryState = { s with rest = Some cfg }

    /// Declares a server-injected parameter, not exposed as a GraphQL argument.
    /// `source` is of the form `"jwt:&lt;claim&gt;"`.
    let inject (parameter: string) (source: string) (s: QueryState) : QueryState =
        { s with injectParams = Map.add parameter source s.injectParams }

    /// Restricts this query to callers holding the given role.
    let requiresRole (role: string) (s: QueryState) : QueryState = { s with requiresRole = Some role }

    /// Restricts this query to an allow-list of actor types (#966).
    ///
    /// Enforced in the same executor gate as `requiresRole`, on every transport. Until
    /// #1123 it was expressible only by hand-writing `schema.json`.
    let requiresActor (actors: string list) (s: QueryState) : QueryState =
        ActorType.validate (sprintf "query '%s'" s.name) actors
        { s with requiresActor = Some actors }

    /// Names the column that orders this query's LIMIT/OFFSET pages (#1303).
    ///
    /// Pass `"none"` to keep a self-ordering view's own ORDER BY. Omit the call entirely and
    /// the compiler derives the entity identity, which is what almost every query wants.
    /// Dropping a declared order does not empty a result or fail a compile — it produces a
    /// different total order over the same rows, which reads as a working schema until
    /// someone compares two pages.
    ///
    /// The value is interpolated into ORDER BY and is validated by the compiler, which is also
    /// where "declared on a query that does not paginate" is refused: this builder cannot see
    /// the resolved auto_params that decide it.
    let paginationOrder (column: string) (s: QueryState) : QueryState =
        { s with paginationOrder = Some column }

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
            rest = s.rest
            inject_params = (if Map.isEmpty s.injectParams then None else Some s.injectParams)
            requires_role = s.requiresRole
            requires_actor = s.requiresActor
            pagination_order = s.paginationOrder
        }

    /// Converts the state to a <see cref="QueryDefinition"/> and registers it in <see cref="SchemaRegistry"/>.
    let register (s: QueryState) : unit = SchemaRegistry.registerQuery (toDefinition s)
