#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Enforce MAX_FLIGHT_TICKET_BYTES (256 KiB)
    if data.len() > 256 * 1024 {
        return;
    }

    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // FlightTicket deserialization must never panic
    match serde_json::from_str::<fraiseql_arrow::ticket::FlightTicket>(s) {
        Ok(ticket) => {
            // Roundtrip: serialize back
            let json = serde_json::to_string(&ticket);
            assert!(json.is_ok(), "FlightTicket must serialize back to JSON");
        }
        Err(_) => {}
    }
});
