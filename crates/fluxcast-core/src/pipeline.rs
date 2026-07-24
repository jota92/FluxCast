//! The M1 media pipeline: a cohesive sender and receiver that wire the crate's
//! deadline, fragmentation, FEC, and retransmission primitives into one flow.
//!
//! [`MediaSender`] turns encoded [`AccessUnit`]s into FCDP `MEDIA` datagrams,
//! optionally emits one XOR `FEC` datagram per frame, retains audio and
//! keyframe datagrams for retransmission, and answers `NACK` requests while the
//! data is still useful. [`MediaReceiver`] reassembles frames, recovers a single
//! lost fragment per frame from parity, drops frames that miss their deadline,
//! and reports the sequence numbers worth requesting again.
//!
//! Control payloads (`FEC`, `NACK`, `ACK`) get explicit, untrusted-input-safe
//! codecs here so every SDK can speak the same feedback language.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use fluxcast_proto::{Header, PacketType};

use crate::{
    AccessUnit, CoreError, MAX_MEDIA_PAYLOAD, OutboundDatagram, RetransmitWindow,
    TransportFeedback, fragment_access_unit, fragment_access_unit_sized,
};

/// Bytes an `FEC` payload spends on its own header, ahead of the parity block.
const FEC_HEADER_LEN: usize = 8;

/// Source-symbol size for FEC-protected media. Kept below the media budget so a
/// parity datagram (as large as the biggest symbol plus [`FEC_HEADER_LEN`])
/// still fits [`MAX_MEDIA_PAYLOAD`].
pub const FEC_SYMBOL_SIZE: usize = MAX_MEDIA_PAYLOAD - FEC_HEADER_LEN;

/// One frame's XOR parity over equal-size source symbols.
///
/// Every source symbol is zero-padded to `symbol_len` before XOR, so the parity
/// is exactly `symbol_len` bytes. `original_len` lets a receiver recover the
/// true length of a rebuilt final symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FecBlock {
    pub symbol_len: u16,
    pub fragment_count: u16,
    pub original_len: u32,
    pub parity: Vec<u8>,
}

impl FecBlock {
    /// Builds the parity block for a frame's ordered source symbols.
    #[must_use]
    fn build(symbols: &[Vec<u8>], original_len: usize) -> Option<Self> {
        if symbols.is_empty() || symbols.len() > usize::from(u16::MAX) {
            return None;
        }
        let symbol_len = symbols.iter().map(Vec::len).max().unwrap_or(0);
        let mut parity = vec![0u8; symbol_len];
        for symbol in symbols {
            for (slot, byte) in parity.iter_mut().zip(symbol) {
                *slot ^= byte;
            }
        }
        Some(Self {
            symbol_len: u16::try_from(symbol_len).ok()?,
            fragment_count: u16::try_from(symbols.len()).ok()?,
            original_len: u32::try_from(original_len).ok()?,
            parity,
        })
    }

    /// Rebuilds the single missing fragment in place, returning its index.
    /// Returns `None` unless exactly one fragment is absent.
    #[must_use]
    fn recover(&self, fragments: &mut [Option<Vec<u8>>]) -> Option<usize> {
        if fragments.len() != usize::from(self.fragment_count) {
            return None;
        }
        let symbol_len = usize::from(self.symbol_len);
        let mut missing = None;
        let mut recovered = self.parity.clone();
        recovered.resize(symbol_len, 0);
        for (index, slot) in fragments.iter().enumerate() {
            match slot {
                Some(present) => {
                    if present.len() > symbol_len {
                        return None;
                    }
                    for (out, byte) in recovered.iter_mut().zip(present) {
                        *out ^= byte;
                    }
                }
                None if missing.is_none() => missing = Some(index),
                None => return None,
            }
        }
        let index = missing?;
        let count = usize::from(self.fragment_count);
        let true_len = if index + 1 == count {
            usize::try_from(self.original_len)
                .ok()?
                .checked_sub(symbol_len.checked_mul(count - 1)?)?
        } else {
            symbol_len
        };
        if true_len > recovered.len() {
            return None;
        }
        recovered.truncate(true_len);
        fragments[index] = Some(recovered);
        Some(index)
    }
}

