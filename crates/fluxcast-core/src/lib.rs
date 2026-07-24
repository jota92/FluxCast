//! The deadline-aware building blocks used by `FluxCast` transports.
//!
//! The crate is deliberately codec agnostic: callers submit already encoded
//! access units (for example H.264 NAL access units or Opus packets).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use fluxcast_proto::{
    DEFAULT_MAX_DATAGRAM_LEN, DecodeError, EncodeError, HEADER_LEN, Header, PacketType,
};

/// Largest payload that fits in the default non-fragmenting UDP budget.
pub const MAX_MEDIA_PAYLOAD: usize = DEFAULT_MAX_DATAGRAM_LEN - HEADER_LEN;

/// Media semantics used by scheduling and recovery decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    VideoKey,
    VideoDelta,
    Metadata,
}

impl MediaKind {
    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::Audio | Self::VideoKey => 0,
            Self::VideoDelta => 2,
            Self::Metadata => 3,
        }
    }

    #[must_use]
    pub const fn is_retransmittable(self) -> bool {
        matches!(self, Self::Audio | Self::VideoKey)
    }
}

/// A complete encoded access unit before UDP fragmentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessUnit {
    pub stream_id: u16,
    pub frame_id: u32,
    pub kind: MediaKind,
    pub deadline: Instant,
    pub bytes: Vec<u8>,
}

/// A fully encoded datagram plus scheduling metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundDatagram {
    pub sequence_number: u32,
    pub kind: MediaKind,
    pub deadline: Instant,
    pub bytes: Vec<u8>,
}

/// Splits an access unit into independently validated FCDP MEDIA datagrams.
///
/// # Errors
///
/// Returns an error when the access unit requires more than `u16::MAX`
/// fragments or when FCDP encoding rejects a fragment.
pub fn fragment_access_unit(
    session_id: u64,
    epoch: u16,
    next_sequence: &mut u32,
    unit: &AccessUnit,
    now: Instant,
) -> Result<Vec<OutboundDatagram>, CoreError> {
    let fragments = unit.bytes.chunks(MAX_MEDIA_PAYLOAD).collect::<Vec<_>>();
    let fragments = if fragments.is_empty() {
        vec![&[][..]]
    } else {
        fragments
    };
    let count = u16::try_from(fragments.len()).map_err(|_| CoreError::TooManyFragments)?;
    let remaining = unit
        .deadline
        .saturating_duration_since(now)
        .as_millis()
        .min(u128::from(u16::MAX));
    let deadline_ms = u16::try_from(remaining).map_err(|_| CoreError::DeadlineOverflow)?;
    let mut packets = Vec::with_capacity(fragments.len());
    for (index, payload) in fragments.into_iter().enumerate() {
        let mut header = Header::new(PacketType::Media);
        header.session_id = session_id;
        header.stream_id = unit.stream_id;
        header.epoch = epoch;
        header.sequence_number = *next_sequence;
        header.frame_id = unit.frame_id;
        header.fragment_index = u16::try_from(index).map_err(|_| CoreError::TooManyFragments)?;
        header.fragment_count = count;
        header.priority = unit.kind.priority();
        header.deadline_ms = deadline_ms;
        let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
        header.encode(payload, &mut bytes)?;
        packets.push(OutboundDatagram {
            sequence_number: *next_sequence,
            kind: unit.kind,
            deadline: unit.deadline,
            bytes,
        });
        *next_sequence = next_sequence.wrapping_add(1);
    }
    Ok(packets)
}

/// Priority queue that never sends media once it has missed its deadline.
#[derive(Debug, Default)]
pub struct DeadlineQueue {
    entries: Vec<OutboundDatagram>,
}

impl DeadlineQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    pub fn push(&mut self, packet: OutboundDatagram) {
        self.entries.push(packet);
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn pop_ready(&mut self, now: Instant) -> Option<OutboundDatagram> {
        self.entries.retain(|entry| entry.deadline > now);
        let index = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| (entry.kind.priority(), entry.deadline))
            .map(|(index, _)| index)?;
        Some(self.entries.swap_remove(index))
    }
}

/// XOR parity for a small block of variable-size fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XorParity {
    pub fragment_count: u16,
    pub lengths: Vec<u16>,
    pub bytes: Vec<u8>,
}

impl XorParity {
    /// Creates one parity fragment. This is suitable only for recovery of one
    /// missing fragment; larger loss patterns need a future FEC scheme.
    #[must_use]
    pub fn encode(fragments: &[Vec<u8>]) -> Option<Self> {
        if fragments.is_empty()
            || fragments.len() > usize::from(u16::MAX)
            || fragments
                .iter()
                .any(|fragment| fragment.len() > usize::from(u16::MAX))
        {
            return None;
        }
        let longest = fragments.iter().map(Vec::len).max()?;
        let mut bytes = vec![0; longest];
        for fragment in fragments {
            for (index, byte) in fragment.iter().enumerate() {
                bytes[index] ^= byte;
            }
        }
        Some(Self {
            fragment_count: u16::try_from(fragments.len()).ok()?,
            lengths: fragments
                .iter()
                .map(|fragment| u16::try_from(fragment.len()).ok())
                .collect::<Option<Vec<_>>>()?,
            bytes,
        })
    }

