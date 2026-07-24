//! The deadline-aware building blocks used by `FluxCast` transports.
//!
//! The crate is deliberately codec agnostic: callers submit already encoded
//! access units (for example H.264 NAL access units or Opus packets).

#![forbid(unsafe_code)]

pub mod simulation;
pub use simulation::{ChannelModel, SimulationReport, simulate_delivery};

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use fluxcast_proto::{
    DEFAULT_MAX_DATAGRAM_LEN, DecodeError, EncodeError, HEADER_LEN, Header, PacketType,
};
use fluxcast_security::{SecurityError, Session};
use rand_core::{OsRng, RngCore};

const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_SUCCESS: u16 = 0x0101;
const STUN_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// A server-reflexive UDP candidate discovered through RFC 5389 STUN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerReflexiveCandidate {
    pub address: SocketAddr,
    pub stun_server: SocketAddr,
}

/// ICE candidate kind. Direct candidates are preferred; relay is the safe
/// fallback when a direct authenticated path cannot be nominated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IceCandidateKind {
    Relay,
    ServerReflexive,
    Host,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IceCandidate {
    pub address: SocketAddr,
    pub kind: IceCandidateKind,
    pub priority: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IceCandidatePair {
    pub local: IceCandidate,
    pub remote: IceCandidate,
}

/// Returns pairs in descending ICE preference. Callers must still perform an
/// authenticated connectivity check before nominating any returned pair.
#[must_use]
pub fn ordered_ice_pairs(local: &[IceCandidate], remote: &[IceCandidate]) -> Vec<IceCandidatePair> {
    let mut pairs: Vec<_> = local
        .iter()
        .copied()
        .flat_map(|left| {
            remote.iter().copied().map(move |right| IceCandidatePair {
                local: left,
                remote: right,
            })
        })
        .collect();
    pairs.sort_unstable_by_key(|pair| {
        (
            std::cmp::Reverse(pair.local.kind),
            std::cmp::Reverse(pair.remote.kind),
            std::cmp::Reverse(pair.local.priority),
            std::cmp::Reverse(pair.remote.priority),
        )
    });
    pairs
}

/// Multi-viewer subscription registry for a media relay.
///
/// A control plane authorizes subscriptions and refreshes them before their
/// lease expires. The relay only sees encrypted FCDP datagrams and therefore
/// never needs access to media keys.
#[derive(Debug, Default)]
pub struct RelaySubscriptions {
    subscriptions: HashMap<u64, HashMap<SocketAddr, Instant>>,
    forwarded_packets: u64,
    forwarded_bytes: u64,
    expired_subscriptions: u64,
    send_failures: HashMap<(u64, SocketAddr), u8>,
}

/// A stable snapshot suitable for a metrics endpoint or health probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayMetrics {
    pub active_sessions: usize,
    pub active_subscribers: usize,
    pub forwarded_packets: u64,
    pub forwarded_bytes: u64,
    pub expired_subscriptions: u64,
}

/// Authenticated candidate-path state used for network migration.
///
/// Call [`PathMigration::observe_authenticated_pong`] only after the PONG was
/// accepted by the session AEAD/replay layer. This prevents an off-path party
/// from steering traffic merely by forging reachability reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathMigration {
    active: SocketAddr,
    active_rtt: Duration,
    active_seen: Instant,
    stale_after: Duration,
}

