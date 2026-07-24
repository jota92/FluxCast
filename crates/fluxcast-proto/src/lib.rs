//! FCDP v0.1 packet primitives.
//!
//! This crate intentionally contains no socket, crypto, or media-codec code.
//! It makes wire-format validation independently testable before transport work
//! begins. Payload encryption and authentication are added by the session layer.

#![forbid(unsafe_code)]

use core::fmt;

/// Two-byte FCDP wire marker (`FC`).
pub const MAGIC: [u8; 2] = *b"FC";
/// The only wire version implemented by this crate.
pub const VERSION_0_1: u8 = 1;
/// Fixed size of a v0.1 header, in bytes.
pub const HEADER_LEN: usize = 37;
/// Size of the header fields authenticated by FCDP's AEAD session layer.
pub const AUTHENTICATED_HEADER_LEN: usize = HEADER_LEN - 2;
/// Maximum recommended UDP datagram size, avoiding IP fragmentation.
pub const DEFAULT_MAX_DATAGRAM_LEN: usize = 1200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    Hello = 1,
    Welcome = 2,
    Media = 3,
    Fec = 4,
    Ack = 5,
    Nack = 6,
    KeyframeRequest = 7,
    Ping = 8,
    Pong = 9,
    Close = 10,
}

impl TryFrom<u8> for PacketType {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Welcome),
            3 => Ok(Self::Media),
            4 => Ok(Self::Fec),
            5 => Ok(Self::Ack),
            6 => Ok(Self::Nack),
            7 => Ok(Self::KeyframeRequest),
            8 => Ok(Self::Ping),
            9 => Ok(Self::Pong),
            10 => Ok(Self::Close),
            other => Err(DecodeError::UnknownPacketType(other)),
        }
    }
}

/// A validated header for a single FCDP datagram.
///
/// All multi-byte integer fields use network byte order (big endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub packet_type: PacketType,
    pub flags: u8,
    pub session_id: u64,
    pub stream_id: u16,
    pub epoch: u16,
    pub sequence_number: u32,
    pub frame_id: u32,
    pub fragment_index: u16,
    pub fragment_count: u16,
    /// `0` is the highest priority.
    pub priority: u8,
    /// Relative expiry from the sender's presentation timestamp, in milliseconds.
    pub deadline_ms: u16,
    pub payload_len: u16,
}

impl Header {
    #[must_use]
    pub const fn new(packet_type: PacketType) -> Self {
        Self {
            version: VERSION_0_1,
            packet_type,
            flags: 0,
            session_id: 0,
            stream_id: 0,
            epoch: 0,
            sequence_number: 0,
            frame_id: 0,
            fragment_index: 0,
            fragment_count: 1,
            priority: 0,
            deadline_ms: 0,
            payload_len: 0,
        }
    }

