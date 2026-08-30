/// Authors the cross-SDK conformance fixture with the F# SDK's public API.
///
/// Driven by `sdks/official/conformance/run.py`; see
/// `sdks/official/conformance/README.md`.
///
/// The one rule for every SDK's copy of this file: author through the SDK, never
/// hand-assemble the JSON.
///
/// `Nullable = false` is spelled out on every non-null field because this SDK's
/// `GraphQLFieldAttribute.Nullable` defaults to `true` — GraphQL's own default for an
/// unadorned type, but the opposite of the C# SDK's default for the same attribute name. The pre-existing `GenerateParitySchema` built the expected
/// document with `JsonObject` literals and never called `SchemaExporter`, so it was
/// structurally incapable of failing.
module FraiseQL.Tests.ConformanceExportTests

open System
open Xunit
open FraiseQL

[<GraphQLType(Name = "User", SqlSource = "v_user")>]
type ConformanceMinimalUser() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Email = "" with get, set

[<GraphQLType(Name = "User", SqlSource = "v_user", Relay = true)>]
type ConformanceUser() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Email = "" with get, set

    [<GraphQLField(Type = "String",
                   Nullable = true,
                   Description = "The user's \"display\" name",
                   DeprecationReason = "use displayName")>]
    member val Name = "" with get, set

    [<GraphQLField(Type = "Float", Nullable = true, Scope = "read:User.salary")>]
    member val Salary = 0.0 with get, set

    // Two words and a digit segment (#1249). An F# member is PascalCase and the
    // first letter is lowered on emit, so this fixture discriminates: `Phone1` must
    // reach the wire as `phone1`.
    [<GraphQLField(Type = "String", Nullable = true)>]
    member val LastLoginAt = "" with get, set

    [<GraphQLField(Type = "String", Nullable = true)>]
    member val Phone1 = "" with get, set

[<GraphQLType(Name = "Order", SqlSource = "v_order")>]
type ConformanceOrder() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "Float", Nullable = false)>]
    member val Total = 0.0 with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Status = "" with get, set

// `Crud` is an authoring-time expansion the compiler has no concept of, so the only
// evidence this SDK implements it is that the operations and input objects appear in the
// compiled schema. `Computed` is the same: emitting the flag makes the document
// uncompilable, so the sole evidence it was honoured is `slug` on the type and absent from
// both input objects.
[<GraphQLType(Name = "SupportTicket", SqlSource = "v_support_ticket", Crud = true)>]
type ConformanceSupportTicket() =
    [<GraphQLField(Type = "Int", Nullable = false)>]
    member val Id = 0 with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Title = "" with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val DueDate = "" with get, set

    [<GraphQLField(Type = "String", Nullable = false, Computed = true)>]
    member val Slug = "" with get, set

[<GraphQLType(Name = "UserNotFound", SqlSource = "v_user_not_found", IsError = true)>]
type ConformanceUserNotFound() =
    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Message = "" with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Code = "" with get, set

[<GraphQLType(Name = "Document", SqlSource = "v_document")>]
type ConformanceDocument() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "Vector",
                   Nullable = false,
                   VectorDimensions = 1536,
                   VectorIndexType = VectorIndex.ivfFlat,
                   VectorDistanceMetric = VectorMetric.l2)>]
    member val Embedding: float array = [||] with get, set

    [<GraphQLField(Type = "BitVector",
                   Nullable = false,
                   VectorDimensions = 768,
                   VectorDistanceMetric = VectorMetric.hamming)>]
    member val Fingerprint = "" with get, set

    [<GraphQLField(Type = "HalfVector",
                   Nullable = true,
                   VectorDimensions = 1536,
                   VectorDistanceMetric = VectorMetric.innerProduct)>]
    member val Compact: float array = [||] with get, set

    [<GraphQLField(Type = "SparseVector",
                   Nullable = true,
                   VectorDimensions = 30000,
                   VectorIndexType = VectorIndex.none)>]
    member val Terms = "" with get, set

    [<GraphQLField(Type = "Float", Nullable = false, VectorDistance = "embedding")>]
    member val Similarity = 0.0 with get, set

