#![no_main]
//! An authoring IR (`schema.json`, what an SDK emits) through `fraiseql compile`: the
//! drifted-key and runtime-section checks on the raw document, deserialization, validation,
//! conversion and every refusal after it. Whatever compiles must load as a server loads it
//! (see `round_trip`). The JSON workflow reads no file but its input.

#[path = "../src/round_trip.rs"]
mod round_trip;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    round_trip::check("schema.json", data);
});
