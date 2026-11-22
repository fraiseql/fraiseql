#![no_main]
//! A raw email as a mailbox delivers it, through `normalize_email`: MIME parsing, the
//! threading headers, addresses, bodies, attachments and the classification that drives
//! bounce and challenge handling. Then every address through `parse_recipient`, the
//! plus-tag routing that turns `support+ticket-42@…` into an entity id.
//!
//! Beyond "no panic":
//! - Normalization is deterministic: the same bytes normalize to the same message.
//! - The idempotency key ends with the SHA-256 of the raw bytes, so two different messages never
//!   share a key, whatever `Message-ID` a sender chose.
//! - A recipient decomposes losslessly: its plus-tag put back before the domain gives the address
//!   it came from, and an address with no tag is its own base.

use fraiseql_functions::{IngestSource, normalize_email, parse_recipient};
use libfuzzer_sys::fuzz_target;
use sha2::{Digest as _, Sha256};

fuzz_target!(|data: &[u8]| {
    let received_at = fraiseql_time::Timestamp::from_second(1_700_000_000).expect("timestamp");
    let source = || IngestSource::Email {
        mailbox: "fuzz".to_string(),
    };
    let Ok(parsed) = normalize_email(data, source(), received_at) else {
        return;
    };

    assert_eq!(
        normalize_email(data, source(), received_at).ok().as_ref(),
        Some(&parsed),
        "the same bytes normalized to a different message"
    );

    let digest = format!("sha256:{}", hex::encode(Sha256::digest(data)));
    assert!(
        parsed.message.idempotency_key.ends_with(&digest),
        "the idempotency key does not bind the message's bytes: {:?}",
        parsed.message.idempotency_key
    );

    let message = &parsed.message;
    for address in message.from.iter().chain(&message.to).chain(&message.cc) {
        let Some(recipient) = parse_recipient(address) else {
            continue;
        };
        let rebuilt = match &recipient.tag {
            Some(tag) => {
                let (local, domain) =
                    recipient.base.rsplit_once('@').expect("a recipient base has a domain");
                format!("{local}+{tag}@{domain}")
            },
            None => recipient.base.clone(),
        };
        assert_eq!(&rebuilt, address, "a recipient does not decompose losslessly");
    }
});