/// Serializes an [`FecBlock`] as an `FEC` datagram payload.
///
/// Layout: `symbol_len: u16`, `fragment_count: u16`, `original_len: u32`, then
/// the `symbol_len` parity bytes.
#[must_use]
pub fn encode_fec_payload(block: &FecBlock) -> Vec<u8> {
    let mut out = Vec::with_capacity(FEC_HEADER_LEN + block.parity.len());
    out.extend_from_slice(&block.symbol_len.to_be_bytes());
    out.extend_from_slice(&block.fragment_count.to_be_bytes());
    out.extend_from_slice(&block.original_len.to_be_bytes());
    out.extend_from_slice(&block.parity);
    out
}

/// Parses an `FEC` payload, rejecting any malformed or inconsistent record.
#[must_use]
pub fn decode_fec_payload(bytes: &[u8]) -> Option<FecBlock> {
    let header: &[u8; FEC_HEADER_LEN] = bytes.get(..FEC_HEADER_LEN)?.try_into().ok()?;
    let symbol_len = u16::from_be_bytes([header[0], header[1]]);
    let fragment_count = u16::from_be_bytes([header[2], header[3]]);
    let original_len = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let parity = bytes.get(FEC_HEADER_LEN..)?.to_vec();
    if parity.len() != usize::from(symbol_len) || fragment_count == 0 {
        return None;
    }
    Some(FecBlock {
        symbol_len,
        fragment_count,
        original_len,
        parity,
    })
}

/// Serializes requested sequence numbers as a `NACK` payload.
#[must_use]
pub fn encode_nack_payload(sequences: &[u32]) -> Vec<u8> {
    let count = u16::try_from(sequences.len()).unwrap_or(u16::MAX);
    let mut out = Vec::with_capacity(2 + usize::from(count) * 4);
    out.extend_from_slice(&count.to_be_bytes());
    for sequence in sequences.iter().take(usize::from(count)) {
        out.extend_from_slice(&sequence.to_be_bytes());
    }
    out
}

/// Parses a `NACK` payload into requested sequence numbers.
#[must_use]
pub fn decode_nack_payload(bytes: &[u8]) -> Option<Vec<u32>> {
    let count = usize::from(u16::from_be_bytes([*bytes.first()?, *bytes.get(1)?]));
    let body = bytes.get(2..2 + count.checked_mul(4)?)?;
    Some(
        body.chunks_exact(4)
            .map(|q| u32::from_be_bytes([q[0], q[1], q[2], q[3]]))
            .collect(),
    )
}

/// Serializes a receiver report as a fixed 16-byte `ACK` payload.
#[must_use]
pub fn encode_feedback_payload(feedback: TransportFeedback) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&feedback.sent.to_be_bytes());
    out[4..8].copy_from_slice(&feedback.received.to_be_bytes());
    out[8..12].copy_from_slice(&feedback.late.to_be_bytes());
    let micros = u32::try_from(feedback.rtt.as_micros()).unwrap_or(u32::MAX);
    out[12..16].copy_from_slice(&micros.to_be_bytes());
    out
}

/// Parses an `ACK` payload into a receiver report.
#[must_use]
pub fn decode_feedback_payload(bytes: &[u8]) -> Option<TransportFeedback> {
    let bytes: &[u8; 16] = bytes.get(..16)?.try_into().ok()?;
    Some(TransportFeedback {
        sent: u32::from_be_bytes(bytes[0..4].try_into().ok()?),
        received: u32::from_be_bytes(bytes[4..8].try_into().ok()?),
        late: u32::from_be_bytes(bytes[8..12].try_into().ok()?),
        rtt: Duration::from_micros(u64::from(u32::from_be_bytes(
            bytes[12..16].try_into().ok()?,
        ))),
    })
}

