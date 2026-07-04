#![allow(clippy::panic)] // Reason: test code, panics acceptable

use std::collections::BTreeMap;

use super::{Classification, IngestSource, classify, derive_thread_key, normalize_email};

fn received() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-03T12:00:00Z")
        .expect("valid timestamp")
        .with_timezone(&chrono::Utc)
}

fn normalize(raw: &[u8]) -> super::ParsedEmail {
    normalize_email(raw, IngestSource::Email, received()).expect("email normalizes")
}

/// A plain human reply with all the ordinary fields.
const SIMPLE: &[u8] = b"\
From: Alice Sender <alice@example.com>\r\n\
To: support+ticket-42@fraise.app\r\n\
Cc: watcher@example.com\r\n\
Subject: Re: your proposal\r\n\
Message-ID: <reply-1@example.com>\r\n\
Date: Fri, 03 Jul 2026 12:00:00 +0000\r\n\
\r\n\
Sounds good, let's proceed.\r\n";

#[test]
fn normalizes_headers_bodies_and_addresses() {
    let parsed = normalize(SIMPLE);
    let message = &parsed.message;

    assert_eq!(message.source, IngestSource::Email);
    assert_eq!(message.idempotency_key, "reply-1@example.com");
    assert_eq!(message.from.as_deref(), Some("alice@example.com"));
    // Display names are stripped so the addresses stay routable.
    assert_eq!(message.to, vec!["support+ticket-42@fraise.app".to_string()]);
    assert_eq!(message.cc, vec!["watcher@example.com".to_string()]);
    assert_eq!(message.subject.as_deref(), Some("Re: your proposal"));
    assert_eq!(message.body_text.as_deref(), Some("Sounds good, let's proceed.\r\n"));
    assert_eq!(message.received_at, received());
    // Curated headers are captured, lower-cased.
    assert_eq!(
        message.headers.get("message-id").map(String::as_str),
        Some("<reply-1@example.com>")
    );
    assert_eq!(message.classification, Some(Classification::Human));
}

#[test]
fn missing_message_id_falls_back_to_a_stable_content_hash() {
    let raw = b"From: a@b.com\r\nSubject: no id\r\n\r\nbody\r\n";
    let first = normalize(raw);
    let second = normalize(raw);
    assert!(first.message.idempotency_key.starts_with("sha256:"));
    // Deterministic: a re-fetch of the same bytes deduplicates.
    assert_eq!(first.message.idempotency_key, second.message.idempotency_key);
}

#[test]
fn parses_multipart_alternative_text_and_html() {
    let raw = b"\
From: a@b.com\r\n\
Subject: newsletter\r\n\
Message-ID: <mp-1@b.com>\r\n\
Content-Type: multipart/alternative; boundary=\"BOUND\"\r\n\
\r\n\
--BOUND\r\n\
Content-Type: text/plain\r\n\
\r\n\
plain body\r\n\
--BOUND\r\n\
Content-Type: text/html\r\n\
\r\n\
<p>html body</p>\r\n\
--BOUND--\r\n";
    let parsed = normalize(raw);
    assert_eq!(parsed.message.body_text.as_deref(), Some("plain body"));
    assert!(parsed.message.body_html.as_deref().unwrap_or_default().contains("html body"));
}

#[test]
fn extracts_attachments_as_pending_with_deferred_storage() {
    let raw = b"\
From: a@b.com\r\n\
Subject: invoice\r\n\
Message-ID: <att-1@b.com>\r\n\
Content-Type: multipart/mixed; boundary=\"BOUND\"\r\n\
\r\n\
--BOUND\r\n\
Content-Type: text/plain\r\n\
\r\n\
see attached\r\n\
--BOUND\r\n\
Content-Type: application/pdf; name=\"invoice.pdf\"\r\n\
Content-Disposition: attachment; filename=\"invoice.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
SGVsbG8gUERG\r\n\
--BOUND--\r\n";
    let parsed = normalize(raw);
    // The message carries no storage refs yet — the caller streams the bytes.
    assert!(parsed.message.attachments.is_empty());
    assert_eq!(parsed.attachments.len(), 1);
    let attachment = &parsed.attachments[0];
    assert_eq!(attachment.filename, "invoice.pdf");
    assert_eq!(attachment.content_type, "application/pdf");
    // base64 "SGVsbG8gUERG" decodes to "Hello PDF".
    assert_eq!(attachment.bytes, b"Hello PDF");
}

#[test]
fn body_text_is_present_alongside_attachment() {
    let raw = b"\
From: a@b.com\r\nSubject: s\r\nMessage-ID: <x@b.com>\r\n\
Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nhello\r\n\
--B\r\nContent-Type: text/plain; name=\"note.txt\"\r\n\
Content-Disposition: attachment; filename=\"note.txt\"\r\n\r\nattached text\r\n--B--\r\n";
    let parsed = normalize(raw);
    assert_eq!(parsed.message.body_text.as_deref(), Some("hello"));
    assert_eq!(parsed.attachments.len(), 1);
    assert_eq!(parsed.attachments[0].filename, "note.txt");
}

// ── Threading ────────────────────────────────────────────────────────────────

#[test]
fn thread_key_is_references_root_for_a_reply_chain() {
    // Ids arrive already stripped of angle brackets from the parser; the root is
    // the first (oldest) entry of References, not the newest ancestor.
    let references = vec!["root@x.com".to_string(), "mid@x.com".to_string()];
    let key = derive_thread_key(Some("newest@x.com"), &["mid@x.com".to_string()], &references);
    assert_eq!(key.as_deref(), Some("root@x.com"));
}

