#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::cache::feed::{Eviction, outbox};

use super::outgoing;

fn views() -> Vec<Eviction> {
    vec![Eviction::Views(vec!["v_user".to_string()])]
}

#[test]
fn a_batch_is_sent_as_announced() {
    let (_outbox, queue) = outbox(4);
    assert_eq!(outgoing(&queue, views()), views());
}

#[test]
fn after_a_lost_announcement_every_other_server_evicts_everything() {
    let (outbox, queue) = outbox(1);
    outbox.announce(Eviction::Views(vec!["v_post".to_string()]));
    outbox.announce(Eviction::Views(vec!["v_tag".to_string()]));
    assert_eq!(outgoing(&queue, views()), vec![Eviction::Everything], "v_tag was lost");
    assert_eq!(outgoing(&queue, views()), views(), "and said once");
}

#[test]
fn a_failed_send_is_made_up_for_by_the_next() {
    let (_outbox, queue) = outbox(4);
    queue.mark_lost();
    assert_eq!(outgoing(&queue, Vec::new()), vec![Eviction::Everything]);
}