/// How much forward error correction the sender adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FecPolicy {
    /// No parity datagrams. Recovery relies on retransmission alone.
    Off,
    /// One XOR parity datagram per frame, repairing any single lost fragment.
    PerFrame,
}

/// Deadline-aware sender that produces FCDP datagrams from access units.
#[derive(Debug)]
pub struct MediaSender {
    session_id: u64,
    epoch: u16,
    next_sequence: u32,
    fec: FecPolicy,
    window: RetransmitWindow,
}

impl MediaSender {
    /// Creates a sender. `retransmit_capacity` bounds the audio/keyframe cache.
    #[must_use]
    pub fn new(session_id: u64, epoch: u16, fec: FecPolicy, retransmit_capacity: usize) -> Self {
        Self {
            session_id,
            epoch,
            next_sequence: 0,
            fec,
            window: RetransmitWindow::new(retransmit_capacity),
        }
    }

    /// Encodes one access unit into MEDIA datagrams plus, when enabled, a single
    /// per-frame FEC datagram. Audio and keyframe datagrams are retained for
    /// possible retransmission.
    ///
    /// # Errors
    ///
    /// Returns an error when the access unit cannot be validly fragmented.
    pub fn encode_access_unit(
        &mut self,
        unit: &AccessUnit,
        now: Instant,
    ) -> Result<Vec<OutboundDatagram>, CoreError> {
        // FEC-protected frames fragment at the smaller symbol size so parity
        // aligns with the media fragments and still fits one datagram.
        let media = if self.fec == FecPolicy::PerFrame {
            fragment_access_unit_sized(
                self.session_id,
                self.epoch,
                &mut self.next_sequence,
                unit,
                now,
                FEC_SYMBOL_SIZE,
            )?
        } else {
            fragment_access_unit(
                self.session_id,
                self.epoch,
                &mut self.next_sequence,
                unit,
                now,
            )?
        };
        for datagram in &media {
            self.window.insert(datagram.clone());
        }

        let mut datagrams = media;
        if self.fec == FecPolicy::PerFrame {
            if let Some(fec) = self.build_fec(unit, now)? {
                datagrams.push(fec);
            }
        }
        Ok(datagrams)
    }

    fn build_fec(
        &mut self,
        unit: &AccessUnit,
        now: Instant,
    ) -> Result<Option<OutboundDatagram>, CoreError> {
        let symbols: Vec<Vec<u8>> = if unit.bytes.is_empty() {
            vec![Vec::new()]
        } else {
            unit.bytes
                .chunks(FEC_SYMBOL_SIZE)
                .map(<[u8]>::to_vec)
                .collect()
        };
        let Some(block) = FecBlock::build(&symbols, unit.bytes.len()) else {
            return Ok(None);
        };
        let payload = encode_fec_payload(&block);
        let remaining = unit
            .deadline
            .saturating_duration_since(now)
            .as_millis()
            .min(u128::from(u16::MAX));
        let deadline_ms = u16::try_from(remaining).map_err(|_| CoreError::DeadlineOverflow)?;

        let mut header = Header::new(PacketType::Fec);
        header.session_id = self.session_id;
        header.stream_id = unit.stream_id;
        header.epoch = self.epoch;
        header.sequence_number = self.next_sequence;
        header.frame_id = unit.frame_id;
        header.fragment_index = 0;
        header.fragment_count = 1;
        header.priority = unit.kind.priority();
        header.deadline_ms = deadline_ms;
        let mut bytes = Vec::new();
        header.encode(&payload, &mut bytes)?;
        let datagram = OutboundDatagram {
            sequence_number: self.next_sequence,
            kind: unit.kind,
            deadline: unit.deadline,
            bytes,
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        Ok(Some(datagram))
    }

    /// Returns retransmissions for the still-recoverable requested sequences.
    /// Expired or non-cached (delta-video) sequences are silently skipped.
    #[must_use]
    pub fn on_nack(&self, requested: &[u32], now: Instant) -> Vec<OutboundDatagram> {
        requested
            .iter()
            .filter_map(|sequence| self.window.get_before_deadline(*sequence, now).cloned())
            .collect()
    }

    #[must_use]
    pub const fn next_sequence(&self) -> u32 {
        self.next_sequence
    }
}

/// The result of feeding one datagram to a [`MediaReceiver`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivered {
    /// The frame is still incomplete; more datagrams are needed.
    Pending,
    /// A complete access unit, reassembled directly from received fragments.
    Clean(Vec<u8>),
    /// A complete access unit whose missing fragment was rebuilt from parity.
    Recovered(Vec<u8>),
    /// The datagram was valid but its frame had already been delivered.
    Duplicate,
}

