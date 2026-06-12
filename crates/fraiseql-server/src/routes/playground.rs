//! GraphQL playground routes (`GraphiQL` and Apollo Sandbox).

use axum::{
    extract::State,
    response::{Html, IntoResponse},
};

use crate::server_config::PlaygroundTool;

/// State for playground handler containing configuration.
#[derive(Clone)]
pub struct PlaygroundState {
    /// GraphQL endpoint URL (relative to server root).
    pub graphql_endpoint: String,
    /// Which playground tool to use.
    pub tool:             PlaygroundTool,
}

impl PlaygroundState {
    /// Create new playground state.
    #[must_use]
    pub fn new(graphql_endpoint: impl Into<String>, tool: PlaygroundTool) -> Self {
        Self {
            graphql_endpoint: graphql_endpoint.into(),
            tool,
        }
    }
}

/// Relaxed `Content-Security-Policy` for the playground IDE pages.
///
/// The IDEs load scripts/styles from public CDNs (unpkg for `GraphiQL`, the Apollo CDN +
/// sandbox iframe for Apollo Sandbox) and use inline scripts plus, for `GraphiQL`, web
/// workers / eval. They therefore need a relaxed CSP that the global strict
/// `script-src 'self'` would block. The global security-headers middleware preserves a
/// handler-set CSP (set-if-absent), so this value survives.
const PLAYGROUND_CSP: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline' 'unsafe-eval' https://unpkg.com https://embeddable-sandbox.cdn.apollographql.com; \
     style-src 'self' 'unsafe-inline' https://unpkg.com; \
     img-src 'self' data: https:; \
     font-src 'self' data: https://unpkg.com; \
     worker-src 'self' blob:; \
     connect-src 'self' https://*.apollographql.com; \
     frame-src https://sandbox.embed.apollographql.com";

/// Playground HTTP handler.
///
/// Serves the configured GraphQL IDE (`GraphiQL` or Apollo Sandbox).
pub async fn playground_handler(State(state): State<PlaygroundState>) -> impl IntoResponse {
    let html = match state.tool {
        PlaygroundTool::GraphiQL => graphiql_html(&state.graphql_endpoint),
        PlaygroundTool::ApolloSandbox => apollo_sandbox_html(&state.graphql_endpoint),
    };
    ([(axum::http::header::CONTENT_SECURITY_POLICY, PLAYGROUND_CSP)], Html(html))
}

/// Generate `GraphiQL` HTML page.
pub(crate) fn graphiql_html(endpoint: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>FraiseQL - GraphiQL</title>
    <style>
        body {{
            height: 100%;
            margin: 0;
            width: 100%;
            overflow: hidden;
        }}
        #graphiql {{
            height: 100vh;
        }}
    </style>
    <link rel="stylesheet" href="https://unpkg.com/graphiql/graphiql.min.css" />
</head>
<body>
    <div id="graphiql">Loading...</div>
    <script
        crossorigin
        src="https://unpkg.com/react@18/umd/react.production.min.js"
    ></script>
    <script
        crossorigin
        src="https://unpkg.com/react-dom@18/umd/react-dom.production.min.js"
    ></script>
    <script
        crossorigin
        src="https://unpkg.com/graphiql/graphiql.min.js"
    ></script>
    <script>
        const fetcher = GraphiQL.createFetcher({{
            url: '{endpoint}',
        }});
        ReactDOM.createRoot(document.getElementById('graphiql')).render(
            React.createElement(GraphiQL, {{
                fetcher,
                defaultEditorToolsVisibility: true,
            }})
        );
    </script>
</body>
</html>"#
    )
}

/// Generate Apollo Sandbox HTML page.
pub(crate) fn apollo_sandbox_html(endpoint: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>FraiseQL - Apollo Sandbox</title>
    <style>
        body {{
            height: 100vh;
            margin: 0;
            width: 100%;
            overflow: hidden;
        }}
        #sandbox {{
            height: 100%;
            width: 100%;
        }}
    </style>
</head>
<body>
    <div id="sandbox">Loading Apollo Sandbox...</div>
    <script src="https://embeddable-sandbox.cdn.apollographql.com/_latest/embeddable-sandbox.umd.production.min.js"></script>
    <script>
        new window.EmbeddedSandbox({{
            target: '#sandbox',
            initialEndpoint: window.location.origin + '{endpoint}',
            includeCookies: false,
        }});
    </script>
</body>
</html>"#
    )
}
