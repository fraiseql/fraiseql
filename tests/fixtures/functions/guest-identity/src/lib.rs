wit_bindgen::generate!({
    path: "../../../../crates/fraiseql-functions/wit",
    world: "fraiseql-function",
});

use crate::exports::fraiseql::host::handle::Guest;

pub struct Component;

impl Guest for Component {
    fn handle(event_json: String) -> Result<String, String> {
        // Identity: return input unchanged
        Ok(event_json)
    }
}
