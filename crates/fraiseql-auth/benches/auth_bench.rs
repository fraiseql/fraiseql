#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items

//! Performance benchmarks for fraiseql-auth hot paths
//!
//! Measures latency for:
//! - JWT token generation and HMAC validation
//! - PKCE S256 challenge computation
//! - Constant-time comparison (equal, unequal, padded)
//! - Rate limiter check (allow and reject paths)

use std::collections::HashMap;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_auth::{
    Claims, ConstantTimeOps, KeyedRateLimiter, jwt::generate_hs256_token, pkce::PkceStateStore,
    rate_limiting::AuthRateLimitConfig,
};

// ---------------------------------------------------------------------------
// JWT benchmarks
// ---------------------------------------------------------------------------

fn jwt_benchmarks(c: &mut Criterion) {
    let secret = b"bench_secret_key_at_least_32_bytes_long!";

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        sub:   "user-bench-42".to_string(),
        iat:   now,
        exp:   now + 3600,
        iss:   "https://bench.fraiseql.dev".to_string(),
        aud:   vec!["api".to_string()],
        extra: HashMap::new(),
    };

    c.bench_function("jwt_generate_hs256", |b| {
        b.iter(|| {
            generate_hs256_token(black_box(&claims), black_box(secret)).unwrap();
        });
    });

    // Pre-generate a token for validation benchmarks
    let token = generate_hs256_token(&claims, secret).unwrap();

    let validator = fraiseql_auth::JwtValidator::new(
        "https://bench.fraiseql.dev",
        jsonwebtoken::Algorithm::HS256,
    )
    .unwrap()
    .with_audiences(&["api"])
    .unwrap();

    c.bench_function("jwt_validate_hmac", |b| {
        b.iter(|| {
            validator.validate_hmac(black_box(&token), black_box(secret)).unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// PKCE benchmarks
// ---------------------------------------------------------------------------

fn pkce_benchmarks(c: &mut Criterion) {
    // s256_challenge is the hot-path crypto operation (SHA-256 + base64url)
    let verifiers: Vec<(usize, String)> = vec![
        (43, "a".repeat(43)),   // minimum RFC 7636 length
        (64, "b".repeat(64)),   // typical generated length
        (128, "c".repeat(128)), // maximum RFC 7636 length
    ];

    let mut group = c.benchmark_group("pkce_s256_challenge");
    for (len, verifier) in &verifiers {
        group.bench_with_input(BenchmarkId::from_parameter(len), verifier, |b, v| {
            b.iter(|| PkceStateStore::s256_challenge(black_box(v)));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Constant-time comparison benchmarks
// ---------------------------------------------------------------------------

fn constant_time_benchmarks(c: &mut Criterion) {
    let token_64 = "a]b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6A7B8C9D0E1F2".as_bytes();

    // Equal slices — the success path
    c.bench_function("ct_compare_equal_64B", |b| {
        b.iter(|| ConstantTimeOps::compare(black_box(token_64), black_box(token_64)));
    });

    // Unequal slices (first byte differs) — must take same time as equal
    let mut unequal = token_64.to_vec();
    unequal[0] ^= 0xFF;
    c.bench_function("ct_compare_unequal_64B", |b| {
        b.iter(|| ConstantTimeOps::compare(black_box(token_64), black_box(&unequal)));
    });

    // String comparison (typical JWT-sized strings)
    let jwt_a = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIiwiaWF0IjoxNzA5MDAwMDAwLCJleHAiOjE3MDkwMDM2MDB9.signature_placeholder";
    let jwt_b = jwt_a;
    c.bench_function("ct_compare_str_jwt_size", |b| {
        b.iter(|| ConstantTimeOps::compare_str(black_box(jwt_a), black_box(jwt_b)));
    });

    // Padded comparison at fixed 512 bytes (JWT constant-time path)
    let short = b"short_token";
    let long = b"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature";
    c.bench_function("ct_compare_padded_512", |b| {
        b.iter(|| ConstantTimeOps::compare_padded(black_box(short), black_box(long), 512));
    });

    c.bench_function("ct_compare_jwt_constant", |b| {
        b.iter(|| ConstantTimeOps::compare_jwt_constant(black_box(jwt_a), black_box(jwt_b)));
    });
}

// ---------------------------------------------------------------------------
// Rate limiter benchmarks
// ---------------------------------------------------------------------------

fn rate_limiter_benchmarks(c: &mut Criterion) {
    // Use a deterministic clock to keep benchmarks stable
    let config_allow = AuthRateLimitConfig {
        enabled:      true,
        max_requests: 1_000_000, // effectively unlimited for the benchmark
        window_secs:  60,
    };

    let limiter_allow = KeyedRateLimiter::with_clock(config_allow, || 1_000);

    c.bench_function("rate_limiter_check_allow", |b| {
        b.iter(|| {
            limiter_allow.check(black_box("192.168.1.1")).unwrap();
        });
    });

    // Benchmark the reject path: fill the limiter first, then measure rejection speed
    let config_reject = AuthRateLimitConfig {
        enabled:      true,
        max_requests: 1,
        window_secs:  3600,
    };
    let limiter_reject = KeyedRateLimiter::with_clock(config_reject, || 1_000);
    limiter_reject.check("192.168.1.1").unwrap(); // consume the single allowed request

    c.bench_function("rate_limiter_check_reject", |b| {
        b.iter(|| {
            let _ = limiter_reject.check(black_box("192.168.1.1"));
        });
    });

    // Benchmark with many distinct keys to stress the HashMap
    let config_many = AuthRateLimitConfig {
        enabled:      true,
        max_requests: 1_000_000,
        window_secs:  60,
    };
    let limiter_many = KeyedRateLimiter::with_clock(config_many, || 1_000);

    // Pre-populate 10k keys
    for i in 0..10_000 {
        let key = format!("10.0.{}.{}", i / 256, i % 256);
        limiter_many.check(&key).unwrap();
    }

    c.bench_function("rate_limiter_check_10k_keys", |b| {
        b.iter(|| {
            limiter_many.check(black_box("10.0.39.16")).unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    jwt_benchmarks,
    pkce_benchmarks,
    constant_time_benchmarks,
    rate_limiter_benchmarks
);
criterion_main!(benches);