impl PathMigration {
    #[must_use]
    pub fn new(
        active: SocketAddr,
        now: Instant,
        initial_rtt: Duration,
        stale_after: Duration,
    ) -> Self {
        Self {
            active,
            active_rtt: initial_rtt,
            active_seen: now,
            stale_after,
        }
    }
    #[must_use]
    pub const fn active(&self) -> SocketAddr {
        self.active
    }
    #[must_use]
    pub const fn active_rtt(&self) -> Duration {
        self.active_rtt
    }
    #[must_use]
    pub fn active_is_stale(&self, now: Instant) -> bool {
        now.duration_since(self.active_seen) >= self.stale_after
    }
    /// Records authenticated reachability and returns true precisely when the
    /// active path changes. A candidate needs a 20% RTT advantage unless the
    /// old path is stale, avoiding oscillation on ordinary jitter.
    pub fn observe_authenticated_pong(
        &mut self,
        candidate: SocketAddr,
        rtt: Duration,
        now: Instant,
    ) -> bool {
        if candidate == self.active {
            self.active_rtt = rtt;
            self.active_seen = now;
            return false;
        }
        let stale = self.active_is_stale(now);
        let materially_faster = rtt.saturating_mul(5) < self.active_rtt.saturating_mul(4);
        if stale || materially_faster {
            self.active = candidate;
            self.active_rtt = rtt;
            self.active_seen = now;
            return true;
        }
        false
    }
}

impl RelaySubscriptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates or renews one subscriber lease.
    pub fn subscribe(&mut self, session_id: u64, subscriber: SocketAddr, expires_at: Instant) {
        self.subscriptions
            .entry(session_id)
            .or_default()
            .insert(subscriber, expires_at);
    }

    /// Explicitly removes a subscriber, for example after a control-plane close.
    pub fn unsubscribe(&mut self, session_id: u64, subscriber: SocketAddr) -> bool {
        let Some(subscribers) = self.subscriptions.get_mut(&session_id) else {
            return false;
        };
        let removed = subscribers.remove(&subscriber).is_some();
        if subscribers.is_empty() {
            self.subscriptions.remove(&session_id);
        }
        removed
    }

    /// Returns currently leased subscribers and removes expired leases.
    #[must_use]
    pub fn recipients(&mut self, session_id: u64, now: Instant) -> Vec<SocketAddr> {
        let Some(subscribers) = self.subscriptions.get_mut(&session_id) else {
            return Vec::new();
        };
        let before = subscribers.len();
        subscribers.retain(|_, expires_at| *expires_at > now);
        self.expired_subscriptions += (before - subscribers.len()) as u64;
        let mut recipients: Vec<_> = subscribers.keys().copied().collect();
        recipients.sort_unstable();
        if subscribers.is_empty() {
            self.subscriptions.remove(&session_id);
        }
        recipients
    }

    /// Records packets successfully handed to the OS for a subscriber.
    pub fn record_forward(&mut self, bytes: usize) {
        self.forwarded_packets = self.forwarded_packets.saturating_add(1);
        self.forwarded_bytes = self.forwarded_bytes.saturating_add(bytes as u64);
    }
    /// Records a failed send. Three consecutive failures remove the stale
    /// subscriber so a broken viewer cannot keep consuming relay work.
    pub fn record_send_failure(&mut self, session_id: u64, subscriber: SocketAddr) -> bool {
        let key = (session_id, subscriber);
        let failures = self.send_failures.entry(key).or_default();
        *failures = failures.saturating_add(1);
        if *failures >= 3 {
            self.send_failures.remove(&key);
            return self.unsubscribe(session_id, subscriber);
        }
        false
    }

    #[must_use]
    pub fn metrics(&self) -> RelayMetrics {
        RelayMetrics {
            active_sessions: self.subscriptions.len(),
            active_subscribers: self.subscriptions.values().map(HashMap::len).sum(),
            forwarded_packets: self.forwarded_packets,
            forwarded_bytes: self.forwarded_bytes,
            expired_subscriptions: self.expired_subscriptions,
        }
    }
}

