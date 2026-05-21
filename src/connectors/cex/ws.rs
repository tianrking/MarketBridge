use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use tokio::time::{Instant, sleep};
use tracing::warn;

const INITIAL_RECONNECT_MS: u64 = 500;
const MAX_RECONNECT_MS: u64 = 30_000;
const STABLE_SESSION_RESET_SECS: u64 = 300;

#[derive(Debug, Clone)]
struct ReconnectBackoff {
    current: Duration,
    max: Duration,
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self {
            current: Duration::from_millis(INITIAL_RECONNECT_MS),
            max: Duration::from_millis(MAX_RECONNECT_MS),
        }
    }
}

impl ReconnectBackoff {
    fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.max);
        delay
    }

    fn reset(&mut self) {
        self.current = Duration::from_millis(INITIAL_RECONNECT_MS);
    }
}

pub async fn run_reconnecting<F, Fut>(label: &'static str, mut run_once: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut backoff = ReconnectBackoff::default();
    loop {
        let started_at = Instant::now();
        match run_once().await {
            Ok(()) => {
                warn!(label, "websocket session ended without error, reconnecting");
            }
            Err(error) => {
                warn!(label, error = %error, "websocket session failed, reconnecting");
            }
        }
        if started_at.elapsed() >= Duration::from_secs(STABLE_SESSION_RESET_SECS) {
            backoff.reset();
        }
        sleep(backoff.next_delay()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::ReconnectBackoff;
    use std::time::Duration;

    #[test]
    fn reconnect_backoff_caps_at_max() {
        let mut backoff = ReconnectBackoff {
            current: Duration::from_secs(20),
            max: Duration::from_secs(30),
        };

        assert_eq!(backoff.next_delay(), Duration::from_secs(20));
        assert_eq!(backoff.next_delay(), Duration::from_secs(30));
        assert_eq!(backoff.next_delay(), Duration::from_secs(30));
    }

    #[test]
    fn reconnect_backoff_can_reset_after_stable_session() {
        let mut backoff = ReconnectBackoff {
            current: Duration::from_secs(30),
            max: Duration::from_secs(30),
        };
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(500));
    }
}