#[derive(Debug)]
struct FrameState {
    base_sequence: u32,
    total: u16,
    priority: u8,
    deadline: Instant,
    fragments: Vec<Option<Vec<u8>>>,
    parity: Option<FecBlock>,
    completed: bool,
}

impl FrameState {
    /// Builds the reassembled bytes and turns this entry into a lightweight
    /// tombstone: `completed` is set and the fragment buffers are released so a
    /// late duplicate is recognized (not rebuilt into a phantom frame) until the
    /// deadline retires the key.
    fn complete(&mut self) -> Vec<u8> {
        let bytes = concat_fragments(&self.fragments);
        self.completed = true;
        self.fragments = Vec::new();
        self.parity = None;
        bytes
    }
}

/// Deadline-aware receiver that reassembles, FEC-recovers, and drives NACKs.
#[derive(Debug, Default)]
pub struct MediaReceiver {
    frames: HashMap<(u16, u32), FrameState>,
}

impl MediaReceiver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts one decoded MEDIA or FEC datagram.
    ///
    /// # Errors
    ///
    /// Returns an error when a datagram's frame metadata contradicts what was
    /// already observed for that frame, or when it is neither MEDIA nor FEC.
    pub fn accept(
        &mut self,
        header: Header,
        payload: &[u8],
        now: Instant,
    ) -> Result<Delivered, CoreError> {
        self.drop_expired(now);
        match header.packet_type {
            PacketType::Media => self.accept_media(header, payload, now),
            PacketType::Fec => self.accept_fec(header, payload, now),
            _ => Err(CoreError::NotMedia),
        }
    }

    fn accept_media(
        &mut self,
        header: Header,
        payload: &[u8],
        now: Instant,
    ) -> Result<Delivered, CoreError> {
        if header.fragment_index >= header.fragment_count {
            return Err(CoreError::InconsistentFrame);
        }
        let key = (header.stream_id, header.frame_id);
        let base_sequence = header
            .sequence_number
            .wrapping_sub(u32::from(header.fragment_index));
        let deadline = now
            .checked_add(Duration::from_millis(u64::from(header.deadline_ms)))
            .unwrap_or(now);
        let entry = self.frames.entry(key).or_insert_with(|| FrameState {
            base_sequence,
            total: header.fragment_count,
            priority: header.priority,
            deadline,
            fragments: vec![None; usize::from(header.fragment_count)],
            parity: None,
            completed: false,
        });
        if entry.total != header.fragment_count {
            self.frames.remove(&key);
            return Err(CoreError::InconsistentFrame);
        }
        if entry.completed {
            return Ok(Delivered::Duplicate);
        }
        entry.base_sequence = base_sequence;
        entry.fragments[usize::from(header.fragment_index)].get_or_insert_with(|| payload.to_vec());
        Ok(Self::try_complete(&mut self.frames, key))
    }

    fn accept_fec(
        &mut self,
        header: Header,
        payload: &[u8],
        now: Instant,
    ) -> Result<Delivered, CoreError> {
        let Some(parity) = decode_fec_payload(payload) else {
            return Err(CoreError::InvalidMedia);
        };
        let key = (header.stream_id, header.frame_id);
        let deadline = now
            .checked_add(Duration::from_millis(u64::from(header.deadline_ms)))
            .unwrap_or(now);
        let fragment_count = parity.fragment_count;
        let entry = self.frames.entry(key).or_insert_with(|| FrameState {
            base_sequence: header.sequence_number,
            total: fragment_count,
            priority: header.priority,
            deadline,
            fragments: vec![None; usize::from(parity.fragment_count)],
            parity: None,
            completed: false,
        });
        if entry.total != fragment_count {
            // A parity block that disagrees with the media fragment count is
            // unusable; keep the media state and ignore the parity.
            return Ok(Delivered::Pending);
        }
        if entry.completed {
            return Ok(Delivered::Duplicate);
        }
        entry.parity = Some(parity);
        Ok(Self::try_complete(&mut self.frames, key))
    }

    fn try_complete(frames: &mut HashMap<(u16, u32), FrameState>, key: (u16, u32)) -> Delivered {
        let Some(entry) = frames.get_mut(&key) else {
            return Delivered::Pending;
        };
        let missing = entry.fragments.iter().filter(|slot| slot.is_none()).count();
        if missing == 0 {
            return Delivered::Clean(entry.complete());
        }
        if missing == 1 {
            if let Some(parity) = entry.parity.clone() {
                if parity.recover(&mut entry.fragments).is_some() {
                    return Delivered::Recovered(entry.complete());
                }
            }
        }
        Delivered::Pending
    }

    /// Sequence numbers worth requesting again: missing fragments of not-yet
    /// deliverable, retransmittable (priority-0) frames that parity cannot fix.
    #[must_use]
    pub fn nack_requests(&self, now: Instant) -> Vec<u32> {
        let mut requests = Vec::new();
        for frame in self.frames.values() {
            if frame.completed || frame.priority != 0 || frame.deadline <= now {
                continue;
            }
            let missing: Vec<usize> = frame
                .fragments
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| slot.is_none().then_some(index))
                .collect();
            // A single loss with parity present will self-heal; do not NACK it.
            if missing.len() == 1 && frame.parity.is_some() {
                continue;
            }
            for index in missing {
                requests.push(
                    frame
                        .base_sequence
                        .wrapping_add(u32::try_from(index).unwrap_or(0)),
                );
            }
        }
        requests.sort_unstable();
        requests
    }

    /// Discards frames whose deadline has passed. Completed frames are already
    /// removed on delivery, so only live, incomplete frames remain.
    pub fn drop_expired(&mut self, now: Instant) {
        self.frames.retain(|_, frame| frame.deadline > now);
    }

    #[must_use]
    pub fn pending_frames(&self) -> usize {
        self.frames.len()
    }
}

