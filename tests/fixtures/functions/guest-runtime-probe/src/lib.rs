//! Reports what the WASI surface a FraiseQL WASM function runs on lets it do.
//!
//! Each probe is attempted, never assumed, and its outcome returned as one JSON
//! object: `runtime_surface_tests` in `fraiseql-functions` pins the answers.

wit_bindgen::generate!({
    path: "wit/fraiseql-host.wit",
    world: "fraiseql-function",
});

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct GuestRuntimeProbe;

fn outcome<T, E: std::fmt::Display>(result: Result<T, E>) -> String {
    match result {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("err: {e}").replace('\\', "\\\\").replace('"', "'"),
    }
}

impl Guest for GuestRuntimeProbe {
    fn handle(_event_json: String) -> Result<String, String> {
        let env_vars = std::env::vars().count();
        let args = std::env::args().count();
        let read_root = outcome(std::fs::read_dir("/"));
        let write_file = outcome(std::fs::write("probe.txt", b"x"));
        let tcp = outcome(std::net::TcpStream::connect("127.0.0.1:9"));
        let wall_clock_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let started = Instant::now();
        let monotonic = Instant::now() >= started;
        // Two hashers seeded from WASI random: equal keys would mean a fixed seed.
        let (mut a, mut b) = (RandomState::new().build_hasher(), RandomState::new().build_hasher());
        a.write_u8(1);
        b.write_u8(1);
        let random_seeds_differ = a.finish() != b.finish();
        println!("stdout from a guest");
        Ok(format!(
            "{{\"env_vars\":{env_vars},\"args\":{args},\"read_root\":\"{read_root}\",\
             \"write_file\":\"{write_file}\",\"tcp\":\"{tcp}\",\
             \"wall_clock_secs\":{wall_clock_secs},\"monotonic\":{monotonic},\
             \"random_seeds_differ\":{random_seeds_differ},\"stdout\":\"ok\"}}"
        ))
    }
}

export!(GuestRuntimeProbe);
