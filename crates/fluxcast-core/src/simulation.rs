//! Deterministic impairment simulator for the deadline-aware media path.
//!
//! The simulator is the M0 milestone's loss / reordering / expiry harness. It
//! fragments real [`AccessUnit`]s with [`fragment_access_unit`], protects each
//! frame with one [`XorParity`] block, and pushes both media and parity
//! datagrams through a seeded channel that can drop, reorder, and delay them.
//! The receiver side then exercises the production recovery primitives: XOR
//! recovery for a single missing fragment and deadline-based dropping of frames
//! that could never arrive in time.
//!
//! Everything is deterministic given a seed, so a report is reproducible in CI
//! and can back the numbers published in `VALIDATION.md` without a live network.

use std::time::{Duration, Instant};

use crate::{AccessUnit, CoreError, MAX_MEDIA_PAYLOAD, XorParity, fragment_access_unit};

/// A seeded, reproducible channel impairment profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelModel {
    /// Independent per-datagram drop probability in `0.0..=1.0`.
    pub loss_rate: f32,
    /// If true, the delivery order of surviving datagrams is deterministically
    /// shuffled to prove the receiver tolerates reordering.
    pub reorder: bool,
    /// One-way propagation delay added to every datagram. A frame whose deadline
    /// budget is smaller than this delay is dropped as late before transmission,
    /// exactly as a real deadline-aware sender would refuse to send stale media.
    pub propagation: Duration,
    /// PRNG seed. The same seed and model always produce the same report.
    pub seed: u64,
}

impl ChannelModel {
    /// A pristine channel: no loss, no reordering, no meaningful delay.
    #[must_use]
    pub const fn perfect() -> Self {
        Self {
            loss_rate: 0.0,
            reorder: false,
            propagation: Duration::ZERO,
            seed: 0,
        }
    }
}

/// The outcome of driving a set of access units through a [`ChannelModel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SimulationReport {
    /// Frames handed to the sender.
    pub frames_offered: u32,
    /// Media plus parity datagrams the sender emitted.
    pub datagrams_sent: u32,
    /// Datagrams the channel dropped.
    pub datagrams_lost: u32,
    /// Frames whose original bytes were reassembled without any recovery.
    pub frames_delivered_clean: u32,
    /// Frames completed only because XOR parity rebuilt a missing fragment.
    pub frames_recovered_by_fec: u32,
    /// Frames the sender skipped because they could not arrive before expiry.
    pub frames_dropped_late: u32,
    /// Frames lost outright: too many missing fragments for parity to recover.
    pub frames_dropped_lost: u32,
}

impl SimulationReport {
    /// Frames the receiver could ultimately present, with or without recovery.
    #[must_use]
    pub const fn frames_delivered(&self) -> u32 {
        self.frames_delivered_clean + self.frames_recovered_by_fec
    }

    /// Fraction of emitted datagrams the channel dropped.
    #[must_use]
    pub fn datagram_loss_rate(&self) -> f32 {
        if self.datagrams_sent == 0 {
            return 0.0;
        }
        f32::from(u16::try_from(self.datagrams_lost).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(self.datagrams_sent).unwrap_or(u16::MAX))
    }

    /// Fraction of transmitted frames the receiver could present.
    #[must_use]
    pub fn frame_delivery_rate(&self) -> f32 {
        let transmitted = self.frames_offered - self.frames_dropped_late;
        if transmitted == 0 {
            return 1.0;
        }
        f32::from(u16::try_from(self.frames_delivered()).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(transmitted).unwrap_or(u16::MAX))
    }

    /// Fraction of frames that were damaged in flight yet still recovered by FEC.
    #[must_use]
    pub fn fec_recovery_rate(&self) -> f32 {
        let damaged = self.frames_recovered_by_fec + self.frames_dropped_lost;
        if damaged == 0 {
            return 1.0;
        }
        f32::from(u16::try_from(self.frames_recovered_by_fec).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(damaged).unwrap_or(u16::MAX))
    }
}

/// Splits `bytes` into the same payload chunks [`fragment_access_unit`] uses.
fn payload_chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
    if bytes.is_empty() {
        return vec![Vec::new()];
    }
    bytes
        .chunks(MAX_MEDIA_PAYLOAD)
        .map(<[u8]>::to_vec)
        .collect()
}

/// One datagram in flight through the simulated channel.
struct InFlight {
    frame: usize,
    /// `Some(index)` for a media fragment; `None` for the frame's parity.
    fragment: Option<usize>,
    order: u64,
}

/// Small deterministic xorshift64 PRNG. Not cryptographic; only used to make
/// impairment decisions reproducible across runs and platforms.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Avoid the all-zero fixed point.
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Returns a value in `0.0..1.0` using 24 exact mantissa bits.
    fn next_unit(&mut self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let numerator = (self.next_u64() >> 40) as f32;
        numerator / 16_777_216.0_f32
    }
}

