//! Evidence about a process-local subscription, not a proof of source coverage.
//! Broadcast lag does not identify a symbol, source timestamp or missing domain
//! within an all-event subscription. Keep the entire subscription tainted until
//! its state is rebuilt; wall-clock passage alone cannot repair cumulative data.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubscriptionQuality {
    pub scope: &'static str,
    pub upstream_completeness: &'static str,
    pub has_known_gap: bool,
    pub gap_epoch: u64,
    pub skipped_events_total: u64,
    /// Local detection time; not the source-time extent of missing events.
    pub last_gap_detected_at_ms: Option<u64>,
    pub recovery_policy: &'static str,
}

impl Default for SubscriptionQuality {
    fn default() -> Self {
        Self {
            scope: "process_local_subscription",
            upstream_completeness: "not_verified",
            has_known_gap: false,
            gap_epoch: 0,
            skipped_events_total: 0,
            last_gap_detected_at_ms: None,
            recovery_policy: "rebuild_state_required_no_automatic_reset",
        }
    }
}

impl SubscriptionQuality {
    pub fn record_gap(&mut self, skipped: u64, detected_at_ms: u64) {
        if skipped == 0 {
            return;
        }
        self.has_known_gap = true;
        self.gap_epoch = self.gap_epoch.saturating_add(1);
        self.skipped_events_total = self.skipped_events_total.saturating_add(skipped);
        self.last_gap_detected_at_ms = Some(detected_at_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_are_sticky_and_detection_time_is_not_used_for_recovery() {
        let mut quality = SubscriptionQuality::default();
        assert!(!quality.has_known_gap);
        assert_eq!(quality.upstream_completeness, "not_verified");
        quality.record_gap(3, 1_000);
        quality.record_gap(2, 500); // Local wall clock may move backwards.
        quality.record_gap(0, u64::MAX);
        assert!(quality.has_known_gap);
        assert_eq!(quality.gap_epoch, 2);
        assert_eq!(quality.skipped_events_total, 5);
        assert_eq!(quality.last_gap_detected_at_ms, Some(500));
    }
}