#[test]
fn thread_key_falls_back_to_in_reply_to_then_own_id() {
    // No references → the In-Reply-To parent is the root proxy.
    assert_eq!(
        derive_thread_key(Some("self@x"), &["parent@x".to_string()], &[]).as_deref(),
        Some("parent@x")
    );
    // No references and no In-Reply-To → a fresh thread keyed on the own id.
    assert_eq!(derive_thread_key(Some("self@x"), &[], &[]).as_deref(), Some("self@x"));
    // Nothing at all → no thread key.
    assert_eq!(derive_thread_key(None, &[], &[]), None);
}

#[test]
fn reply_chain_messages_collapse_to_one_thread_key() {
    let original = b"\
From: a@x.com\r\nTo: b@x.com\r\nSubject: hi\r\n\
Message-ID: <root@x.com>\r\n\r\nfirst\r\n";
    let reply = b"\
From: b@x.com\r\nTo: a@x.com\r\nSubject: Re: hi\r\n\
Message-ID: <reply@x.com>\r\n\
In-Reply-To: <root@x.com>\r\n\
References: <root@x.com>\r\n\r\nsecond\r\n";
    let first = normalize(original).message;
    let second = normalize(reply).message;
    assert_eq!(first.thread_key.as_deref(), Some("root@x.com"));
    assert_eq!(second.thread_key.as_deref(), Some("root@x.com"));
    assert_eq!(first.thread_key, second.thread_key);
}

// ── Classification ───────────────────────────────────────────────────────────

fn headers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

#[test]
fn human_reply_classifies_as_human() {
    assert_eq!(
        classify(&headers(&[]), Some("real@person.com"), false, Some("Re: hi")),
        Classification::Human
    );
}

#[test]
fn delivery_status_report_classifies_as_bounce() {
    assert_eq!(
        classify(&headers(&[]), Some("MAILER-DAEMON@mx.example.com"), true, None),
        Classification::Bounce
    );
}

#[test]
fn mailer_daemon_and_failed_recipients_classify_as_bounce() {
    assert_eq!(
        classify(&headers(&[]), Some("mailer-daemon@mx.com"), false, None),
        Classification::Bounce
    );
    assert_eq!(
        classify(&headers(&[("x-failed-recipients", "gone@x.com")]), Some("x@y.com"), false, None),
        Classification::Bounce
    );
}

#[test]
fn auto_replied_classifies_as_out_of_office() {
    assert_eq!(
        classify(
            &headers(&[("auto-submitted", "auto-replied")]),
            Some("ooo@x.com"),
            false,
            Some("Out of office")
        ),
        Classification::OutOfOffice
    );
    assert_eq!(
        classify(&headers(&[("precedence", "auto_reply")]), Some("ooo@x.com"), false, None),
        Classification::OutOfOffice
    );
}

#[test]
fn auto_generated_and_lists_classify_as_auto_generated() {
    // Auto-Submitted with parameters still resolves the keyword.
    assert_eq!(
        classify(
            &headers(&[("auto-submitted", "auto-generated; foo")]),
            Some("noreply@x.com"),
            false,
            None
        ),
        Classification::AutoGenerated
    );
    assert_eq!(
        classify(&headers(&[("precedence", "bulk")]), Some("news@x.com"), false, None),
        Classification::AutoGenerated
    );
    assert_eq!(
        classify(&headers(&[("list-id", "<list.x.com>")]), Some("news@x.com"), false, None),
        Classification::AutoGenerated
    );
}

#[test]
fn auto_submitted_no_is_still_human() {
    assert_eq!(
        classify(&headers(&[("auto-submitted", "no")]), Some("real@x.com"), false, Some("Re: hi")),
        Classification::Human
    );
}

#[test]
fn challenge_needs_both_automated_and_a_confirm_subject() {
    // Automated + confirm phrase → challenge.
    assert_eq!(
        classify(
            &headers(&[("auto-submitted", "auto-generated")]),
            Some("challenge@x.com"),
            false,
            Some("Please confirm your message")
        ),
        Classification::Challenge
    );
    // A human asking to confirm is NOT a challenge (no automated signal).
    assert_eq!(
        classify(&headers(&[]), Some("real@x.com"), false, Some("Please confirm the meeting")),
        Classification::Human
    );
    // An explicit challenge header alone suffices.
    assert_eq!(
        classify(&headers(&[("x-challenge", "1")]), Some("cr@x.com"), false, None),
        Classification::Challenge
    );
}

#[test]
fn is_automated_guard_matches_classification() {
    assert!(!Classification::Human.is_automated());
    for class in [
        Classification::OutOfOffice,
        Classification::Bounce,
        Classification::Challenge,
        Classification::AutoGenerated,
    ] {
        assert!(class.is_automated());
    }
}

#[test]
fn dsn_multipart_report_normalizes_as_bounce() {
    let raw = b"\
From: MAILER-DAEMON@mx.example.com\r\n\
To: sender@example.com\r\n\
Subject: Undelivered Mail Returned to Sender\r\n\
Message-ID: <dsn-1@mx.example.com>\r\n\
Content-Type: multipart/report; report-type=delivery-status; boundary=\"B\"\r\n\
\r\n\
--B\r\n\
Content-Type: text/plain\r\n\
\r\n\
Delivery failed.\r\n\
--B\r\n\
Content-Type: message/delivery-status\r\n\
\r\n\
Final-Recipient: rfc822; gone@example.com\r\n\
Action: failed\r\n\
--B--\r\n";
    let parsed = normalize(raw);
    assert_eq!(parsed.message.classification, Some(Classification::Bounce));
}
