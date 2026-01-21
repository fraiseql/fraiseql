//! GraphQL playground routes (GraphiQL and Apollo Sandbox).

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

/// Playground HTTP handler.
///
/// Serves the configured GraphQL IDE (GraphiQL or Apollo Sandbox).
pub async fn playground_handler(State(state): State<PlaygroundState>) -> impl IntoResponse {
    let html = match state.tool {
        PlaygroundTool::GraphiQL => graphiql_html(&state.graphql_endpoint),
        PlaygroundTool::ApolloSandbox => apollo_sandbox_html(&state.graphql_endpoint),
    };
    Html(html)
}

/// Generate GraphiQL HTML page.
fn graphiql_html(endpoint: &str) -> String {
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
fn apollo_sandbox_html(endpoint: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphiql_html_contains_endpoint() {
        let html = graphiql_html("/graphql");
        assert!(html.contains("/graphql"));
        assert!(html.contains("GraphiQL"));
        assert!(html.contains("graphiql.min.js"));
    }

    #[test]
    fn test_apollo_sandbox_html_contains_endpoint() {
        let html = apollo_sandbox_html("/graphql");
        assert!(html.contains("/graphql"));
        assert!(html.contains("EmbeddedSandbox"));
        assert!(html.contains("embeddable-sandbox.umd.production.min.js"));
    }

    #[test]
    fn test_playground_state_new() {
        let state = PlaygroundState::new("/graphql", PlaygroundTool::ApolloSandbox);
        assert_eq!(state.graphql_endpoint, "/graphql");
        assert_eq!(state.tool, PlaygroundTool::ApolloSandbox);
    }
}
