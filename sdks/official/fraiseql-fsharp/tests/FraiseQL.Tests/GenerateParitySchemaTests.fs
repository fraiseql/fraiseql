/// Generate parity schema for cross-SDK comparison.
///
/// Usage:
///   SCHEMA_OUTPUT_FILE=/tmp/schema_fsharp.json dotnet test --filter GenerateParitySchema
module FraiseQL.Tests.GenerateParitySchemaTests

open System
open System.IO
open System.Text.Json
open System.Text.Json.Nodes
open Xunit

let private makeField (name: string) (typ: string) (nullable: bool) =
    let f = JsonObject()
    f["name"] <- name
    f["type"] <- typ
    f["nullable"] <- nullable
    f

let private makeArgument (name: string) (typ: string) (nullable: bool) =
    let a = JsonObject()
    a["name"] <- name
    a["type"] <- typ
    a["nullable"] <- nullable
    a

let private makeType (name: string) (sqlSource: string) (fields: JsonObject list) =
    let t = JsonObject()
    t["name"] <- name
    t["sql_source"] <- sqlSource
    let fa = JsonArray()
    for f in fields do
        fa.Add(f)
    t["fields"] <- fa
    t

[<Fact>]
let ``GenerateParitySchema`` () =
    let root = JsonObject()

    // ── Types ────────────────────────────────────────────────────────────

    let types = JsonArray()

    types.Add(
        makeType "User" "v_user" [
            makeField "id" "ID" false
            makeField "email" "String" false
            makeField "name" "String" false
        ]
    )

    types.Add(
        makeType "Order" "v_order" [
            makeField "id" "ID" false
            makeField "total" "Float" false
        ]
    )

    let userNotFound =
        makeType "UserNotFound" "v_user_not_found" [
            makeField "message" "String" false
            makeField "code" "String" false
        ]
    userNotFound["is_error"] <- true
    types.Add(userNotFound)

    root["types"] <- types

    // ── Queries ──────────────────────────────────────────────────────────

    let queries = JsonArray()

    let users = JsonObject()
    users["name"] <- "users"
    users["return_type"] <- "User"
    users["returns_list"] <- true
    users["nullable"] <- false
    users["sql_source"] <- "v_user"
    users["arguments"] <- JsonArray()
    queries.Add(users)

    let tenantOrders = JsonObject()
    tenantOrders["name"] <- "tenantOrders"
    tenantOrders["return_type"] <- "Order"
    tenantOrders["returns_list"] <- true
    tenantOrders["nullable"] <- false
    tenantOrders["sql_source"] <- "v_order"
    let injectTenant = JsonObject()
    injectTenant["tenant_id"] <- "jwt:tenant_id"
    tenantOrders["inject_params"] <- injectTenant
    tenantOrders["cache_ttl_seconds"] <- 300
    tenantOrders["requires_role"] <- "admin"
    tenantOrders["arguments"] <- JsonArray()
    queries.Add(tenantOrders)

    root["queries"] <- queries

    // ── Mutations ────────────────────────────────────────────────────────

    let mutations = JsonArray()

    let createUser = JsonObject()
    createUser["name"] <- "createUser"
    createUser["return_type"] <- "User"
    createUser["sql_source"] <- "fn_create_user"
    createUser["operation"] <- "insert"
    let createUserArgs = JsonArray()
    createUserArgs.Add(makeArgument "email" "String" false)
    createUserArgs.Add(makeArgument "name" "String" false)
    createUser["arguments"] <- createUserArgs
    mutations.Add(createUser)

    let placeOrder = JsonObject()
    placeOrder["name"] <- "placeOrder"
    placeOrder["return_type"] <- "Order"
    placeOrder["sql_source"] <- "fn_place_order"
    placeOrder["operation"] <- "insert"
    let injectUser = JsonObject()
    injectUser["user_id"] <- "jwt:sub"
    placeOrder["inject_params"] <- injectUser
    let invalidViews = JsonArray()
    invalidViews.Add("v_order_summary")
    placeOrder["invalidates_views"] <- invalidViews
    let invalidTables = JsonArray()
    invalidTables.Add("tf_sales")
    placeOrder["invalidates_fact_tables"] <- invalidTables
    placeOrder["arguments"] <- JsonArray()
    mutations.Add(placeOrder)

    root["mutations"] <- mutations

    // ── Output ───────────────────────────────────────────────────────────

    let options = JsonSerializerOptions(WriteIndented = true)
    let json = root.ToJsonString(options)

    match Environment.GetEnvironmentVariable("SCHEMA_OUTPUT_FILE") with
    | null
    | "" -> printfn "%s" json
    | path -> File.WriteAllText(path, json)
