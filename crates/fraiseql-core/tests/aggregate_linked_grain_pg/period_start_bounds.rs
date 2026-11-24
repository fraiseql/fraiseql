//! A range bounded on a period's first day, exclusively, starts the whole periods after
//! it: `day_gt: 2026-02-01` leaves 1 February to the edges, so February is read from the
//! finer table and the month table answers from March.
//!
//! **Parallelism:** reads the parent's `tf_lg_*` fixtures.

use serde_json::json;

use super::{DAY, MONTH, answers_as_fine, executor, request};

#[tokio::test]
async fn a_range_after_a_periods_first_day_reads_that_period_from_the_edges() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for range in [
        json!({ "day_gt": "2026-02-01" }),
        json!({ "day_gt": "2026-02-01", "day_lte": "2026-04-30" }),
        json!({ "day_gt": "2026-01-31", "day_lt": "2026-05-01" }),
    ] {
        for group in [&[][..], &["day_month"]] {
            let request = request(&range, group, &["amount_sum", "qty_sum"]);
            answers_as_fine(&executor, MONTH, DAY, &request).await;
        }
    }
}