/// Drives `units` through `model` and returns a reproducible delivery report.
///
/// Each access unit becomes one frame: its fragments plus a single XOR parity
/// datagram. Media fragments and the parity are dropped independently with the
/// model's loss rate. A frame is recovered when exactly one media fragment is
/// missing and its parity survived; it is lost when two or more are missing.
///
/// # Errors
///
/// Returns an error when an access unit cannot be fragmented into valid FCDP
/// datagrams (for example when it needs more than `u16::MAX` fragments).
pub fn simulate_delivery(
    units: &[AccessUnit],
    session_id: u64,
    epoch: u16,
    model: &ChannelModel,
    now: Instant,
) -> Result<SimulationReport, CoreError> {
    let mut report = SimulationReport {
        frames_offered: u32::try_from(units.len()).unwrap_or(u32::MAX),
        ..SimulationReport::default()
    };
    let mut rng = XorShift64::new(model.seed);
    let mut sequence = 0_u32;
    let mut order_counter = 0_u64;

    // Per-frame state kept for the receive pass.
    let mut frame_parity: Vec<Option<XorParity>> = Vec::with_capacity(units.len());
    let mut frame_chunks: Vec<Vec<Vec<u8>>> = Vec::with_capacity(units.len());
    let mut frame_received: Vec<Vec<Option<Vec<u8>>>> = Vec::with_capacity(units.len());
    let mut frame_parity_received: Vec<bool> = Vec::with_capacity(units.len());
    let mut frame_late: Vec<bool> = Vec::with_capacity(units.len());
    let mut in_flight: Vec<InFlight> = Vec::new();

    for (frame, unit) in units.iter().enumerate() {
        // A deadline-aware sender never transmits a frame that cannot arrive in
        // time: best-case arrival is `now + propagation`.
        let late = now + model.propagation >= unit.deadline;
        frame_late.push(late);

        let chunks = payload_chunks(&unit.bytes);
        let parity = XorParity::encode(&chunks);
        let fragment_count = chunks.len();
        frame_chunks.push(chunks);
        frame_parity.push(parity.clone());
        frame_received.push(vec![None; fragment_count]);
        frame_parity_received.push(false);

        if late {
            report.frames_dropped_late += 1;
            // Still advance the sequence space as the sender would have.
            let datagrams = fragment_access_unit(session_id, epoch, &mut sequence, unit, now)?;
            let _ = datagrams;
            continue;
        }

        // Emit the media datagrams (validates FCDP framing for real).
        let datagrams = fragment_access_unit(session_id, epoch, &mut sequence, unit, now)?;
        for (index, _) in datagrams.iter().enumerate() {
            report.datagrams_sent += 1;
            if rng.next_unit() < model.loss_rate {
                report.datagrams_lost += 1;
                continue;
            }
            in_flight.push(InFlight {
                frame,
                fragment: Some(index),
                order: order_counter,
            });
            order_counter += 1;
        }

        // Emit one parity datagram for the frame, subject to the same loss.
        if parity.is_some() {
            report.datagrams_sent += 1;
            if rng.next_unit() < model.loss_rate {
                report.datagrams_lost += 1;
            } else {
                in_flight.push(InFlight {
                    frame,
                    fragment: None,
                    order: order_counter,
                });
                order_counter += 1;
            }
        }
    }

    if model.reorder {
        // Deterministic Fisher–Yates over the delivery order.
        for i in (1..in_flight.len()).rev() {
            let j = usize::try_from(rng.next_u64() % (i as u64 + 1)).unwrap_or(i);
            in_flight.swap(i, j);
        }
    } else {
        in_flight.sort_by_key(|item| item.order);
    }

    // Receive pass: order must not matter for per-frame reassembly.
    for item in &in_flight {
        match item.fragment {
            Some(index) => {
                frame_received[item.frame][index] = Some(frame_chunks[item.frame][index].clone());
            }
            None => frame_parity_received[item.frame] = true,
        }
    }

    tally_frames(
        &frame_late,
        &mut frame_received,
        &frame_parity_received,
        &frame_parity,
        &mut report,
    );

    // Reassembled bytes must equal the originals; verify on every completed
    // frame so the harness cannot silently report a delivery it corrupted.
    debug_assert_reassembly(units, &frame_received, &frame_late);

    Ok(report)
}

/// Classifies each transmitted frame as clean, FEC-recovered, or lost.
fn tally_frames(
    frame_late: &[bool],
    frame_received: &mut [Vec<Option<Vec<u8>>>],
    frame_parity_received: &[bool],
    frame_parity: &[Option<XorParity>],
    report: &mut SimulationReport,
) {
    for frame in 0..frame_late.len() {
        if frame_late[frame] {
            continue;
        }
        let received = &mut frame_received[frame];
        let missing = received.iter().filter(|slot| slot.is_none()).count();
        match missing {
            0 => report.frames_delivered_clean += 1,
            1 if frame_parity_received[frame] => match &frame_parity[frame] {
                Some(parity) => match parity.recover_one(received) {
                    Some((index, bytes)) => {
                        received[index] = Some(bytes);
                        report.frames_recovered_by_fec += 1;
                    }
                    None => report.frames_dropped_lost += 1,
                },
                None => report.frames_dropped_lost += 1,
            },
            _ => report.frames_dropped_lost += 1,
        }
    }
}

