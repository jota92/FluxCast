//! Standard observability counters and Prometheus/OpenMetrics rendering.
//!
//! The spec (§11) fixes a small set of technical indicators every `FluxCast`
//! implementation must expose. This module collects them and renders the
//! Prometheus/`OpenMetrics` text exposition format so any deployment can scrape
//! latency, loss, recovery, and relay-resource metrics without a bespoke
//! endpoint.

use std::fmt::Write as _;

use crate::RelayMetrics;

/// Per-session delivery counters, updated by the receive path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionMetrics {
    /// MEDIA fragments the sender indicated (received + missing).
    pub media_expected: u64,
    /// MEDIA fragments actually received.
    pub media_received: u64,
    /// Frames rebuilt from FEC parity.
    pub frames_fec_recovered: u64,
    /// Frames completed by a NACK retransmission.
    pub frames_nack_recovered: u64,
    /// Frames dropped for missing their deadline.
    pub frames_late_dropped: u64,
    /// Frames presented to the application (clean, FEC, or NACK).
    pub frames_delivered: u64,
    /// Frames offered for transmission.
    pub frames_total: u64,
    /// Cumulative audio-gap time in milliseconds.
    pub audio_gap_ms: u64,
    /// Most recent keyframe recovery time in milliseconds.
    pub keyframe_recovery_ms: u64,
    /// Total bytes sent to subscribers this interval.
    pub egress_bytes: u64,
}

impl SessionMetrics {
    /// Fraction of expected MEDIA fragments that were not received.
    #[must_use]
    pub fn packet_loss_rate(&self) -> f64 {
        ratio(
            self.media_expected - self.media_received.min(self.media_expected),
            self.media_expected,
        )
    }

    /// Fraction of delivered frames that needed FEC to complete.
    #[must_use]
    pub fn fec_recovery_rate(&self) -> f64 {
        ratio(self.frames_fec_recovered, self.frames_delivered)
    }

    /// Fraction of delivered frames that needed a NACK retransmission.
    #[must_use]
    pub fn nack_recovery_rate(&self) -> f64 {
        ratio(self.frames_nack_recovered, self.frames_delivered)
    }

    /// Fraction of frames dropped for missing their deadline.
    #[must_use]
    pub fn late_frame_drop_rate(&self) -> f64 {
        ratio(self.frames_late_dropped, self.frames_total)
    }

    /// Renders these metrics as Prometheus text for one session label.
    #[must_use]
    pub fn to_prometheus(&self, session: &str) -> String {
        let label = escape_label(session);
        let mut out = String::new();
        gauge(
            &mut out,
            "fluxcast_packet_loss_rate",
            "Fraction of expected MEDIA fragments not received",
            &label,
            self.packet_loss_rate(),
        );
        gauge(
            &mut out,
            "fluxcast_fec_recovery_rate",
            "Fraction of delivered frames repaired by FEC",
            &label,
            self.fec_recovery_rate(),
        );
        gauge(
            &mut out,
            "fluxcast_nack_recovery_rate",
            "Fraction of delivered frames repaired by retransmission",
            &label,
            self.nack_recovery_rate(),
        );
        gauge(
            &mut out,
            "fluxcast_late_frame_drop_rate",
            "Fraction of frames dropped for missing their deadline",
            &label,
            self.late_frame_drop_rate(),
        );
        gauge(
            &mut out,
            "fluxcast_keyframe_recovery_ms",
            "Most recent keyframe recovery time in milliseconds",
            &label,
            as_f64(self.keyframe_recovery_ms),
        );
        gauge(
            &mut out,
            "fluxcast_audio_gap_ms",
            "Cumulative audio gap in milliseconds",
            &label,
            as_f64(self.audio_gap_ms),
        );
        gauge(
            &mut out,
            "fluxcast_egress_bytes_total",
            "Bytes sent to subscribers",
            &label,
            as_f64(self.egress_bytes),
        );
        out
    }
}