    /// Encodes this header and payload into `output`.
    ///
    /// # Errors
    ///
    /// Returns an error when the version, fragment metadata, priority, payload
    /// length, or resulting datagram size is invalid.
    pub fn encode(self, payload: &[u8], output: &mut Vec<u8>) -> Result<(), EncodeError> {
        if payload.len() > usize::from(u16::MAX) {
            return Err(EncodeError::PayloadTooLarge);
        }
        if self.version != VERSION_0_1 {
            return Err(EncodeError::UnsupportedVersion(self.version));
        }
        if self.fragment_count == 0 || self.fragment_index >= self.fragment_count {
            return Err(EncodeError::InvalidFragment);
        }
        if self.priority > 3 {
            return Err(EncodeError::InvalidPriority(self.priority));
        }
        if HEADER_LEN + payload.len() > DEFAULT_MAX_DATAGRAM_LEN {
            return Err(EncodeError::DatagramTooLarge);
        }
        let payload_len = u16::try_from(payload.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
        let prefix = self.authenticated_bytes(payload_len);
        output.clear();
        output.reserve(HEADER_LEN + payload.len());
        output.extend_from_slice(&prefix);
        let checksum = crc16(&prefix);
        output.extend_from_slice(&checksum.to_be_bytes());
        output.extend_from_slice(payload);
        Ok(())
    }

    /// Returns the exact header bytes used as AEAD associated data.
    ///
    /// # Errors
    ///
    /// Returns an error when payload length cannot fit in a v0.1 header.
    pub fn associated_data(
        self,
        payload_len: usize,
    ) -> Result<[u8; AUTHENTICATED_HEADER_LEN], EncodeError> {
        let length = u16::try_from(payload_len).map_err(|_| EncodeError::PayloadTooLarge)?;
        Ok(self.authenticated_bytes(length))
    }

    fn authenticated_bytes(self, payload_len: u16) -> [u8; AUTHENTICATED_HEADER_LEN] {
        let mut output = [0_u8; AUTHENTICATED_HEADER_LEN];
        output[..2].copy_from_slice(&MAGIC);
        output[2..6].copy_from_slice(&[self.version, self.packet_type as u8, self.flags, 0]);
        output[6..14].copy_from_slice(&self.session_id.to_be_bytes());
        output[14..16].copy_from_slice(&self.stream_id.to_be_bytes());
        output[16..18].copy_from_slice(&self.epoch.to_be_bytes());
        output[18..22].copy_from_slice(&self.sequence_number.to_be_bytes());
        output[22..26].copy_from_slice(&self.frame_id.to_be_bytes());
        output[26..28].copy_from_slice(&self.fragment_index.to_be_bytes());
        output[28..30].copy_from_slice(&self.fragment_count.to_be_bytes());
        output[30] = self.priority;
        output[31..33].copy_from_slice(&self.deadline_ms.to_be_bytes());
        output[33..35].copy_from_slice(&payload_len.to_be_bytes());
        output
    }

    /// Parses and validates an entire FCDP datagram.
    ///
    /// # Errors
    ///
    /// Returns an error for truncated, unsupported, malformed, corrupt, or
    /// internally inconsistent datagrams.
    ///
    /// # Panics
    ///
    /// This function does not panic for arbitrary datagram input.
    pub fn decode(datagram: &[u8]) -> Result<(Self, &[u8]), DecodeError> {
        if datagram.len() < HEADER_LEN {
            return Err(DecodeError::Truncated);
        }
        if datagram[..2] != MAGIC {
            return Err(DecodeError::InvalidMagic);
        }
        if datagram[2] != VERSION_0_1 {
            return Err(DecodeError::UnsupportedVersion(datagram[2]));
        }
        let expected = u16::from_be_bytes([datagram[HEADER_LEN - 2], datagram[HEADER_LEN - 1]]);
        if crc16(&datagram[..HEADER_LEN - 2]) != expected {
            return Err(DecodeError::InvalidHeaderCrc);
        }
        let header = Self {
            version: datagram[2],
            packet_type: PacketType::try_from(datagram[3])?,
            flags: datagram[4],
            session_id: u64::from_be_bytes(datagram[6..14].try_into().expect("fixed range")),
            stream_id: u16::from_be_bytes(datagram[14..16].try_into().expect("fixed range")),
            epoch: u16::from_be_bytes(datagram[16..18].try_into().expect("fixed range")),
            sequence_number: u32::from_be_bytes(datagram[18..22].try_into().expect("fixed range")),
            frame_id: u32::from_be_bytes(datagram[22..26].try_into().expect("fixed range")),
            fragment_index: u16::from_be_bytes(datagram[26..28].try_into().expect("fixed range")),
            fragment_count: u16::from_be_bytes(datagram[28..30].try_into().expect("fixed range")),
            priority: datagram[30],
            deadline_ms: u16::from_be_bytes(datagram[31..33].try_into().expect("fixed range")),
            payload_len: u16::from_be_bytes(datagram[33..35].try_into().expect("fixed range")),
        };
        if header.fragment_count == 0 || header.fragment_index >= header.fragment_count {
            return Err(DecodeError::InvalidFragment);
        }
        if header.priority > 3 {
            return Err(DecodeError::InvalidPriority(header.priority));
        }
        let payload = &datagram[HEADER_LEN..];
        if payload.len() != usize::from(header.payload_len) {
            return Err(DecodeError::PayloadLengthMismatch);
        }
        Ok((header, payload))
    }
}

/// CRC-16/CCITT-FALSE. It detects accidental header corruption; it is not a
/// security mechanism and must be checked before authenticated decryption.
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    PayloadTooLarge,
    DatagramTooLarge,
    UnsupportedVersion(u8),
    InvalidFragment,
    InvalidPriority(u8),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    InvalidMagic,
    UnsupportedVersion(u8),
    UnknownPacketType(u8),
    InvalidHeaderCrc,
    InvalidFragment,
    InvalidPriority(u8),
    PayloadLengthMismatch,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FCDP encode error: {self:?}")
    }
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FCDP decode error: {self:?}")
    }
}
impl std::error::Error for EncodeError {}
impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_packet_round_trips() {
        let mut h = Header::new(PacketType::Media);
        h.session_id = 42;
        h.stream_id = 1;
        h.sequence_number = 9;
        h.frame_id = 3;
        h.priority = 2;
        h.deadline_ms = 120;
        let mut packet = Vec::new();
        h.encode(b"opus-or-h264", &mut packet).unwrap();
        let (actual, payload) = Header::decode(&packet).unwrap();
        assert_eq!(actual.session_id, 42);
        assert_eq!(actual.payload_len, 12);
        assert_eq!(payload, b"opus-or-h264");
    }
    #[test]
    fn corruption_is_rejected() {
        let mut p = Vec::new();
        Header::new(PacketType::Ping).encode(&[], &mut p).unwrap();
        p[14] ^= 1;
        assert_eq!(Header::decode(&p), Err(DecodeError::InvalidHeaderCrc));
    }
    #[test]
    fn packet_larger_than_mtu_budget_is_rejected() {
        let mut p = Vec::new();
        assert_eq!(
            Header::new(PacketType::Media).encode(&vec![0; DEFAULT_MAX_DATAGRAM_LEN], &mut p),
            Err(EncodeError::DatagramTooLarge)
        );
    }
    #[test]
    fn decoder_rejects_untrusted_bytes_without_panicking() {
        let mut state = 0x4d59_5df4_d0f3_3173_u64;
        for length in 0..=DEFAULT_MAX_DATAGRAM_LEN + 8 {
            let mut packet = vec![0_u8; length];
            for byte in &mut packet {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state.to_le_bytes()[0];
            }
            let _ = Header::decode(&packet);
        }
    }
}
