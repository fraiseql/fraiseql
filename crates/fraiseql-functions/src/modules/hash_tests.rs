#![allow(clippy::unwrap_used)] // Reason: test module

use std::collections::BTreeMap;

use super::function_hash;
use crate::modules::resolve::{ModuleKey, resolve_specifier};

fn key(path: &str) -> ModuleKey {
    resolve_specifier(&format!("@shared/{path}")).unwrap()
}

fn modules(entries: &[(&str, &str)]) -> BTreeMap<ModuleKey, String> {
    entries
        .iter()
        .map(|(path, source)| (key(path), (*source).to_string()))
        .collect()
}

const ENTRY: &[u8] = b"import { x } from \"@shared/a.ts\";\nexport default () => x;\n";

#[test]
fn the_hash_is_stable() {
    let set = modules(&[("a.ts", "export const x = 1;\n")]);
    assert_eq!(function_hash(ENTRY, &set), function_hash(ENTRY, &set));
    assert_eq!(function_hash(ENTRY, &set).len(), 64);
}

#[test]
fn a_changed_module_changes_the_hash() {
    let before = modules(&[("a.ts", "export const x = 1;\n")]);
    let after = modules(&[("a.ts", "export const x = 2;\n")]);
    assert_ne!(function_hash(ENTRY, &before), function_hash(ENTRY, &after));
}

#[test]
fn a_changed_entry_changes_the_hash() {
    let set = modules(&[("a.ts", "export const x = 1;\n")]);
    assert_ne!(function_hash(ENTRY, &set), function_hash(b"export default () => 2;\n", &set));
}

#[test]
fn an_added_module_changes_the_hash() {
    let one = modules(&[("a.ts", "export const x = 1;\n")]);
    let two = modules(&[("a.ts", "export const x = 1;\n"), ("b.ts", "")]);
    assert_ne!(function_hash(ENTRY, &one), function_hash(ENTRY, &two));
}

/// A module moved to another key, with the same bytes, is a different set.
#[test]
fn a_renamed_module_changes_the_hash() {
    let before = modules(&[("a.ts", "export const x = 1;\n")]);
    let after = modules(&[("b.ts", "export const x = 1;\n")]);
    assert_ne!(function_hash(ENTRY, &before), function_hash(ENTRY, &after));
}

/// Bytes moved across the key/content boundary make a different set. Unprefixed, both
/// sets concatenate to `a.tsb.tsQ`; the length prefixes are what tell them apart.
#[test]
fn moving_bytes_between_key_and_content_changes_the_hash() {
    let one = modules(&[("a.ts", "b.tsQ")]);
    let two = modules(&[("a.tsb.ts", "Q")]);
    assert_ne!(function_hash(ENTRY, &one), function_hash(ENTRY, &two));
}

/// Bytes moved from one module's content into the next module's key, with the same
/// number of modules: unprefixed, both concatenate to `a.tsb.tsc.ts`.
#[test]
fn moving_bytes_between_content_and_the_next_key_changes_the_hash() {
    let one = modules(&[("a.ts", "b.ts"), ("c.ts", "")]);
    let two = modules(&[("a.ts", ""), ("b.tsc.ts", "")]);
    assert_ne!(function_hash(ENTRY, &one), function_hash(ENTRY, &two));
}

/// An empty module still counts: it is imported, and its presence is part of the set.
#[test]
fn an_empty_module_is_not_an_absent_one() {
    let none = modules(&[]);
    let empty = modules(&[("a.ts", "")]);
    assert_ne!(function_hash(ENTRY, &none), function_hash(ENTRY, &empty));
}

#[test]
fn the_hash_does_not_depend_on_insertion_order() {
    let mut forward = BTreeMap::new();
    forward.insert(key("a.ts"), "1".to_string());
    forward.insert(key("b.ts"), "2".to_string());
    let mut backward = BTreeMap::new();
    backward.insert(key("b.ts"), "2".to_string());
    backward.insert(key("a.ts"), "1".to_string());
    assert_eq!(function_hash(ENTRY, &forward), function_hash(ENTRY, &backward));
}

/// The domain tag is part of the hash: the same bytes hashed as anything else differ.
#[test]
fn the_hash_is_not_the_plain_digest_of_the_entry() {
    use sha2::{Digest, Sha256};
    let plain = hex::encode(Sha256::digest(ENTRY));
    assert_ne!(function_hash(ENTRY, &modules(&[])), plain);
}

/// The encoding, pinned: a change to it changes every function's hash, so it is a
/// deliberate act that bumps the version in the domain tag, never a side effect.
#[test]
fn the_encoding_is_pinned() {
    let set = modules(&[("a.ts", "export const x = 1;\n")]);
    assert_eq!(function_hash(b"export default () => 1;\n", &set), GOLDEN);
}

/// Computed independently from the spec (Python `hashlib`, `struct.pack("<Q", len)`).
const GOLDEN: &str = "7903dd062d652531e661560574da4a5f16ca89551af30dc09785fab517faa1c2";