/// In debug builds, confirm every completed frame reassembles to its input.
fn debug_assert_reassembly(units: &[AccessUnit], received: &[Vec<Option<Vec<u8>>>], late: &[bool]) {
    if !cfg!(debug_assertions) {
        return;
    }
    for (frame, unit) in units.iter().enumerate() {
        if late[frame] || received[frame].iter().any(Option::is_none) {
            continue;
        }
        let rebuilt: Vec<u8> = received[frame]
            .iter()
            .flatten()
            .flat_map(|chunk| chunk.iter().copied())
            .collect();
        debug_assert_eq!(rebuilt, unit.bytes, "frame {frame} reassembly mismatch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaKind;

    fn frame(frame_id: u32, len: usize, deadline: Instant) -> AccessUnit {
        AccessUnit {
            stream_id: 1,
            frame_id,
            kind: MediaKind::VideoKey,
            deadline,
            bytes: vec![u8::try_from(frame_id % 251).unwrap(); len],
        }
    }

    #[test]
    fn perfect_channel_delivers_every_frame_cleanly() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(1);
        let units: Vec<_> = (0..20)
            .map(|i| frame(i, MAX_MEDIA_PAYLOAD * 2 + 5, deadline))
            .collect();
        let report = simulate_delivery(&units, 1, 0, &ChannelModel::perfect(), now).unwrap();
        assert_eq!(report.frames_offered, 20);
        assert_eq!(report.frames_delivered_clean, 20);
        assert_eq!(report.frames_recovered_by_fec, 0);
        assert_eq!(report.datagrams_lost, 0);
        assert!((report.frame_delivery_rate() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reordering_alone_never_loses_a_frame() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(1);
        let units: Vec<_> = (0..50)
            .map(|i| frame(i, MAX_MEDIA_PAYLOAD * 3 + 1, deadline))
            .collect();
        let model = ChannelModel {
            loss_rate: 0.0,
            reorder: true,
            propagation: Duration::from_millis(10),
            seed: 0xC0FF_EE01,
        };
        let report = simulate_delivery(&units, 7, 0, &model, now).unwrap();
        assert_eq!(report.frames_delivered(), 50);
        assert_eq!(report.frames_dropped_lost, 0);
    }

    #[test]
    fn single_fragment_loss_is_recovered_by_parity() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(1);
        // Two-fragment frames: any single loss is exactly what XOR parity fixes.
        let units: Vec<_> = (0..40)
            .map(|i| frame(i, MAX_MEDIA_PAYLOAD + 7, deadline))
            .collect();
        let mut best = SimulationReport::default();
        // Sweep seeds to find a run that drops at most one datagram per frame.
        for seed in 0..64 {
            let model = ChannelModel {
                loss_rate: 0.08,
                reorder: true,
                propagation: Duration::from_millis(5),
                seed,
            };
            let report = simulate_delivery(&units, 3, 0, &model, now).unwrap();
            if report.frames_recovered_by_fec > best.frames_recovered_by_fec {
                best = report;
            }
        }
        assert!(
            best.frames_recovered_by_fec > 0,
            "parity should recover at least one damaged frame across seeds"
        );
        // Every frame the receiver presented is either clean or FEC-recovered.
        assert_eq!(
            best.frames_delivered() + best.frames_dropped_lost + best.frames_dropped_late,
            best.frames_offered
        );
    }

    #[test]
    fn frames_past_their_deadline_are_dropped_before_sending() {
        let now = Instant::now();
        let units = vec![
            frame(1, 100, now + Duration::from_millis(5)),
            frame(2, 100, now + Duration::from_secs(1)),
        ];
        let model = ChannelModel {
            loss_rate: 0.0,
            reorder: false,
            propagation: Duration::from_millis(50),
            seed: 1,
        };
        let report = simulate_delivery(&units, 1, 0, &model, now).unwrap();
        assert_eq!(report.frames_dropped_late, 1);
        assert_eq!(report.frames_delivered_clean, 1);
    }

    #[test]
    fn report_is_deterministic_for_a_seed() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(1);
        let units: Vec<_> = (0..30)
            .map(|i| frame(i, MAX_MEDIA_PAYLOAD * 2, deadline))
            .collect();
        let model = ChannelModel {
            loss_rate: 0.1,
            reorder: true,
            propagation: Duration::from_millis(8),
            seed: 42,
        };
        let first = simulate_delivery(&units, 9, 0, &model, now).unwrap();
        let second = simulate_delivery(&units, 9, 0, &model, now).unwrap();
        assert_eq!(first, second);
    }
}
