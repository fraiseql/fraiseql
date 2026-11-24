//! The usage period a mutation is counted under: the current UTC year and month as
//! `YYYY-MM`, computed here from the system clock with no date library.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tracing_subscriber::{Registry, layer::SubscriberExt as _};

use super::MutationAuditLayer;
use crate::usage::aggregator::UsageAggregator;

/// The proleptic Gregorian year and month of a day count from 1970-01-01.
fn year_month(days: i64) -> (i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month)
}

fn period_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is past 1970")
        .as_secs();
    let (year, month) = year_month(i64::try_from(seconds / 86_400).expect("days fit"));
    format!("{year:04}-{month:02}")
}

#[test]
fn the_year_month_arithmetic_is_the_gregorian_calendar() {
    assert_eq!(year_month(0), (1970, 1));
    assert_eq!(year_month(19_737), (2024, 1));
    assert_eq!(year_month(19_782), (2024, 2));
    assert_eq!(year_month(-1), (1969, 12));
}

#[test]
fn a_mutation_is_counted_under_the_utc_year_and_month() {
    let aggregator = Arc::new(UsageAggregator::new());
    let subscriber = Registry::default().with(MutationAuditLayer::new(Arc::clone(&aggregator)));
    let _guard = fraiseql_test_support::tracing_capture::set_default(subscriber);

    let before = period_now();
    tracing::info!(
        target: "fraiseql::mutation_audit",
        mutation_name = "create_user",
        entity_type = %"User",
        operation = %"create",
        tenant_id = %"acme",
        "mutation.executed"
    );
    let after = period_now();

    let counted = [before, after]
        .into_iter()
        .any(|period| aggregator.query("acme", &period).mutations.get("User") == Some(&1));
    assert!(counted, "the mutation is counted under {}", period_now());
}