    /// Reconstructs the sole missing fragment, returning `None` if zero or
    /// more than one fragment is absent or if metadata is inconsistent.
    #[must_use]
    pub fn recover_one(&self, fragments: &[Option<Vec<u8>>]) -> Option<(usize, Vec<u8>)> {
        if fragments.len() != usize::from(self.fragment_count)
            || self.lengths.len() != fragments.len()
        {
            return None;
        }
        let missing = fragments
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.is_none().then_some(index))
            .collect::<Vec<_>>();
        if missing.len() != 1 {
            return None;
        }
        let index = missing[0];
        let expected = usize::from(self.lengths[index]);
        if expected > self.bytes.len() {
            return None;
        }
        let mut recovered = self.bytes.clone();
        for present in fragments.iter().flatten() {
            if present.len() > recovered.len() {
                return None;
            }
            for (offset, byte) in present.iter().enumerate() {
                recovered[offset] ^= byte;
            }
        }
        recovered.truncate(expected);
        Some((index, recovered))
    }
}

#[derive(Debug)]
struct PendingFrame {
    deadline: Instant,
    total: u16,
    fragments: Vec<Option<Vec<u8>>>,
}

/// Reassembles out-of-order media. Late incomplete frames are discarded.
#[derive(Debug, Default)]
pub struct Reassembler {
    frames: HashMap<(u16, u32), PendingFrame>,
}

impl Reassembler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            frames: HashMap::new(),
        }
    }
    /// Adds one decoded MEDIA payload. Returns an access-unit byte vector once all fragments arrive.
    ///
    /// # Errors
    ///
    /// Returns an error when the packet is not media or does not match the
    /// metadata already observed for its frame.
    pub fn push(
        &mut self,
        header: Header,
        payload: &[u8],
        now: Instant,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        if header.packet_type != PacketType::Media {
            return Err(CoreError::NotMedia);
        }
        self.drop_expired(now);
        let deadline = now
            .checked_add(Duration::from_millis(u64::from(header.deadline_ms)))
            .unwrap_or(now);
        let key = (header.stream_id, header.frame_id);
        let entry = self.frames.entry(key).or_insert_with(|| PendingFrame {
            deadline,
            total: header.fragment_count,
            fragments: vec![None; usize::from(header.fragment_count)],
        });
        if entry.total != header.fragment_count || entry.deadline <= now {
            self.frames.remove(&key);
            return Err(CoreError::InconsistentFrame);
        }
        entry.fragments[usize::from(header.fragment_index)].get_or_insert_with(|| payload.to_vec());
        if entry.fragments.iter().any(Option::is_none) {
            return Ok(None);
        }
        let frame = self
            .frames
            .remove(&key)
            .ok_or(CoreError::InconsistentFrame)?;
        Ok(Some(
            frame.fragments.into_iter().flatten().flatten().collect(),
        ))
    }
    pub fn drop_expired(&mut self, now: Instant) {
        self.frames.retain(|_, frame| frame.deadline > now);
    }
}

/// Bounded retransmission cache. It stores only audio and key video packets.
#[derive(Debug)]
pub struct RetransmitWindow {
    packets: BTreeMap<u32, OutboundDatagram>,
    capacity: usize,
}

impl RetransmitWindow {
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            packets: BTreeMap::new(),
            capacity,
        }
    }
    pub fn insert(&mut self, packet: OutboundDatagram) {
        if !packet.kind.is_retransmittable() {
            return;
        }
        self.packets.insert(packet.sequence_number, packet);
        while self.packets.len() > self.capacity {
            let Some(first) = self.packets.first_key_value().map(|(key, _)| *key) else {
                break;
            };
            self.packets.remove(&first);
        }
    }
    #[must_use]
    pub fn get_before_deadline(&self, sequence: u32, now: Instant) -> Option<&OutboundDatagram> {
        self.packets
            .get(&sequence)
            .filter(|packet| packet.deadline > now)
    }
}

/// Conservative AIMD bitrate estimator. Units are bits per second.
#[derive(Debug, Clone)]
pub struct BitrateController {
    current: u64,
    floor: u64,
    ceiling: u64,
}