/// Queries a STUN server for the public UDP mapping of a locally-bound socket.
///
/// This is the server-reflexive-candidate half of ICE. Candidate pairing,
/// authenticated connectivity checks, TURN allocation, and nomination remain
/// connection-layer responsibilities.
///
/// # Errors
///
/// Returns an I/O error for a timeout/network failure and `InvalidData` for a
/// response that is not a valid response to this transaction.
pub fn discover_server_reflexive_candidate(
    socket: &UdpSocket,
    stun_server: SocketAddr,
    timeout: Duration,
) -> io::Result<ServerReflexiveCandidate> {
    let mut transaction_id = [0_u8; 12];
    OsRng.fill_bytes(&mut transaction_id);
    let request = stun_binding_request(transaction_id);
    socket.send_to(&request, stun_server)?;
    socket.set_read_timeout(Some(timeout))?;
    let mut buffer = [0_u8; 576];
    let received = socket.recv_from(&mut buffer);
    socket.set_read_timeout(None)?;
    let (length, peer) = received?;
    if peer != stun_server {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected STUN peer",
        ));
    }
    let address = parse_stun_binding_success(&buffer[..length], transaction_id)?;
    Ok(ServerReflexiveCandidate {
        address,
        stun_server,
    })
}

fn stun_binding_request(transaction_id: [u8; 12]) -> [u8; 20] {
    let mut packet = [0_u8; 20];
    packet[..2].copy_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());
    packet[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    packet[8..].copy_from_slice(&transaction_id);
    packet
}

fn parse_stun_binding_success(packet: &[u8], transaction_id: [u8; 12]) -> io::Result<SocketAddr> {
    if packet.len() < 20
        || u16::from_be_bytes([packet[0], packet[1]]) != STUN_BINDING_SUCCESS
        || u16::from_be_bytes([packet[2], packet[3]]) as usize + 20 != packet.len()
        || packet[4..8] != STUN_MAGIC_COOKIE.to_be_bytes()
        || packet[8..20] != transaction_id
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid STUN response",
        ));
    }
    let mut cursor = 20;
    while cursor + 4 <= packet.len() {
        let attribute_type = u16::from_be_bytes([packet[cursor], packet[cursor + 1]]);
        let length = u16::from_be_bytes([packet[cursor + 2], packet[cursor + 3]]) as usize;
        let value_start = cursor + 4;
        let value_end = value_start
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "STUN attribute overflow"))?;
        if value_end > packet.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated STUN attribute",
            ));
        }
        if attribute_type == STUN_XOR_MAPPED_ADDRESS {
            return parse_xor_mapped_address(&packet[value_start..value_end], transaction_id);
        }
        cursor = value_end + ((4 - (length % 4)) % 4);
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "STUN response has no XOR-MAPPED-ADDRESS",
    ))
}

fn parse_xor_mapped_address(value: &[u8], transaction_id: [u8; 12]) -> io::Result<SocketAddr> {
    if value.len() < 4 || value[0] != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid XOR-MAPPED-ADDRESS",
        ));
    }
    let port = u16::from_be_bytes([value[2], value[3]]) ^ (STUN_MAGIC_COOKIE >> 16) as u16;
    match value[1] {
        0x01 if value.len() == 8 => {
            let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
            let ip = std::net::Ipv4Addr::new(
                value[4] ^ cookie[0],
                value[5] ^ cookie[1],
                value[6] ^ cookie[2],
                value[7] ^ cookie[3],
            );
            Ok(SocketAddr::from((ip, port)))
        }
        0x02 if value.len() == 20 => {
            let mut mask = [0_u8; 16];
            mask[..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(&transaction_id);
            let mut bytes = [0_u8; 16];
            for (output, (input, xor)) in bytes.iter_mut().zip(value[4..].iter().zip(mask.iter())) {
                *output = input ^ xor;
            }
            Ok(SocketAddr::from((std::net::Ipv6Addr::from(bytes), port)))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported XOR-MAPPED-ADDRESS family",
        )),
    }
}

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

/// One complete H.264 NAL unit in Annex-B form, including its start code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct H264NalUnit<'a> {
    pub bytes: &'a [u8],
    pub nal_type: u8,
    pub kind: MediaKind,
}

