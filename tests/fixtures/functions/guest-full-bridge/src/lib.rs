wit_bindgen::generate!({
    path: "wit/fraiseql-host.wit",
    world: "fraiseql-function",
});

struct GuestFullBridge;

impl Guest for GuestFullBridge {
    fn handle(_event_json: String) -> Result<String, String> {
        use fraiseql::host::context;
        use fraiseql::host::io;
        use fraiseql::host::logging;

        let mut results = Vec::new();

        // 1. Log a message at each level
        logging::log(logging::LogLevel::Debug, "debug message");
        logging::log(logging::LogLevel::Info, "info message");
        logging::log(logging::LogLevel::Warn, "warn message");
        logging::log(logging::LogLevel::Error, "error message");
        results.push(r#""logging":"ok""#.to_string());

        // 2. Read event payload
        let payload = context::get_event_payload();
        let payload_ok = !payload.is_empty();
        results.push(format!(r#""event_payload":{}"#, payload_ok));

        // 3. Get auth context
        let auth_result = match context::get_auth_context() {
            Ok(json) => format!(r#""auth_context":{{"ok":true,"len":{}}}"#, json.len()),
            Err(e) => format!(r#""auth_context":{{"ok":false,"error":"{}"}}"#, e),
        };
        results.push(auth_result);

        // 4. Read an env var
        let env_result = match context::get_env_var("FRAISEQL_TEST_VAR") {
            Some(val) => format!(r#""env_var":{{"found":true,"value":"{}"}}"#, val),
            None => r#""env_var":{"found":false}"#.to_string(),
        };
        results.push(env_result);

        // 5. Make an HTTP request
        let http_result = match io::http_request("GET", "https://mock.test/api", &[], None) {
            Ok(resp) => format!(r#""http_request":{{"ok":true,"status":{}}}"#, resp.status),
            Err(e) => format!(r#""http_request":{{"ok":false,"error":"{}"}}"#, e),
        };
        results.push(http_result);

        // 6. Execute a GraphQL query
        let query_result = match io::query("{ users { id } }", "{}") {
            Ok(data) => format!(r#""query":{{"ok":true,"len":{}}}"#, data.len()),
            Err(e) => format!(r#""query":{{"ok":false,"error":"{}"}}"#, e),
        };
        results.push(query_result);

        // 7. Put a storage object
        let put_result = match io::storage_put("test-bucket", "test-key", b"hello world", "text/plain") {
            Ok(()) => r#""storage_put":{"ok":true}"#.to_string(),
            Err(e) => format!(r#""storage_put":{{"ok":false,"error":"{}"}}"#, e),
        };
        results.push(put_result);

        // 8. Get the storage object back
        let get_result = match io::storage_get("test-bucket", "test-key") {
            Ok(data) => format!(r#""storage_get":{{"ok":true,"len":{}}}"#, data.len()),
            Err(e) => format!(r#""storage_get":{{"ok":false,"error":"{}"}}"#, e),
        };
        results.push(get_result);

        // 9. Return JSON summary
        let json = format!("{{{}}}", results.join(","));
        Ok(json)
    }
}

export!(GuestFullBridge);
