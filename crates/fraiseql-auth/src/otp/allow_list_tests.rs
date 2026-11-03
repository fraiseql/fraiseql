use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Semaphore;

use super::*;

struct AdmitAll;

#[async_trait]
impl OtpAllowList for AdmitAll {
    async fn allows(&self, _email: &str) -> Result<bool> {
        Ok(true)
    }
}

/// Delivers one mail per permit, so a test can hold the relay stalled.
struct StalledRelay {
    permits:   Arc<Semaphore>,
    delivered: Arc<AtomicUsize>,
}

#[async_trait]
impl EmailDelivery for StalledRelay {
    async fn send_otp(&self, _email: &str, _issue: &OtpIssue) -> Result<String> {
        self.permits.acquire().await.expect("semaphore open").forget();
        self.delivered.fetch_add(1, Ordering::SeqCst);
        Ok(String::new())
    }
}

#[tokio::test]
async fn a_full_queue_drops_the_mail_and_never_blocks_the_request() {
    let permits = Arc::new(Semaphore::new(0));
    let delivered = Arc::new(AtomicUsize::new(0));
    let gate = OtpAllowGate::new(
        Arc::new(AdmitAll),
        Arc::new(StalledRelay {
            permits:   Arc::clone(&permits),
            delivered: Arc::clone(&delivered),
        }),
        1,
    );

    // The worker holds at most one, the queue one more; the other eight are dropped. Any
    // of them waiting on the stalled relay would hang here and trip the timeout.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        for n in 0..10 {
            gate.deliver_if_admitted(&format!("{n}@example.com"), OtpIssue::generate())
                .await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a stalled relay must not block a request");

    permits.add_permits(10);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while delivered.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the queued mail leaves once the relay recovers");
    tokio::task::yield_now().await;
    assert_eq!(delivered.load(Ordering::SeqCst), 2, "the rest were dropped");
}

#[test]
fn a_logged_failure_does_not_name_the_address() {
    assert_eq!(
        without_address("recipient <a@example.com> rejected", "a@example.com"),
        "recipient <[address]> rejected"
    );
}
