use std::future::Future;
use std::io::Read;
use std::time::Duration;

use anyhow::Result;
use flate2::read::{GzDecoder, ZlibDecoder};
use tokio::time::{Instant, sleep};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
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
    run_reconnecting_until(label, CancellationToken::new(), move || run_once()).await
}

pub async fn run_reconnecting_until<F, Fut>(
    label: &'static str,
    shutdown: CancellationToken,
    mut run_once: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut backoff = ReconnectBackoff::default();
    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }
        let started_at = Instant::now();
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            result = run_once() => {
                match result {
                    Ok(()) => {
                        warn!(label, "websocket session ended without error, reconnecting");
                    }
                    Err(error) => {
                        warn!(label, error = %error, "websocket session failed, reconnecting");
                    }
                }
            }
        };
        if started_at.elapsed() >= Duration::from_secs(STABLE_SESSION_RESET_SECS) {
            backoff.reset();
        }
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            _ = sleep(backoff.next_delay()) => {}
        }
    }
}

pub fn message_text(message: &Message) -> Result<Option<String>> {
    match message {
        Message::Text(text) => Ok(Some(text.to_string())),
        Message::Binary(bytes) => decode_binary_message(bytes).map(Some),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(None),
    }
}

fn decode_binary_message(bytes: &[u8]) -> Result<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_string());
    }

    let mut gzip = String::new();
    if GzDecoder::new(bytes).read_to_string(&mut gzip).is_ok() {
        return Ok(gzip);
    }

    let mut zlib = String::new();
    if ZlibDecoder::new(bytes).read_to_string(&mut zlib).is_ok() {
        return Ok(zlib);
    }

    anyhow::bail!("binary websocket message is not utf8, gzip, or zlib")
}

#[cfg(test)]
mod tests {
    use super::{ReconnectBackoff, message_text};
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    use std::time::Duration;
    use tokio_tungstenite::tungstenite::Message;

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

    #[test]
    fn websocket_message_text_decodes_gzip_binary() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(br#"{"ok":true}"#).expect("write gzip");
        let bytes = encoder.finish().expect("finish gzip");

        let text = message_text(&Message::Binary(bytes))
            .expect("decode")
            .expect("text");

        assert_eq!(text, r#"{"ok":true}"#);
    }
}
