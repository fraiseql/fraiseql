//! Opaque resumption ids for `/{resource}/stream` (ruling AA 5).
//!
//! A stream's `id:` used to be the Change-Spine `seq`: a server-wide position, counting
//! every change of every type and tenant, including the ones this stream withholds — so the
//! gap between two ids a client received measured what it was not shown. The id is now the
//! position sealed with XChaCha20-Poly1305 under a key this process draws at start, with the
//! stream's type as associated data: opaque to the client, bound to its stream, and still
//! the exact change-log position a resume reads back from (#1310).
//!
//! A token that does not open — another stream's, another process's, altered — names
//! nothing this stream can resume from, and is refused as an unknown resume point is
//! (`410`): never answered from now on, which would be a gap the client cannot see.

use std::sync::OnceLock;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use rand::RngCore;

const NONCE_LEN: usize = 24;

fn cipher() -> &'static XChaCha20Poly1305 {
    static CIPHER: OnceLock<XChaCha20Poly1305> = OnceLock::new();
    CIPHER.get_or_init(|| {
        let mut key = [0_u8; 32];
        rand::rng().fill_bytes(&mut key);
        XChaCha20Poly1305::new(&key.into())
    })
}

/// Seal `position` as the id of a frame on `stream`'s stream. `None` only if the cipher
/// fails, which it does not for an eight-byte message.
#[must_use]
pub fn seal(position: i64, stream: &str) -> Option<String> {
    let mut nonce = [0_u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let sealed = cipher()
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &position.to_be_bytes(),
                aad: stream.as_bytes(),
            },
        )
        .ok()?;
    let mut token = nonce.to_vec();
    token.extend_from_slice(&sealed);
    Some(URL_SAFE_NO_PAD.encode(token))
}

/// The position `token` seals, if this process sealed it for `stream`.
#[must_use]
pub fn open(token: &str, stream: &str) -> Option<i64> {
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    if bytes.len() <= NONCE_LEN {
        return None;
    }
    let (nonce, sealed) = bytes.split_at(NONCE_LEN);
    let position = cipher()
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad: stream.as_bytes(),
            },
        )
        .ok()?;
    Some(i64::from_be_bytes(position.try_into().ok()?))
}
