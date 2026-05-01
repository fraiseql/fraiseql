wit_bindgen::generate!({
    path: "wit",
    world: "fraiseql-function",
});

struct GuestImpl;

impl Guest for GuestImpl {
    fn handle(event_json: String) -> Result<String, String> {
        // Insert "transformed":true before the final closing brace.
        // EventPayload JSON ends with a timestamp string (which ends in `"`),
        // so the very last `}` is the outer object's closing brace.
        let trimmed = event_json.trim_end();
        match trimmed.rfind('}') {
            Some(idx) => {
                let (before, after) = trimmed.split_at(idx);
                Ok(format!("{},\"transformed\":true{}", before, after))
            }
            None => Ok(event_json),
        }
    }
}

export!(GuestImpl);
