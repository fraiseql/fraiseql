#[allow(non_snake_case)]
pub mod exports {
    pub mod handle {
        pub struct Guest;
        impl Guest {
            pub fn handle(event_json: String) -> Result<String, String> {
                // Parse JSON, add a transformed field, return new JSON
                let mut obj: serde_json::Value = serde_json::from_str(&event_json)
                    .map_err(|e| e.to_string())?;

                // Add a transformed marker field
                if let serde_json::Value::Object(ref mut map) = obj {
                    map.insert("transformed".to_string(), serde_json::Value::Bool(true));
                }

                serde_json::to_string(&obj).map_err(|e| e.to_string())
            }
        }
    }
}

// Note: In a real setup, wit-bindgen would generate proper Component Model
// bindings. For Cycle 5 testing, this stub allows compilation.