fn concat_fragments(fragments: &[Option<Vec<u8>>]) -> Vec<u8> {
    fragments
        .iter()
        .flatten()
        .flat_map(|chunk| chunk.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAX_MEDIA_PAYLOAD, MediaKind};

    fn key_frame(frame_id: u32, len: usize, deadline: Instant) -> AccessUnit {
        AccessUnit {
            stream_id: 1,
            frame_id,
            kind: MediaKind::VideoKey,
            deadline,
            bytes: (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect(),
        }
    }

    fn decode(datagram: &OutboundDatagram) -> (Header, Vec<u8>) {
        let (header, payload) = Header::decode(&datagram.bytes).unwrap();
        (header, payload.to_vec())
    }

    #[test]
    fn fec_block_round_trips_and_recovers_each_position() {
        let symbols = vec![b"aaaa".to_vec(), b"bbbb".to_vec(), b"cc".to_vec()];
        let original_len = 4 + 4 + 2;
        let block = FecBlock::build(&symbols, original_len).unwrap();
        // Wire round trip.
        assert_eq!(
            decode_fec_payload(&encode_fec_payload(&block)),
            Some(block.clone())
        );
        assert_eq!(decode_fec_payload(&[]), None);
        assert_eq!(decode_fec_payload(&[0, 4, 0, 3, 0, 0, 0, 10]), None); // parity len mismatch

        // Every single-position loss is recoverable.
        for drop in 0..symbols.len() {
            let mut fragments: Vec<Option<Vec<u8>>> = symbols.iter().cloned().map(Some).collect();
            fragments[drop] = None;
            assert_eq!(block.recover(&mut fragments), Some(drop));
            assert_eq!(fragments[drop].as_ref(), Some(&symbols[drop]));
        }
        // Two losses cannot be recovered.
        let mut two = vec![None, None, Some(symbols[2].clone())];
        assert_eq!(block.recover(&mut two), None);
    }

    #[test]
    fn nack_and_feedback_round_trip() {
        assert_eq!(
            decode_nack_payload(&encode_nack_payload(&[1, 2, 4_000_000])),
            Some(vec![1, 2, 4_000_000])
        );
        let fb = TransportFeedback {
            sent: 100,
            received: 91,
            late: 3,
            rtt: Duration::from_micros(31_250),
        };
        assert_eq!(
            decode_feedback_payload(&encode_feedback_payload(fb)),
            Some(fb)
        );
        assert_eq!(decode_nack_payload(&[0, 2, 0, 0]), None); // truncated body
    }

    #[test]
    fn clean_multi_fragment_frame_reassembles() {
        let now = Instant::now();
        let unit = key_frame(1, MAX_MEDIA_PAYLOAD * 2 + 10, now + Duration::from_secs(1));
        let mut sender = MediaSender::new(7, 0, FecPolicy::PerFrame, 128);
        let datagrams = sender.encode_access_unit(&unit, now).unwrap();
        // Three media fragments + one parity.
        assert_eq!(datagrams.len(), 4);
        let mut receiver = MediaReceiver::new();
        let mut delivered = None;
        for datagram in &datagrams[..3] {
            let (header, payload) = decode(datagram);
            if let Delivered::Clean(bytes) = receiver.accept(header, &payload, now).unwrap() {
                delivered = Some(bytes);
            }
        }
        assert_eq!(delivered, Some(unit.bytes));
    }

    #[test]
    fn single_fragment_loss_is_healed_by_parity_without_a_nack() {
        let now = Instant::now();
        let unit = key_frame(2, MAX_MEDIA_PAYLOAD * 2 + 4, now + Duration::from_secs(1));
        let mut sender = MediaSender::new(7, 0, FecPolicy::PerFrame, 128);
        let datagrams = sender.encode_access_unit(&unit, now).unwrap();
        let mut receiver = MediaReceiver::new();

        // Drop the middle media fragment (index 1); deliver the rest + parity.
        let mut result = Delivered::Pending;
        for (i, datagram) in datagrams.iter().enumerate() {
            if i == 1 {
                continue;
            }
            let (header, payload) = decode(datagram);
            result = receiver.accept(header, &payload, now).unwrap();
        }
        assert_eq!(result, Delivered::Recovered(unit.bytes));
        // Because parity healed it, nothing should be requested.
        assert!(receiver.nack_requests(now).is_empty());
    }

    #[test]
    fn two_losses_trigger_a_nack_and_retransmission_completes_the_frame() {
        let now = Instant::now();
        let unit = key_frame(3, MAX_MEDIA_PAYLOAD * 3 + 7, now + Duration::from_secs(1));
        let mut sender = MediaSender::new(7, 0, FecPolicy::PerFrame, 128);
        let datagrams = sender.encode_access_unit(&unit, now).unwrap();
        // Four media fragments + parity (index 4). Drop media 0 and 2 AND the
        // parity, so only retransmission can complete the frame.
        let parity_index = datagrams.len() - 1;
        let mut receiver = MediaReceiver::new();
        for (i, datagram) in datagrams.iter().enumerate() {
            if i == 0 || i == 2 || i == parity_index {
                continue;
            }
            let (header, payload) = decode(datagram);
            assert_eq!(
                receiver.accept(header, &payload, now).unwrap(),
                Delivered::Pending
            );
        }
        let nacks = receiver.nack_requests(now);
        assert_eq!(nacks.len(), 2);

        // The sender retransmits the still-cached datagrams; the frame completes.
        let mut final_delivery = Delivered::Pending;
        for datagram in sender.on_nack(&nacks, now) {
            let (header, payload) = decode(&datagram);
            final_delivery = receiver.accept(header, &payload, now).unwrap();
        }
        assert_eq!(final_delivery, Delivered::Clean(unit.bytes));
    }

    #[test]
    fn delta_video_losses_are_never_retransmitted() {
        let now = Instant::now();
        let unit = AccessUnit {
            stream_id: 1,
            frame_id: 4,
            kind: MediaKind::VideoDelta,
            deadline: now + Duration::from_secs(1),
            bytes: vec![9; MAX_MEDIA_PAYLOAD * 2],
        };
        let mut sender = MediaSender::new(7, 0, FecPolicy::Off, 128);
        let datagrams = sender.encode_access_unit(&unit, now).unwrap();
        assert_eq!(datagrams.len(), 2); // no parity when FEC is off
        // Ask for both sequences; delta video is not cached, so nothing returns.
        assert!(sender.on_nack(&[0, 1], now).is_empty());
    }

    #[test]
    fn frames_past_their_deadline_are_dropped() {
        let now = Instant::now();
        let unit = key_frame(5, MAX_MEDIA_PAYLOAD * 2, now + Duration::from_millis(10));
        let mut sender = MediaSender::new(7, 0, FecPolicy::PerFrame, 128);
        let datagrams = sender.encode_access_unit(&unit, now).unwrap();
        let mut receiver = MediaReceiver::new();
        let (header, payload) = decode(&datagrams[0]);
        receiver.accept(header, &payload, now).unwrap();
        assert_eq!(receiver.pending_frames(), 1);
        // Well past the deadline: the partial frame is discarded.
        receiver.drop_expired(now + Duration::from_secs(1));
        assert_eq!(receiver.pending_frames(), 0);
    }

    #[test]
    fn lossy_keyframe_stream_is_fully_recovered_by_fec_and_nack() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(2);
        let mut sender = MediaSender::new(11, 0, FecPolicy::PerFrame, 4096);
        let mut receiver = MediaReceiver::new();
        // Deterministic ~1-in-6 datagram loss.
        let mut state = 0x1234_5678_9abc_def0_u64;
        let lost = |state: &mut u64| {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            *state % 6 == 0
        };

        let mut delivered = 0u32;
        let frame_count = 120u32;
        for frame_id in 0..frame_count {
            // Large keyframes so several are multi-fragment and exercise FEC.
            let unit = key_frame(frame_id, MAX_MEDIA_PAYLOAD * 2 + 15, deadline);
            let datagrams = sender.encode_access_unit(&unit, now).unwrap();
            for datagram in &datagrams {
                if lost(&mut state) {
                    continue;
                }
                let (header, payload) = decode(datagram);
                match receiver.accept(header, &payload, now).unwrap() {
                    Delivered::Clean(bytes) | Delivered::Recovered(bytes) => {
                        assert_eq!(bytes, unit.bytes);
                        delivered += 1;
                    }
                    Delivered::Pending | Delivered::Duplicate => {}
                }
            }
            // Retransmit anything FEC could not repair; keyframes are cached.
            let nacks = receiver.nack_requests(now);
            for datagram in sender.on_nack(&nacks, now) {
                if lost(&mut state) {
                    continue;
                }
                let (header, payload) = decode(&datagram);
                if let Delivered::Clean(bytes) | Delivered::Recovered(bytes) =
                    receiver.accept(header, &payload, now).unwrap()
                {
                    assert_eq!(bytes, unit.bytes);
                    delivered += 1;
                }
            }
        }
        // With per-frame parity plus one NACK round, every keyframe within its
        // (generous) deadline is delivered. This is the property a viewer needs:
        // audio and key video do not stall under moderate loss.
        assert_eq!(delivered, frame_count);
    }
}
