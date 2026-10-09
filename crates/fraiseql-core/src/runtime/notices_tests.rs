#![allow(clippy::unwrap_used)] // Reason: test code

use serde_json::json;

use super::*;

fn notice() -> ResponseNotice {
    ResponseNotice {
        path:   vec!["docs".to_string()],
        kind:   POSSIBLY_TRUNCATED,
        detail: json!({ "requested": 10, "returned": 3, "verified": false }),
    }
}

#[tokio::test]
async fn a_notice_pushed_in_scope_is_collected_and_outside_is_dropped() {
    push_notice(notice());
    let ((), notices) = collect_notices(async { push_notice(notice()) }).await;
    assert_eq!(notices, vec![notice()]);
}

/// A short result no count could verify (a read with nested relations has no counting
/// statement) is served with an unverified notice under every policy: `refuse` refuses
/// only what was counted.
#[test]
fn an_uncounted_short_result_is_never_refused() {
    for policy in [
        ShortResultPolicy::Signal,
        ShortResultPolicy::Verify,
        ShortResultPolicy::Refuse,
    ] {
        let ((), notices) = collected(|| {
            settle_short_nearest(policy, "docs", 10, 3, None).expect("served");
        });
        assert_eq!(notices, vec![notice()], "{policy:?}");
    }
}

/// A counted truncation: a verified notice under `verify`, a refusal under `refuse`; a
/// full page is never noticed.
#[test]
fn a_counted_truncation_is_verified_or_refused() {
    let ((), notices) = collected(|| {
        settle_short_nearest(ShortResultPolicy::Verify, "docs", 10, 3, Some(10))
            .expect("verify serves");
    });
    assert_eq!(notices[0].detail["verified"], json!(true));

    let refused = settle_short_nearest(ShortResultPolicy::Refuse, "docs", 10, 3, Some(10));
    assert!(matches!(refused, Err(crate::error::FraiseQLError::Unsupported { .. })));

    let ((), notices) = collected(|| {
        settle_short_nearest(ShortResultPolicy::Signal, "docs", 10, 10, None)
            .expect("a full page is served");
    });
    assert!(notices.is_empty());
}

/// Run `f` inside a notice scope on a fresh current-thread runtime.
fn collected(f: impl FnOnce()) -> ((), Vec<ResponseNotice>) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime")
        .block_on(collect_notices(async move { f() }))
}

#[test]
fn notices_land_under_extensions_and_none_leave_the_response_alone() {
    let served = json!({ "data": { "docs": [] } });
    assert_eq!(with_notices(served.clone(), &[]), served);
    let noticed = with_notices(served, &[notice()]);
    assert_eq!(noticed["extensions"]["notices"][0]["kind"], json!(POSSIBLY_TRUNCATED));
    assert_eq!(noticed["data"], json!({ "docs": [] }));
}
