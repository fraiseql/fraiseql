wit_bindgen::generate!({
    path: "wit",
    world: "fraiseql-function",
});

struct GuestImpl;

impl Guest for GuestImpl {
    fn handle(event_json: String) -> Result<String, String> {
        // Identity: return the event JSON unchanged
        Ok(event_json)
    }
}

export!(GuestImpl);