/// Renders relay resource/fan-out metrics as Prometheus text.
#[must_use]
pub fn relay_metrics_to_prometheus(
    metrics: &RelayMetrics,
    cpu_percent: f64,
    memory_mb: f64,
) -> String {
    let mut out = String::new();
    gauge(
        &mut out,
        "fluxcast_relay_active_sessions",
        "Sessions currently forwarded by the relay",
        "",
        as_f64(metrics.active_sessions as u64),
    );
    gauge(
        &mut out,
        "fluxcast_relay_active_subscribers",
        "Subscribers currently leased across all sessions",
        "",
        as_f64(metrics.active_subscribers as u64),
    );
    gauge(
        &mut out,
        "fluxcast_relay_forwarded_packets_total",
        "Datagrams the relay handed to the OS",
        "",
        as_f64(metrics.forwarded_packets),
    );
    gauge(
        &mut out,
        "fluxcast_relay_forwarded_bytes_total",
        "Bytes the relay handed to the OS",
        "",
        as_f64(metrics.forwarded_bytes),
    );
    gauge(
        &mut out,
        "fluxcast_relay_cpu_percent",
        "Relay process CPU usage percent",
        "",
        cpu_percent,
    );
    gauge(
        &mut out,
        "fluxcast_relay_memory_mb",
        "Relay resident memory in megabytes",
        "",
        memory_mb,
    );
    out
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    (as_f64(numerator) / as_f64(denominator)).clamp(0.0, 1.0)
}

/// Widening cast for metric magnitudes. Values here never exceed 2^52, so the
/// theoretical `f64` mantissa loss cannot occur in practice.
#[allow(clippy::cast_precision_loss)]
fn as_f64(value: u64) -> f64 {
    value as f64
}

fn gauge(out: &mut String, name: &str, help: &str, label: &str, value: f64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    if label.is_empty() {
        let _ = writeln!(out, "{name} {value}");
    } else {
        let _ = writeln!(out, "{name}{{session=\"{label}\"}} {value}");
    }
}

fn escape_label(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '"' | '\\' => '_',
            '\n' => ' ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_derived_and_bounded() {
        let metrics = SessionMetrics {
            media_expected: 1000,
            media_received: 985,
            frames_fec_recovered: 12,
            frames_nack_recovered: 3,
            frames_late_dropped: 2,
            frames_delivered: 300,
            frames_total: 302,
            ..SessionMetrics::default()
        };
        assert!((metrics.packet_loss_rate() - 0.015).abs() < 1e-9);
        assert!((metrics.fec_recovery_rate() - 12.0 / 300.0).abs() < 1e-9);
        assert!((metrics.nack_recovery_rate() - 3.0 / 300.0).abs() < 1e-9);
        assert!((metrics.late_frame_drop_rate() - 2.0 / 302.0).abs() < 1e-9);
    }

    #[test]
    fn empty_counters_do_not_divide_by_zero() {
        let metrics = SessionMetrics::default();
        assert!(metrics.packet_loss_rate().abs() < f64::EPSILON);
        assert!(metrics.fec_recovery_rate().abs() < f64::EPSILON);
    }

    #[test]
    fn prometheus_output_is_well_formed() {
        let metrics = SessionMetrics {
            media_expected: 100,
            media_received: 99,
            frames_delivered: 50,
            frames_fec_recovered: 1,
            frames_total: 50,
            ..SessionMetrics::default()
        };
        let text = metrics.to_prometheus("room/live");
        assert!(text.contains("# TYPE fluxcast_packet_loss_rate gauge"));
        assert!(text.contains("fluxcast_packet_loss_rate{session=\"room/live\"}"));
        // Labels are sanitized: quotes/backslashes cannot break the format.
        let messy = metrics.to_prometheus("a\"b\\c");
        assert!(messy.contains("session=\"a_b_c\""));
    }

    #[test]
    fn relay_metrics_render_without_a_session_label() {
        let relay = RelayMetrics {
            active_sessions: 2,
            active_subscribers: 7,
            forwarded_packets: 1000,
            forwarded_bytes: 1_200_000,
            expired_subscriptions: 3,
        };
        let text = relay_metrics_to_prometheus(&relay, 12.5, 48.0);
        assert!(text.contains("fluxcast_relay_active_subscribers 7"));
        assert!(text.contains("fluxcast_relay_cpu_percent 12.5"));
    }
}