impl BitrateController {
    #[must_use]
    pub const fn new(initial_bps: u64, floor_bps: u64, ceiling_bps: u64) -> Self {
        Self {
            current: initial_bps,
            floor: floor_bps,
            ceiling: ceiling_bps,
        }
    }
    #[must_use]
    pub const fn current_bps(&self) -> u64 {
        self.current
    }
    pub fn observe(&mut self, loss_rate: f32, late_rate: f32, stable_for: Duration) {
        if loss_rate > 0.01 || late_rate > 0.01 {
            self.current = self
                .current
                .saturating_mul(80)
                .saturating_div(100)
                .max(self.floor);
        } else if stable_for >= Duration::from_secs(3) {
            self.current = self
                .current
                .saturating_add((self.current / 20).max(1))
                .min(self.ceiling);
        }
    }
}

/// A blocking UDP endpoint intended for CLI tools, integration tests, and the
/// first native SDK. Production event-loop adapters may wrap this type.
#[derive(Debug)]
pub struct UdpEndpoint {
    socket: UdpSocket,
}

impl UdpEndpoint {
    /// Binds a UDP endpoint and configures nonblocking operation.
    ///
    /// # Errors
    ///
    /// Returns the underlying socket bind or configuration error.
    pub fn bind(address: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(address)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket })
    }
    /// Sends one already encoded datagram.
    ///
    /// # Errors
    ///
    /// Returns the underlying UDP send error.
    pub fn send(&self, destination: SocketAddr, packet: &[u8]) -> io::Result<usize> {
        self.socket.send_to(packet, destination)
    }
    /// Receives and validates one datagram, or `None` when no datagram is ready.
    ///
    /// # Errors
    ///
    /// Returns I/O errors and malformed FCDP datagrams.
    pub fn receive(&self, buffer: &mut [u8]) -> io::Result<Option<(Header, usize, SocketAddr)>> {
        match self.socket.recv_from(buffer) {
            Ok((length, peer)) => Header::decode(&buffer[..length])
                .map(|(header, _)| Some((header, length, peer)))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
    /// Returns the bound socket address.
    ///
    /// # Errors
    ///
    /// Returns the underlying socket query error.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

#[derive(Debug)]
pub enum CoreError {
    Encode(EncodeError),
    Decode(DecodeError),
    TooManyFragments,
    DeadlineOverflow,
    NotMedia,
    InconsistentFrame,
}
impl From<EncodeError> for CoreError {
    fn from(value: EncodeError) -> Self {
        Self::Encode(value)
    }
}
impl From<DecodeError> for CoreError {
    fn from(value: DecodeError) -> Self {
        Self::Decode(value)
    }
}
impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FluxCast core error: {self:?}")
    }
}
impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reassembles_out_of_order_fragments() {
        let now = Instant::now();
        let unit = AccessUnit {
            stream_id: 1,
            frame_id: 4,
            kind: MediaKind::VideoKey,
            deadline: now + Duration::from_secs(1),
            bytes: vec![8; MAX_MEDIA_PAYLOAD + 3],
        };
        let mut sequence = 0;
        let packets = fragment_access_unit(9, 1, &mut sequence, &unit, now).unwrap();
        let mut r = Reassembler::new();
        let (_, second_payload) = Header::decode(&packets[1].bytes).unwrap();
        let second_header = Header::decode(&packets[1].bytes).unwrap().0;
        assert!(
            r.push(second_header, second_payload, now)
                .unwrap()
                .is_none()
        );
        let (first_header, first_payload) = Header::decode(&packets[0].bytes).unwrap();
        assert_eq!(
            r.push(first_header, first_payload, now).unwrap(),
            Some(unit.bytes)
        );
    }
    #[test]
    fn xor_recovers_one_missing_fragment() {
        let source = vec![b"one".to_vec(), b"two-two".to_vec(), b"three".to_vec()];
        let parity = XorParity::encode(&source).unwrap();
        let got = parity
            .recover_one(&[Some(source[0].clone()), None, Some(source[2].clone())])
            .unwrap();
        assert_eq!(got, (1, source[1].clone()));
    }
    #[test]
    fn deadline_queue_discards_old_media() {
        let now = Instant::now();
        let mut queue = DeadlineQueue::new();
        queue.push(OutboundDatagram {
            sequence_number: 1,
            kind: MediaKind::Audio,
            deadline: now.checked_sub(Duration::from_millis(1)).unwrap(),
            bytes: vec![],
        });
        assert!(queue.pop_ready(now).is_none());
    }
    #[test]
    fn controller_decreases_fast_and_increases_slowly() {
        let mut c = BitrateController::new(1_000, 500, 2_000);
        c.observe(0.02, 0.0, Duration::ZERO);
        assert_eq!(c.current_bps(), 800);
        c.observe(0.0, 0.0, Duration::from_secs(3));
        assert_eq!(c.current_bps(), 840);
    }
}
