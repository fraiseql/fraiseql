#![allow(clippy::unwrap_used)] // Reason: benchmark setup code
#![allow(clippy::cast_possible_truncation)] // Reason: i iterates 0..32, safely within u8
#![allow(missing_docs)]

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_secrets::encryption::{FieldEncryption, VersionedFieldEncryption};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = (i as u8).wrapping_mul(7).wrapping_add(42);
    }
    key
}

fn make_fallback_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = (i as u8).wrapping_mul(13).wrapping_add(99);
    }
    key
}

// ---------------------------------------------------------------------------
// FieldEncryption benchmarks
// ---------------------------------------------------------------------------

fn field_encryption_benchmarks(c: &mut Criterion) {
    let key = make_key();
    let fe = FieldEncryption::new(&key).unwrap();

    let payloads: Vec<(usize, String)> = vec![
        (16, "a_short_payload_!".to_string()),
        (256, "x".repeat(256)),
        (4096, "y".repeat(4096)),
        (65536, "z".repeat(65536)),
    ];

    {
        let mut group = c.benchmark_group("field_encrypt");
        for (size, plaintext) in &payloads {
            group.bench_with_input(BenchmarkId::new("bytes", size), plaintext, |b, pt| {
                b.iter(|| fe.encrypt(black_box(pt)).unwrap());
            });
        }
        group.finish();
    }

    {
        let mut group = c.benchmark_group("field_decrypt");
        for (size, plaintext) in &payloads {
            let ciphertext = fe.encrypt(plaintext).unwrap();
            group.bench_with_input(BenchmarkId::new("bytes", size), &ciphertext, |b, ct| {
                b.iter(|| fe.decrypt(black_box(ct)).unwrap());
            });
        }
        group.finish();
    }
}

fn field_encryption_with_context(c: &mut Criterion) {
    let key = make_key();
    let fe = FieldEncryption::new(&key).unwrap();
    let plaintext = "sensitive_field_value_for_user_42";
    let context = "users.email.42";

    c.bench_function("encrypt_with_context", |b| {
        b.iter(|| fe.encrypt_with_context(black_box(plaintext), black_box(context)).unwrap());
    });

    let ciphertext = fe.encrypt_with_context(plaintext, context).unwrap();
    c.bench_function("decrypt_with_context", |b| {
        b.iter(|| fe.decrypt_with_context(black_box(&ciphertext), black_box(context)).unwrap());
    });
}

// ---------------------------------------------------------------------------
// Versioned encryption benchmarks (key rotation)
// ---------------------------------------------------------------------------

fn versioned_encryption_benchmarks(c: &mut Criterion) {
    let primary_key = make_key();
    let fallback_key = make_fallback_key();

    let ve = VersionedFieldEncryption::new(2, &primary_key)
        .unwrap()
        .with_fallback(1, &fallback_key)
        .unwrap();

    let plaintext = "versioned_secret_data";

    c.bench_function("versioned_encrypt", |b| {
        b.iter(|| ve.encrypt(black_box(plaintext)).unwrap());
    });

    let current_ct = ve.encrypt(plaintext).unwrap();
    c.bench_function("versioned_decrypt_current", |b| {
        b.iter(|| ve.decrypt(black_box(&current_ct)).unwrap());
    });

    let old_ve = VersionedFieldEncryption::new(1, &fallback_key).unwrap();
    let old_ct = old_ve.encrypt(plaintext).unwrap();
    c.bench_function("versioned_decrypt_fallback", |b| {
        b.iter(|| ve.decrypt(black_box(&old_ct)).unwrap());
    });

    c.bench_function("versioned_reencrypt", |b| {
        b.iter(|| ve.reencrypt_from_fallback(black_box(&old_ct)).unwrap());
    });

    c.bench_function("versioned_extract_version", |b| {
        b.iter(|| VersionedFieldEncryption::extract_version(black_box(&current_ct)).unwrap());
    });
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    field_encryption_benchmarks,
    field_encryption_with_context,
    versioned_encryption_benchmarks,
);
criterion_main!(benches);
