wit_bindgen::generate!({
    path: "wit/fraiseql-host.wit",
    world: "fraiseql-function",
});

struct GuestIdentity;

impl Guest for GuestIdentity {
    fn handle(event_json: String) -> Result<String, String> {
        // Identity: return input unchanged
        Ok(event_json)
    }
}

export!(GuestIdentity);