let private authorMinimal () =
    SchemaRegistry.register typeof<ConformanceMinimalUser>

    QueryBuilder.query "users"
    |> QueryBuilder.returnType "User"
    |> QueryBuilder.returnsList true
    |> QueryBuilder.sqlSource "v_user"
    |> QueryBuilder.register

let private authorFull () =
    SchemaRegistry.register typeof<ConformanceUser>
    SchemaRegistry.register typeof<ConformanceOrder>
    SchemaRegistry.register typeof<ConformanceSupportTicket>
    SchemaRegistry.register typeof<ConformanceUserNotFound>
    SchemaRegistry.register typeof<ConformanceDocument>

    SchemaRegistry.registerInput
        {
            name = "CreateUserInput"
            fields =
                [
                    { name = "email"; type_ = "String"; nullable = false }
                    { name = "name"; type_ = "String"; nullable = true }
                ]
            description = None
        }

    SchemaRegistry.registerEnum "OrderStatus" [ "PENDING"; "SHIPPED"; "CANCELLED" ] None

    QueryBuilder.query "users"
    |> QueryBuilder.returnType "User"
    |> QueryBuilder.returnsList true
    |> QueryBuilder.sqlSource "v_user"
    |> QueryBuilder.register

    QueryBuilder.query "user"
    |> QueryBuilder.returnType "User"
    |> QueryBuilder.nullable true
    |> QueryBuilder.sqlSource "v_user"
    |> QueryBuilder.withArgument "id" "ID" false
    |> QueryBuilder.register

    QueryBuilder.query "tenantOrders"
    |> QueryBuilder.returnType "Order"
    |> QueryBuilder.returnsList true
    |> QueryBuilder.sqlSource "v_order"
    |> QueryBuilder.inject "tenant_id" "jwt:tenant_id"
    |> QueryBuilder.cacheTtlSeconds 300
    |> QueryBuilder.requiresRole "admin"
    // #966's actor allow-list, enforced in the same executor gate as requiresRole on every
    // transport, and authorable in no SDK until #1123.
    |> QueryBuilder.requiresActor [ ActorType.humanUser; ActorType.serviceAccount ]
    |> QueryBuilder.register

    MutationBuilder.mutation "createUser"
    |> MutationBuilder.returnType "User"
    |> MutationBuilder.sqlSource "fn_create_user"
    |> MutationBuilder.operation "insert"
    |> MutationBuilder.withArgument "email" "String" false
    |> MutationBuilder.withArgument "name" "String" true
    |> MutationBuilder.invalidatesViews [ "v_user"; "v_user_summary" ]
    |> MutationBuilder.invalidatesFactTables [ "tf_signup" ]
    // #1253: the role gate on the write side, implemented in all eleven mutation builders
    // and compared in none until this construct.
    |> MutationBuilder.requiresRole "admin"
    |> MutationBuilder.requiresActor [ ActorType.serviceAccount ]
    |> MutationBuilder.register

    MutationBuilder.mutation "placeOrder"
    |> MutationBuilder.returnType "Order"
    |> MutationBuilder.sqlSource "fn_place_order"
    |> MutationBuilder.operation "insert"
    |> MutationBuilder.inject "user_id" "jwt:sub"
    |> MutationBuilder.invalidatesViews [ "v_order_summary" ]
    |> MutationBuilder.invalidatesFactTables [ "tf_sale" ]
    |> MutationBuilder.register

[<Fact>]
let ConformanceExport () =
    let fixture = Environment.GetEnvironmentVariable("FRAISEQL_CONFORMANCE_FIXTURE")
    let output = Environment.GetEnvironmentVariable("FRAISEQL_CONFORMANCE_OUT")

    // Not being driven by the harness — nothing to do. Failing here would break an
    // ordinary `dotnet test`.
    if not (isNull fixture) && not (isNull output) then
        SchemaRegistry.reset ()

        match fixture with
        | "minimal" -> authorMinimal ()
        | "full" -> authorFull ()
        | other -> failwithf "unknown fixture %s" other

        SchemaExporter.exportToFile output