/// Splits an Annex-B H.264 byte stream at NAL-unit boundaries.
///
/// This accepts both three- and four-byte start codes. Access-unit grouping
/// (AUD/slice boundary detection) is deliberately left to the capture layer;
/// every returned NAL is independently suitable for deadline-aware transport.
///
/// # Errors
///
/// Returns an error if the stream contains no start code or an empty NAL.
pub fn split_h264_annex_b(input: &[u8]) -> Result<Vec<H264NalUnit<'_>>, CoreError> {
    let starts = annex_b_start_codes(input);
    if starts.is_empty() {
        return Err(CoreError::InvalidMedia);
    }
    let mut units = Vec::with_capacity(starts.len());
    for (index, (start, prefix)) in starts.iter().copied().enumerate() {
        let end = starts
            .get(index + 1)
            .map_or(input.len(), |(offset, _)| *offset);
        let payload_start = start + prefix;
        if payload_start >= end {
            return Err(CoreError::InvalidMedia);
        }
        let nal_type = input[payload_start] & 0x1f;
        let kind = if nal_type == 5 {
            MediaKind::VideoKey
        } else {
            MediaKind::VideoDelta
        };
        units.push(H264NalUnit {
            bytes: &input[start..end],
            nal_type,
            kind,
        });
    }
    Ok(units)
}

fn annex_b_start_codes(input: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= input.len() {
        let prefix = if input[index..].starts_with(&[0, 0, 1]) {
            3
        } else if index + 4 <= input.len() && input[index..].starts_with(&[0, 0, 0, 1]) {
            4
        } else {
            index += 1;
            continue;
        };
        starts.push((index, prefix));
        index += prefix;
    }
    starts
}

/// Validates and splits an Ogg Opus stream into complete pages for transport.
/// Keeping page boundaries lets a receiver concatenate the recovered bytes into
/// a standards-compliant `.opus` file without needing codec keys at the relay.
///
/// # Errors
///
/// Returns an error for malformed capture patterns, lacing values, or partial pages.
pub fn split_ogg_pages(input: &[u8]) -> Result<Vec<&[u8]>, CoreError> {
    let mut pages = Vec::new();
    let mut cursor = 0;
    while cursor < input.len() {
        if input.get(cursor..cursor + 4) != Some(b"OggS") || input.get(cursor + 4) != Some(&0) {
            return Err(CoreError::InvalidMedia);
        }
        let segments = *input.get(cursor + 26).ok_or(CoreError::InvalidMedia)? as usize;
        let lacing_start = cursor + 27;
        let body_start = lacing_start + segments;
        let lacing = input
            .get(lacing_start..body_start)
            .ok_or(CoreError::InvalidMedia)?;
        let body_len: usize = lacing.iter().map(|value| usize::from(*value)).sum();
        let end = body_start
            .checked_add(body_len)
            .ok_or(CoreError::InvalidMedia)?;
        if end > input.len() {
            return Err(CoreError::InvalidMedia);
        }
        pages.push(&input[cursor..end]);
        cursor = end;
    }
    if pages.is_empty() {
        return Err(CoreError::InvalidMedia);
    }
    Ok(pages)
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

/// Receiver report used by the adaptive sender. Counts cover one reporting
/// interval; `late` means packets that arrived after their media deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportFeedback {
    pub sent: u32,
    pub received: u32,
    pub late: u32,
    pub rtt: Duration,
}

