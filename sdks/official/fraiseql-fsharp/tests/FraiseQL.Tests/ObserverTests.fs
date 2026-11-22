/// Observer authoring (#2147): an observer per action kind, as the compiler reads it. An
/// action's destination is a union, so a webhook has exactly one by construction.
module FraiseQL.Tests.ObserverTests

open System
open System.Text.Json.Nodes
open Xunit
open FraiseQL

let private exported () : JsonObject =
    (JsonNode.Parse(SchemaExporter.export ())).AsObject()

let private assertJson (expected: string) (actual: JsonNode) =
    Assert.True(
        JsonNode.DeepEquals(JsonNode.Parse(expected), actual),
        sprintf "expected %s\nactual   %s" expected (actual.ToJsonString())
    )

/// Registers one observer with this action and returns the action as exported.
let private actionJson (add: ObserverBuilder.ObserverState -> ObserverBuilder.ObserverState) : JsonNode =
    SchemaRegistry.reset ()
    ObserverBuilder.observer "probe"
    |> ObserverBuilder.entity "Order"
    |> ObserverBuilder.event "INSERT"
    |> add
    |> ObserverBuilder.register
    let action = exported().["observers"].[0].["actions"].[0]
    SchemaRegistry.reset ()
    action

[<Fact>]
let ``a webhook observer carries every key it is given`` () =
    SchemaRegistry.reset ()
    ObserverBuilder.observer "orderPlacedWebhook"
    |> ObserverBuilder.entity "Order"
    |> ObserverBuilder.event "INSERT"
    |> ObserverBuilder.webhook
        { ObserverAction.webhook (WebhookDestination.UrlEnv "HOOK_URL") with
            method = Some "PUT"
            body_template = Some """{"total": {{ total }}}"""
            signing_secret_env = Some "HOOK_KEY"
            headers = Map [ "X-Source", "tests" ]
            headers_env = Map [ "Authorization", "HOOK_TOKEN" ] }
    |> ObserverBuilder.retry
        { max_attempts = 5; backoff_strategy = "linear"; initial_delay_ms = 200; max_delay_ms = 5000 }
    |> ObserverBuilder.register

    assertJson
        """
        [{
          "name": "orderPlacedWebhook", "entity": "Order", "event": "INSERT",
          "actions": [{
            "type": "webhook", "url_env": "HOOK_URL", "method": "PUT",
            "body_template": "{\"total\": {{ total }}}", "signing_secret_env": "HOOK_KEY",
            "headers": {"X-Source": "tests"}, "headers_env": {"Authorization": "HOOK_TOKEN"}
          }],
          "retry": {"max_attempts": 5, "backoff_strategy": "linear",
                    "initial_delay_ms": 200, "max_delay_ms": 5000}
        }]
        """
        (exported().["observers"])
    SchemaRegistry.reset ()

[<Fact>]
let ``a webhook given only its url authors only its url`` () =
    assertJson
        """{"type": "webhook", "url": "https://hooks.example/x"}"""
        (actionJson (ObserverBuilder.webhook (ObserverAction.webhook (WebhookDestination.Url "https://hooks.example/x"))))

[<Fact>]
let ``a slack action reads the default env`` () =
    assertJson
        """{"type": "slack", "channel": "#orders", "message_template": "Order {{ id }} shipped",
            "webhook_url_env": "SLACK_WEBHOOK_URL"}"""
        (actionJson (ObserverBuilder.slack (ObserverAction.slack "#orders" "Order {{ id }} shipped")))

[<Fact>]
let ``a slack action reads the env it is given`` () =
    assertJson
        """{"type": "slack", "channel": "#o", "message_template": "m", "webhook_url_env": "OPS_SLACK"}"""
        (actionJson (
            ObserverBuilder.slack
                { ObserverAction.slack "#o" "m" with destination = SlackDestination.WebhookUrlEnv "OPS_SLACK" }
        ))

[<Fact>]
let ``a slack action given a url emits no default env beside it`` () =
    assertJson
        """{"type": "slack", "channel": "#o", "message_template": "m",
            "webhook_url": "https://hooks.slack.example/x"}"""
        (actionJson (
            ObserverBuilder.slack
                { ObserverAction.slack "#o" "m" with
                    destination = SlackDestination.WebhookUrl "https://hooks.slack.example/x" }
        ))

[<Fact>]
let ``an email action names its body template`` () =
    assertJson
        """{"type": "email", "to": "ops@example.com", "subject": "Order deleted",
            "body_template": "Order {{ id }}"}"""
        (actionJson (ObserverBuilder.email (ObserverAction.email "ops@example.com" "Order deleted" "Order {{ id }}")))

[<Fact>]
let ``a condition is emitted and an unset retry is left out`` () =
    SchemaRegistry.reset ()
    ObserverBuilder.observer "orderShipped"
    |> ObserverBuilder.entity "Order"
    |> ObserverBuilder.event "UPDATE"
    |> ObserverBuilder.condition "status == 'shipped'"
    |> ObserverBuilder.email (ObserverAction.email "ops@example.com" "s" "b")
    |> ObserverBuilder.register
    let definition = exported().["observers"].[0].AsObject()
    SchemaRegistry.reset ()
    Assert.Equal("status == 'shipped'", definition.["condition"].GetValue<string>())
    // No retry: the server's own policy applies, so no block is written.
    Assert.False(definition.ContainsKey "retry")

[<Fact>]
let ``no observer leaves the section out`` () =
    SchemaRegistry.reset ()
    Assert.False(exported().ContainsKey "observers")

[<Fact>]
let ``an observer name is registered once`` () =
    SchemaRegistry.reset ()
    let declare () =
        ObserverBuilder.observer "orderCreated"
        |> ObserverBuilder.entity "Order"
        |> ObserverBuilder.event "INSERT"
        |> ObserverBuilder.email (ObserverAction.email "a@example.com" "s" "b")
        |> ObserverBuilder.register
    declare ()
    let refused = Assert.Throws<InvalidOperationException>(Action declare)
    SchemaRegistry.reset ()
    Assert.Contains("observer 'orderCreated' is already registered", refused.Message)
