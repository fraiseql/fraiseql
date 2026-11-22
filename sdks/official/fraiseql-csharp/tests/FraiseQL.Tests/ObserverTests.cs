using System.Text.Json.Nodes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>Observer authoring (#2147): an observer per action kind, as the compiler reads it.</summary>
[Collection(RegistryTestCollection.Name)]
public sealed class ObserverTests : IDisposable
{
    public ObserverTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    private static JsonNode Exported() => JsonNode.Parse(SchemaExporter.Export())!;

    private static JsonArray Observers() => Exported()["observers"]!.AsArray();

    private static void AssertJson(string expected, JsonNode? actual) =>
        Assert.True(
            JsonNode.DeepEquals(JsonNode.Parse(expected), actual),
            $"expected {expected}\nactual   {actual?.ToJsonString()}");

    [Fact]
    public void AWebhookObserverCarriesEveryKeyItIsGiven()
    {
        ObserverBuilder.Observer("orderPlacedWebhook")
            .Entity("Order")
            .Event("INSERT")
            .Action(ObserverAction.Webhook(
                urlEnv: "HOOK_URL",
                method: "PUT",
                bodyTemplate: "{\"total\": {{ total }}}",
                signingSecretEnv: "HOOK_KEY",
                headers: new Dictionary<string, string> { ["X-Source"] = "tests" },
                headersEnv: new Dictionary<string, string> { ["Authorization"] = "HOOK_TOKEN" }))
            .Retry(new RetryConfig(
                MaxAttempts: 5, BackoffStrategy: "linear", InitialDelayMs: 200, MaxDelayMs: 5000))
            .Register();

        AssertJson("""
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
            """, Observers());
    }

    private static JsonNode? ActionJson(ObserverAction action)
    {
        ObserverBuilder.Observer("probe").Entity("Order").Event("INSERT").Action(action).Register();
        return Observers()[0]!["actions"]![0];
    }

    [Fact]
    public void AWebhookGivenOnlyItsUrlAuthorsOnlyItsUrl()
    {
        AssertJson("""{"type": "webhook", "url": "https://hooks.example/x"}""",
            ActionJson(ObserverAction.Webhook("https://hooks.example/x")));
    }

    [Theory]
    [InlineData(null, null)]
    [InlineData("https://hooks.example/x", "HOOK_URL")]
    public void AWebhookHasExactlyOneDestination(string? url, string? urlEnv)
    {
        var refused = Assert.Throws<ArgumentException>(() => ObserverAction.Webhook(url, urlEnv));
        Assert.Contains("exactly one of url and urlEnv", refused.Message);
    }

    [Fact]
    public void ASlackActionReadsTheDefaultEnv()
    {
        AssertJson("""
            {"type": "slack", "channel": "#orders", "message_template": "Order {{ id }} shipped",
             "webhook_url_env": "SLACK_WEBHOOK_URL"}
            """, ActionJson(ObserverAction.Slack("#orders", "Order {{ id }} shipped")));
    }

    [Fact]
    public void ASlackActionReadsTheEnvItIsGiven()
    {
        AssertJson("""
            {"type": "slack", "channel": "#o", "message_template": "m", "webhook_url_env": "OPS_SLACK"}
            """, ActionJson(ObserverAction.Slack("#o", "m", webhookUrlEnv: "OPS_SLACK")));
    }

    [Fact]
    public void ASlackActionGivenAUrlEmitsNoDefaultEnvBesideIt()
    {
        AssertJson("""
            {"type": "slack", "channel": "#o", "message_template": "m",
             "webhook_url": "https://hooks.slack.example/x"}
            """, ActionJson(ObserverAction.Slack("#o", "m", webhookUrl: "https://hooks.slack.example/x")));
    }

    [Fact]
    public void ASlackActionHasOneDestination()
    {
        var refused = Assert.Throws<ArgumentException>(() =>
            ObserverAction.Slack("#o", "m", webhookUrl: "https://x", webhookUrlEnv: "OPS_SLACK"));
        Assert.Contains("at most one of webhookUrl and webhookUrlEnv", refused.Message);
    }

    [Fact]
    public void AnEmailActionNamesItsBodyTemplate()
    {
        AssertJson("""
            {"type": "email", "to": "ops@example.com", "subject": "Order deleted",
             "body_template": "Order {{ id }}"}
            """, ActionJson(ObserverAction.Email("ops@example.com", "Order deleted", "Order {{ id }}")));
    }

    [Fact]
    public void AConditionIsEmittedAndAnUnsetRetryIsLeftOut()
    {
        ObserverBuilder.Observer("orderShipped")
            .Entity("Order")
            .Event("UPDATE")
            .Condition("status == 'shipped'")
            .Action(ObserverAction.Email("ops@example.com", "s", "b"))
            .Register();

        var definition = Observers()[0]!.AsObject();
        Assert.Equal("status == 'shipped'", (string?)definition["condition"]);
        // No retry: the server's own policy applies, so no block is written.
        Assert.False(definition.ContainsKey("retry"));
    }

    [Fact]
    public void NoObserverLeavesTheSectionOut()
    {
        Assert.False(Exported().AsObject().ContainsKey("observers"));
    }

    [Fact]
    public void AnObserverNameIsRegisteredOnce()
    {
        void Declare() => ObserverBuilder.Observer("orderCreated")
            .Entity("Order")
            .Event("INSERT")
            .Action(ObserverAction.Email("a@example.com", "s", "b"))
            .Register();

        Declare();
        var refused = Assert.Throws<InvalidOperationException>(Declare);
        Assert.Contains("observer 'orderCreated' is already registered", refused.Message);
    }
}