/// Encoder-facing action derived from an authenticated receiver report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptationDecision {
    pub target_bps: u64,
    pub request_keyframe: bool,
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
    /// Updates the target from a receiver report and requests a keyframe after
    /// material video loss, so a decoder does not wait for a damaged GOP.
    #[must_use]
    pub fn observe_feedback(
        &mut self,
        feedback: TransportFeedback,
        stable_for: Duration,
    ) -> AdaptationDecision {
        // A report interval is bounded to u16 for wire compatibility; clamp
        // pathological counters before converting to the estimator's f32 API.
        let sent = u16::try_from(feedback.sent.max(1)).unwrap_or(u16::MAX);
        let received = u16::try_from(feedback.received)
            .unwrap_or(u16::MAX)
            .min(sent);
        let lost = sent.saturating_sub(received);
        let loss_rate = f32::from(lost) / f32::from(sent);
        let late_rate =
            f32::from(u16::try_from(feedback.late).unwrap_or(u16::MAX).min(sent)) / f32::from(sent);
        self.observe(loss_rate, late_rate, stable_for);
        AdaptationDecision {
            target_bps: self.current,
            request_keyframe: loss_rate > 0.05 || late_rate > 0.05,
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

/// UDP transport which accepts only authenticated encrypted FCDP datagrams.
/// The caller owns session establishment and rotates the session for each epoch.
pub struct SecureUdpEndpoint {
    endpoint: UdpEndpoint,
    session: Session,
}

impl SecureUdpEndpoint {
    /// Binds a nonblocking secure UDP endpoint.
    ///
    /// # Errors
    ///
    /// Returns the underlying UDP bind or configuration error.
    pub fn bind(address: SocketAddr, session: Session) -> io::Result<Self> {
        Ok(Self {
            endpoint: UdpEndpoint::bind(address)?,
            session,
        })
    }
    /// Encrypts and sends one FCDP payload.
    ///
    /// # Errors
    ///
    /// Returns a security error for invalid packets or an I/O error for send failures.
    pub fn send(
        &self,
        destination: SocketAddr,
        header: Header,
        payload: &[u8],
    ) -> Result<usize, SecureTransportError> {
        Ok(self
            .endpoint
            .send(destination, &self.session.seal_fcdp(header, payload)?)?)
    }
    /// Receives, authenticates, replay-checks, and decrypts one FCDP payload.
    ///
    /// # Errors
    ///
    /// Returns malformed, unauthenticated, replayed, or I/O datagrams as errors.
    pub fn receive(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<Option<(Header, Vec<u8>, SocketAddr)>, SecureTransportError> {
        match self.endpoint.socket.recv_from(buffer) {
            Ok((length, peer)) => {
                let (header, payload) = self.session.open_fcdp(&buffer[..length])?;
                Ok(Some((header, payload, peer)))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(SecureTransportError::Io(error)),
        }
    }
    /// Returns the bound address.
    ///
    /// # Errors
    ///
    /// Returns the underlying socket query error.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.endpoint.local_addr()
    }
}

#[derive(Debug)]
pub enum SecureTransportError {
    Io(io::Error),
    Security(SecurityError),
}
impl From<io::Error> for SecureTransportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<SecurityError> for SecureTransportError {
    fn from(value: SecurityError) -> Self {
        Self::Security(value)
    }
}
impl std::fmt::Display for SecureTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "secure UDP transport error: {self:?}")
    }
}
impl std::error::Error for SecureTransportError {}

