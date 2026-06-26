//! Render a subgraph's Apollo-Federation `_service { sdl }` from its compiled schema,
//! without starting a server. Mirrors the server's path:
//! `generate_service_sdl(raw_schema(), federation_metadata())`.
//!
//! Usage: cargo run --example fed_sdl --features federation -- <schema.compiled.json>

use std::fs;

use fraiseql_core::CompiledSchema;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: fed_sdl <schema.compiled.json>");
    let json = fs::read_to_string(&path).expect("read compiled schema");
    let schema = CompiledSchema::from_json(&json, false).expect("parse compiled schema");
    let meta = schema
        .federation_metadata()
        .expect("schema has no enabled federation block");
    let raw = schema.raw_schema();
    let sdl = fraiseql_core::federation::generate_service_sdl(&raw, &meta);
    println!("{sdl}");
}
