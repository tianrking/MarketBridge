use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use tokio::time::sleep;
use tracing::warn;

const INITIAL_RECONNECT_MS: u64 = 500;
const MAX_RECONNECT_MS: u64 = 30_000;

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
}

pub async fn run_reconnecting<F, Fut>(label: &'static str, mut run_once: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut backoff = ReconnectBackoff::default();
    loop {
        match run_once().await {
            Ok(()) => {
                warn!(label, "websocket session ended without error, reconnecting");
            }
            Err(error) => {
                warn!(label, error = %error, "websocket session failed, reconnecting");
            }
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
}
