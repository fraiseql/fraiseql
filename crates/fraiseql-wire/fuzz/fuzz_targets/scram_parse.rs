#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Fuzz SCRAM-SHA-256 message parsing via client_final (processes server-first messages)
    let mut client =
        fraiseql_wire::auth::ScramClient::new("fuzzuser".to_string(), "fuzzpass".to_string());
    let _client_first = client.client_first();

    // Feed arbitrary data as a server-first message
    let result = client.client_final(data);

    match result {
        Ok((client_final, state)) => {
            // Client final message must follow RFC 5802 format: c=...,r=...,p=...
            assert!(
                client_final.starts_with("c="),
                "Client final message doesn't start with 'c=': {client_final}"
            );
            assert!(
                client_final.contains(",p="),
                "Client final message missing proof: {client_final}"
            );

            // Also fuzz verify_server_final with arbitrary data
            let _ = client.verify_server_final(data, &state);
        }
        Err(e) => {
            // Error messages must be non-empty
            let msg = e.to_string();
            assert!(!msg.is_empty(), "SCRAM error produced empty message");
        }
    }
});