#[derive(Debug)]
pub enum CoreError {
    Encode(EncodeError),
    Decode(DecodeError),
    TooManyFragments,
    DeadlineOverflow,
    NotMedia,
    InconsistentFrame,
    InvalidMedia,
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
    #[test]
    fn feedback_reduces_rate_and_requests_a_recovery_keyframe() {
        let mut controller = BitrateController::new(1_000_000, 100_000, 2_000_000);
        let decision = controller.observe_feedback(
            TransportFeedback {
                sent: 100,
                received: 90,
                late: 2,
                rtt: Duration::from_millis(30),
            },
            Duration::ZERO,
        );
        assert_eq!(decision.target_bps, 800_000);
        assert!(decision.request_keyframe);
    }
    #[test]
    fn parses_rfc5389_xor_mapped_ipv4_address() {
        let transaction_id = [7_u8; 12];
        let mut response = vec![0_u8; 32];
        response[..2].copy_from_slice(&STUN_BINDING_SUCCESS.to_be_bytes());
        response[2..4].copy_from_slice(&12_u16.to_be_bytes());
        response[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        response[8..20].copy_from_slice(&transaction_id);
        response[20..22].copy_from_slice(&STUN_XOR_MAPPED_ADDRESS.to_be_bytes());
        response[22..24].copy_from_slice(&8_u16.to_be_bytes());
        response[25] = 0x01;
        let port = 50_000_u16;
        response[26..28].copy_from_slice(&(port ^ (STUN_MAGIC_COOKIE >> 16) as u16).to_be_bytes());
        let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
        let address = [203_u8, 0, 113, 7];
        for (output, (input, xor)) in response[28..].iter_mut().zip(address.iter().zip(cookie)) {
            *output = input ^ xor;
        }
        assert_eq!(
            parse_stun_binding_success(&response, transaction_id).unwrap(),
            "203.0.113.7:50000".parse().unwrap()
        );
    }
    #[test]
    fn relay_subscriptions_fan_out_and_expire() {
        let now = Instant::now();
        let first = "127.0.0.1:30001".parse().unwrap();
        let second = "127.0.0.1:30002".parse().unwrap();
        let mut relay = RelaySubscriptions::new();
        relay.subscribe(42, first, now + Duration::from_secs(1));
        relay.subscribe(42, second, now + Duration::from_millis(1));
        assert_eq!(relay.recipients(42, now), vec![first, second]);
        relay.record_forward(1200);
        relay.record_forward(100);
        assert_eq!(relay.metrics().forwarded_bytes, 1300);
        assert!(
            relay
                .recipients(42, now + Duration::from_secs(2))
                .is_empty()
        );
        let metrics = relay.metrics();
        assert_eq!(metrics.active_sessions, 0);
        assert_eq!(metrics.expired_subscriptions, 2);
    }
    #[test]
    fn migration_switches_only_for_stale_or_materially_better_path() {
        let now = Instant::now();
        let primary = "127.0.0.1:40001".parse().unwrap();
        let backup = "127.0.0.1:40002".parse().unwrap();
        let mut paths = PathMigration::new(
            primary,
            now,
            Duration::from_millis(100),
            Duration::from_secs(3),
        );
        assert!(!paths.observe_authenticated_pong(
            backup,
            Duration::from_millis(90),
            now + Duration::from_millis(10)
        ));
        assert!(paths.observe_authenticated_pong(
            backup,
            Duration::from_millis(70),
            now + Duration::from_millis(20)
        ));
        assert_eq!(paths.active(), backup);
        assert!(!paths.observe_authenticated_pong(
            primary,
            Duration::from_millis(100),
            now + Duration::from_millis(30)
        ));
        assert!(paths.observe_authenticated_pong(
            primary,
            Duration::from_millis(100),
            now + Duration::from_secs(4)
        ));
        assert_eq!(paths.active(), primary);
    }
    #[test]
    fn ice_orders_direct_candidates_before_turn_relay() {
        let host = IceCandidate {
            address: "127.0.0.1:5000".parse().unwrap(),
            kind: IceCandidateKind::Host,
            priority: 1,
        };
        let relay = IceCandidate {
            address: "127.0.0.1:3478".parse().unwrap(),
            kind: IceCandidateKind::Relay,
            priority: 1,
        };
        let pairs = ordered_ice_pairs(&[relay, host], &[relay, host]);
        assert_eq!(pairs[0].local.kind, IceCandidateKind::Host);
        assert_eq!(pairs[0].remote.kind, IceCandidateKind::Host);
        assert_eq!(pairs.last().unwrap().local.kind, IceCandidateKind::Relay);
    }
    #[test]
    fn splits_annex_b_h264_and_preserves_keyframe_priority() {
        let source = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x65, 2, 3];
        let units = split_h264_annex_b(&source).unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].nal_type, 7);
        assert_eq!(units[1].kind, MediaKind::VideoKey);
        assert_eq!(units[1].bytes, &[0, 0, 1, 0x65, 2, 3]);
    }
    #[test]
    fn splits_complete_ogg_pages() {
        let mut page = vec![0_u8; 28];
        page[..4].copy_from_slice(b"OggS");
        page[26] = 1;
        page[27] = 3;
        page.extend_from_slice(b"abc");
        assert_eq!(split_ogg_pages(&page).unwrap(), vec![page.as_slice()]);
    }
}
