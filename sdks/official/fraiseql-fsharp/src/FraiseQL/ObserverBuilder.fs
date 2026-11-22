namespace FraiseQL

/// Builds the actions an observer runs (#2147). Each returns a record to adjust with
/// copy-and-update syntax before it is added with <see cref="ObserverBuilder"/>.
module ObserverAction =

    /// The variable a Slack action reads its incoming-webhook URL from, unless told another.
    [<Literal>]
    let DefaultSlackWebhookEnv = "SLACK_WEBHOOK_URL"

    /// A webhook to one destination, with no optional key set.
    let webhook (destination: WebhookDestination) : WebhookAction =
        {
            destination = destination
            method = None
            body_template = None
            signing_secret_env = None
            headers = Map.empty
            headers_env = Map.empty
        }

    /// A Slack post of `message`, a `{{ field }}` template, to `channel`, at the URL in
    /// <see cref="DefaultSlackWebhookEnv"/> unless `destination` is set to another.
    let slack (channel: string) (message: string) : SlackAction =
        {
            channel = channel
            message = message
            destination = SlackDestination.WebhookUrlEnv DefaultSlackWebhookEnv
        }

    /// An email to `recipient`; `body` is a `{{ field }}` template.
    let email (recipient: string) (subject: string) (body: string) : EmailAction =
        { ``to`` = recipient; subject = subject; body = body }

/// Pipe-friendly builder for an observer (#2147): actions run when a row of an entity is
/// inserted, updated or deleted.
///
/// Example:
/// <code>
/// ObserverBuilder.observer "orderShipped"
/// |> ObserverBuilder.entity "Order"
/// |> ObserverBuilder.event "UPDATE"
/// |> ObserverBuilder.condition "status == 'shipped'"
/// |> ObserverBuilder.slack (ObserverAction.slack "#orders" "Order {{ id }} shipped")
/// |> ObserverBuilder.register
/// </code>
module ObserverBuilder =

    type ObserverState =
        {
            name: string
            entity: string
            event: string
            condition: string option
            actions: Map<string, obj> list
            retry: RetryConfig option
        }

    /// Starts an observer of the given name, unique within a schema.
    let observer (name: string) : ObserverState =
        { name = name; entity = ""; event = ""; condition = None; actions = []; retry = None }

    /// The type whose rows are observed.
    let entity (e: string) (s: ObserverState) : ObserverState = { s with entity = e }

    /// "INSERT", "UPDATE" or "DELETE".
    let event (e: string) (s: ObserverState) : ObserverState = { s with event = e }

    /// An expression on the row; the actions run only when it holds.
    let condition (c: string) (s: ObserverState) : ObserverState = { s with condition = Some c }

    /// How a failed action is retried; with none, the server's own policy applies.
    let retry (r: RetryConfig) (s: ObserverState) : ObserverState = { s with retry = Some r }

    let private addAction (keys: (string * obj) list) (s: ObserverState) : ObserverState =
        { s with actions = s.actions @ [ Map keys ] }

    let private optional (key: string) (value: string option) : (string * obj) list =
        value |> Option.map (fun v -> key, box v) |> Option.toList

    let private nonEmpty (key: string) (value: Map<string, string>) : (string * obj) list =
        if Map.isEmpty value then [] else [ key, box value ]

    /// Adds a webhook action.
    let webhook (a: WebhookAction) (s: ObserverState) : ObserverState =
        let destination =
            match a.destination with
            | WebhookDestination.Url url -> "url", box url
            | WebhookDestination.UrlEnv envVar -> "url_env", box envVar
        addAction
            ([ "type", box "webhook"; destination ]
             @ optional "method" a.method
             @ optional "body_template" a.body_template
             @ optional "signing_secret_env" a.signing_secret_env
             @ nonEmpty "headers" a.headers
             @ nonEmpty "headers_env" a.headers_env)
            s

    /// Adds a Slack action.
    let slack (a: SlackAction) (s: ObserverState) : ObserverState =
        let destination =
            match a.destination with
            | SlackDestination.WebhookUrl url -> "webhook_url", box url
            | SlackDestination.WebhookUrlEnv envVar -> "webhook_url_env", box envVar
        addAction
            [ "type", box "slack"; "channel", box a.channel; "message_template", box a.message; destination ]
            s

    /// Adds an email action.
    let email (a: EmailAction) (s: ObserverState) : ObserverState =
        addAction
            [ "type", box "email"; "to", box a.``to``; "subject", box a.subject; "body_template", box a.body ]
            s

    /// Converts the state to an <see cref="ObserverDefinition"/>.
    let toDefinition (s: ObserverState) : ObserverDefinition =
        {
            name = s.name
            entity = s.entity
            event = s.event
            condition = s.condition
            actions = s.actions
            retry = s.retry
        }

    /// Registers the observer in <see cref="SchemaRegistry"/>.
    /// Raises <see cref="System.InvalidOperationException"/> if its name is already registered.
    let register (s: ObserverState) : unit = SchemaRegistry.registerObserver (toDefinition s)
