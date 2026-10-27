//! Render a subgraph's Apollo-Federation `_service { sdl }` from its compiled schema,
//! without starting a server. Mirrors the server's path:
//! [`CompiledSchema::federation_service_sdl`].
//!
//! Usage: `cargo run --example fed_sdl --features federation -- <schema.compiled.json>`
#![allow(clippy::print_stdout)] // Printing the SDL to stdout is this tool's purpose.

use std::fs;

use fraiseql_core::CompiledSchema;

fn main() {
    let path = std::env::args().nth(1).expect("usage: fed_sdl <schema.compiled.json>");
    let json = fs::read_to_string(&path).expect("read compiled schema");
    let schema = CompiledSchema::from_json(&json, false).expect("parse compiled schema");
    let sdl = schema.federation_service_sdl().expect("schema has no enabled federation block");
    println!("{sdl}");
}
