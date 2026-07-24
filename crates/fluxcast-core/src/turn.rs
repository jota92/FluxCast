//! A minimal TURN (RFC 5766 / RFC 8489) client for relayed NAT traversal.
//!
//! `FluxCast`'s v0 topology is Relay-centric, but a direct authenticated path is
//! always preferred. When ICE cannot nominate a direct pair, a TURN allocation
//! provides the safe fallback transport address. This module implements the
//! long-term-credential Allocate, `CreatePermission`, `ChannelBind`, `Refresh`,
//! and `ChannelData` exchanges over a blocking [`UdpSocket`], reusing audited
//! `RustCrypto` primitives (MD5 + HMAC-SHA1) for `MESSAGE-INTEGRITY`.
//!
//! The client speaks TURN only; it never inspects the relayed FCDP payload,
//! which stays end-to-end encrypted by the session layer.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use rand_core::{OsRng, RngCore};
use sha1::Sha1;

const MAGIC_COOKIE: u32 = 0x2112_A442;

// STUN/TURN method + class encodings (message type = method | class bits).
const ALLOCATE_REQUEST: u16 = 0x0003;
const ALLOCATE_SUCCESS: u16 = 0x0103;
const ALLOCATE_ERROR: u16 = 0x0113;
const REFRESH_REQUEST: u16 = 0x0004;
const REFRESH_SUCCESS: u16 = 0x0104;
const CREATE_PERMISSION_REQUEST: u16 = 0x0008;
const CREATE_PERMISSION_SUCCESS: u16 = 0x0108;
const CHANNEL_BIND_REQUEST: u16 = 0x0009;
const CHANNEL_BIND_SUCCESS: u16 = 0x0109;
const DATA_INDICATION: u16 = 0x0017;

// Attribute types.
const ATTR_USERNAME: u16 = 0x0006;
const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const ATTR_ERROR_CODE: u16 = 0x0009;
const ATTR_LIFETIME: u16 = 0x000d;
const ATTR_XOR_PEER_ADDRESS: u16 = 0x0012;
const ATTR_DATA: u16 = 0x0013;
const ATTR_REALM: u16 = 0x0014;
const ATTR_NONCE: u16 = 0x0015;
const ATTR_XOR_RELAYED_ADDRESS: u16 = 0x0016;
const ATTR_REQUESTED_TRANSPORT: u16 = 0x0019;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const ATTR_CHANNEL_NUMBER: u16 = 0x000c;

const REQUESTED_TRANSPORT_UDP: u8 = 17;

/// The result of a successful TURN allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    /// The relayed transport address peers send to.
    pub relayed: SocketAddr,
    /// This client's server-reflexive address, as seen by the TURN server.
    pub mapped: SocketAddr,
    /// Allocation lifetime granted by the server, in seconds.
    pub lifetime_secs: u32,
}

/// Long-term TURN credentials, matching coturn's `lt-cred-mech`.
#[derive(Debug, Clone)]
pub struct TurnCredentials {
    pub username: String,
    pub password: String,
}

/// A blocking TURN client bound to one UDP socket and one server.
#[derive(Debug)]
pub struct TurnClient {
    socket: UdpSocket,
    server: SocketAddr,
    credentials: TurnCredentials,
    realm: String,
    nonce: Vec<u8>,
    timeout: Duration,
}

impl TurnClient {
    /// Binds a client. The socket may be pre-bound; the server is the TURN
    /// listener (typically UDP `:3478`).
    ///
    /// # Errors
    ///
    /// Returns the socket configuration error.
    pub fn new(
        socket: UdpSocket,
        server: SocketAddr,
        credentials: TurnCredentials,
        timeout: Duration,
    ) -> io::Result<Self> {
        socket.set_read_timeout(Some(timeout))?;
        Ok(Self {
            socket,
            server,
            credentials,
            realm: String::new(),
            nonce: Vec::new(),
            timeout,
        })
    }

    /// The long-term-credential key: `MD5(username ":" realm ":" password)`.
    fn integrity_key(&self) -> [u8; 16] {
        let mut hasher = Md5::new();
        hasher.update(self.credentials.username.as_bytes());
        hasher.update(b":");
        hasher.update(self.realm.as_bytes());
        hasher.update(b":");
        hasher.update(self.credentials.password.as_bytes());
        hasher.finalize().into()
    }

    /// Requests a UDP relay allocation, performing the 401 challenge exchange.
    ///
    /// # Errors
    ///
    /// Returns an I/O error on timeout, or `InvalidData` when the server rejects
    /// the request or returns a malformed response.
    pub fn allocate(&mut self) -> io::Result<Allocation> {
        // First unauthenticated Allocate to obtain realm and nonce.
        let request = self.build_allocate(false);
        let response = self.transact(&request)?;
        let message = Message::parse(&response)?;
        if message.kind == ALLOCATE_ERROR {
            self.absorb_challenge(&message)?;
        } else if message.kind == ALLOCATE_SUCCESS {
            return Self::parse_allocation(&message);
        }

        // Authenticated retry with USERNAME/REALM/NONCE/MESSAGE-INTEGRITY.
        let request = self.build_allocate(true);
        let response = self.transact(&request)?;
        let message = Message::parse(&response)?;
        if message.kind != ALLOCATE_SUCCESS {
            return Err(server_error("TURN allocate rejected", &message));
        }
        Self::parse_allocation(&message)
    }

    /// Installs a send permission for `peer` so its packets reach this client.
    ///
    /// # Errors
    ///
    /// Returns an error on timeout or a non-success response.
    pub fn create_permission(&mut self, peer: SocketAddr) -> io::Result<()> {
        let mut builder = MessageBuilder::new(CREATE_PERMISSION_REQUEST);
        let txid = builder.transaction_id;
        builder.add_xor_address(ATTR_XOR_PEER_ADDRESS, peer, &txid);
        self.finish_authenticated(&mut builder);
        let response = self.transact(&builder.finish())?;
        let message = Message::parse(&response)?;
        if message.kind != CREATE_PERMISSION_SUCCESS {
            return Err(server_error("TURN create-permission rejected", &message));
        }
        Ok(())
    }

    /// Binds `peer` to `channel` (0x4000–0x7FFE) for efficient `ChannelData`.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid channel, timeout, or non-success.
    pub fn channel_bind(&mut self, peer: SocketAddr, channel: u16) -> io::Result<()> {
        if !(0x4000..=0x7ffe).contains(&channel) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TURN channel number must be in 0x4000..=0x7ffe",
            ));
        }
        let mut builder = MessageBuilder::new(CHANNEL_BIND_REQUEST);
        let txid = builder.transaction_id;
        let mut channel_value = [0u8; 4];
        channel_value[..2].copy_from_slice(&channel.to_be_bytes());
        builder.add_attribute(ATTR_CHANNEL_NUMBER, &channel_value);
        builder.add_xor_address(ATTR_XOR_PEER_ADDRESS, peer, &txid);
        self.finish_authenticated(&mut builder);
        let response = self.transact(&builder.finish())?;
        let message = Message::parse(&response)?;
        if message.kind != CHANNEL_BIND_SUCCESS {
            return Err(server_error("TURN channel-bind rejected", &message));
        }
        Ok(())
    }

    /// Refreshes the allocation for another `lifetime` seconds. `0` releases it.
    ///
    /// # Errors
    ///
    /// Returns an error on timeout or a non-success response.
    pub fn refresh(&mut self, lifetime: u32) -> io::Result<u32> {
        let mut builder = MessageBuilder::new(REFRESH_REQUEST);
        builder.add_attribute(ATTR_LIFETIME, &lifetime.to_be_bytes());
        self.finish_authenticated(&mut builder);
        let response = self.transact(&builder.finish())?;
        let message = Message::parse(&response)?;
        if message.kind != REFRESH_SUCCESS {
            return Err(server_error("TURN refresh rejected", &message));
        }
        Ok(message
            .attribute(ATTR_LIFETIME)
            .and_then(|value| value.get(..4))
            .map_or(lifetime, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    }

    /// Sends `data` to the peer bound to `channel` via a `ChannelData` message.
    ///
    /// # Errors
    ///
    /// Returns the underlying send error.
    pub fn send_channel_data(&self, channel: u16, data: &[u8]) -> io::Result<usize> {
        let mut frame = Vec::with_capacity(4 + data.len());
        frame.extend_from_slice(&channel.to_be_bytes());
        frame.extend_from_slice(&u16::try_from(data.len()).unwrap_or(u16::MAX).to_be_bytes());
        frame.extend_from_slice(data);
        self.socket.send_to(&frame, self.server)
    }

    /// Receives one relayed `ChannelData` payload, returning `(channel, bytes)`.
    /// STUN/TURN control messages that arrive are skipped.
    ///
    /// # Errors
    ///
    /// Returns a timeout or other I/O error, or `InvalidData` for a malformed
    /// `ChannelData` frame.
    pub fn recv_channel_data(&self, buffer: &mut [u8]) -> io::Result<(u16, Vec<u8>)> {
        loop {
            let (len, from) = self.socket.recv_from(buffer)?;
            if from != self.server {
                continue;
            }
            let frame = &buffer[..len];
            let Some(&first) = frame.first() else {
                continue;
            };
            // ChannelData channel numbers begin at 0x40; STUN types do not.
            if !(0x40..=0x7f).contains(&first) {
                continue;
            }
            if frame.len() < 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "truncated ChannelData",
                ));
            }
            let channel = u16::from_be_bytes([frame[0], frame[1]]);
            let length = usize::from(u16::from_be_bytes([frame[2], frame[3]]));
            let payload = frame
                .get(4..4 + length)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ChannelData overrun"))?;
            return Ok((channel, payload.to_vec()));
        }
    }

    /// Receives one relayed application datagram forwarded by the TURN server,
    /// accepting either a `ChannelData` frame or a STUN `Data` indication, and
    /// returns `(peer, bytes)`. `peer` is present only for `Data` indications.
    ///
    /// # Errors
    ///
    /// Returns a timeout or other I/O error.
    pub fn recv_relayed(&self, buffer: &mut [u8]) -> io::Result<(Option<SocketAddr>, Vec<u8>)> {
        loop {
            let (len, from) = self.socket.recv_from(buffer)?;
            if from != self.server {
                continue;
            }
            let frame = &buffer[..len];
            let Some(&first) = frame.first() else {
                continue;
            };
            if (0x40..=0x7f).contains(&first) {
                if frame.len() < 4 {
                    continue;
                }
                let length = usize::from(u16::from_be_bytes([frame[2], frame[3]]));
                if let Some(data) = frame.get(4..4 + length) {
                    return Ok((None, data.to_vec()));
                }
                continue;
            }
            let Ok(message) = Message::parse(frame) else {
                continue;
            };
            if message.kind == DATA_INDICATION {
                if let Some(data) = message.attribute(ATTR_DATA) {
                    let peer = message.xor_address(ATTR_XOR_PEER_ADDRESS);
                    return Ok((peer, data.to_vec()));
                }
            }
        }
    }

    /// The bound UDP socket, for receiving raw relayed datagrams that the TURN
    /// server forwards from a peer (delivered as plain UDP from the relay).
    #[must_use]
    pub const fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    /// The TURN server this client is talking to.
    #[must_use]
    pub const fn server(&self) -> SocketAddr {
        self.server
    }

    fn build_allocate(&self, authenticated: bool) -> Vec<u8> {
        let mut builder = MessageBuilder::new(ALLOCATE_REQUEST);
        let mut transport = [0u8; 4];
        transport[0] = REQUESTED_TRANSPORT_UDP;
        builder.add_attribute(ATTR_REQUESTED_TRANSPORT, &transport);
        if authenticated {
            self.finish_authenticated(&mut builder);
        }
        builder.finish()
    }

    /// Appends USERNAME/REALM/NONCE and the trailing MESSAGE-INTEGRITY.
    fn finish_authenticated(&self, builder: &mut MessageBuilder) {
        builder.add_attribute(ATTR_USERNAME, self.credentials.username.as_bytes());
        builder.add_attribute(ATTR_REALM, self.realm.as_bytes());
        builder.add_attribute(ATTR_NONCE, &self.nonce);
        builder.add_message_integrity(&self.integrity_key());
    }

    fn absorb_challenge(&mut self, message: &Message) -> io::Result<()> {
        let realm = message
            .attribute(ATTR_REALM)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "challenge without realm"))?;
        let nonce = message
            .attribute(ATTR_NONCE)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "challenge without nonce"))?;
        self.realm = String::from_utf8_lossy(realm).into_owned();
        self.nonce = nonce.to_vec();
        Ok(())
    }

    fn parse_allocation(message: &Message) -> io::Result<Allocation> {
        let relayed = message
            .xor_address(ATTR_XOR_RELAYED_ADDRESS)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no relayed address"))?;
        let mapped = message
            .xor_address(ATTR_XOR_MAPPED_ADDRESS)
            .unwrap_or(relayed);
        let lifetime_secs = message
            .attribute(ATTR_LIFETIME)
            .and_then(|value| value.get(..4))
            .map_or(600, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
        Ok(Allocation {
            relayed,
            mapped,
            lifetime_secs,
        })
    }

    fn transact(&self, request: &[u8]) -> io::Result<Vec<u8>> {
        self.socket.set_read_timeout(Some(self.timeout))?;
        self.socket.send_to(request, self.server)?;
        let mut buffer = vec![0u8; 1500];
        loop {
            let (len, from) = self.socket.recv_from(&mut buffer)?;
            if from != self.server {
                continue;
            }
            // Ignore relayed ChannelData that races with a control response.
            if buffer
                .first()
                .is_some_and(|first| (0x40..=0x7f).contains(first))
            {
                continue;
            }
            buffer.truncate(len);
            return Ok(buffer);
        }
    }
}

type HmacSha1 = Hmac<Sha1>;

/// Incremental STUN/TURN message builder.
struct MessageBuilder {
    message_type: u16,
    transaction_id: [u8; 12],
    attributes: Vec<u8>,
}

impl MessageBuilder {
    fn new(message_type: u16) -> Self {
        let mut transaction_id = [0u8; 12];
        OsRng.fill_bytes(&mut transaction_id);
        Self {
            message_type,
            transaction_id,
            attributes: Vec::new(),
        }
    }

    fn add_attribute(&mut self, attribute_type: u16, value: &[u8]) {
        self.attributes
            .extend_from_slice(&attribute_type.to_be_bytes());
        self.attributes
            .extend_from_slice(&u16::try_from(value.len()).unwrap_or(u16::MAX).to_be_bytes());
        self.attributes.extend_from_slice(value);
        // Attributes are padded to a 4-byte boundary.
        while self.attributes.len() % 4 != 0 {
            self.attributes.push(0);
        }
    }

    fn add_xor_address(&mut self, attribute_type: u16, address: SocketAddr, txid: &[u8; 12]) {
        self.add_attribute(attribute_type, &encode_xor_address(address, txid));
    }

    /// Appends MESSAGE-INTEGRITY (HMAC-SHA1) over the message so far, with the
    /// header length set to include the integrity attribute.
    fn add_message_integrity(&mut self, key: &[u8]) {
        let length_with_mi = self.attributes.len() + 4 + 20;
        let mut signed = self.header(u16::try_from(length_with_mi).unwrap_or(u16::MAX));
        signed.extend_from_slice(&self.attributes);
        let mut mac = HmacSha1::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&signed);
        let digest = mac.finalize().into_bytes();
        self.add_attribute(ATTR_MESSAGE_INTEGRITY, &digest);
    }

    fn header(&self, length: u16) -> Vec<u8> {
        let mut header = Vec::with_capacity(20);
        header.extend_from_slice(&self.message_type.to_be_bytes());
        header.extend_from_slice(&length.to_be_bytes());
        header.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        header.extend_from_slice(&self.transaction_id);
        header
    }

    fn finish(&self) -> Vec<u8> {
        let mut message = self.header(u16::try_from(self.attributes.len()).unwrap_or(u16::MAX));
        message.extend_from_slice(&self.attributes);
        message
    }
}

/// A parsed STUN/TURN message.
struct Message {
    kind: u16,
    transaction_id: [u8; 12],
    attributes: Vec<(u16, Vec<u8>)>,
}

impl Message {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 20
            || u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) != MAGIC_COOKIE
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a STUN message",
            ));
        }
        let kind = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
        let mut transaction_id = [0u8; 12];
        transaction_id.copy_from_slice(&bytes[8..20]);
        let body = bytes
            .get(20..20 + length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated STUN body"))?;
        let mut attributes = Vec::new();
        let mut cursor = 0;
        while cursor + 4 <= body.len() {
            let attribute_type = u16::from_be_bytes([body[cursor], body[cursor + 1]]);
            let value_len = usize::from(u16::from_be_bytes([body[cursor + 2], body[cursor + 3]]));
            let start = cursor + 4;
            let end = start
                .checked_add(value_len)
                .filter(|end| *end <= body.len())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad STUN attribute"))?;
            attributes.push((attribute_type, body[start..end].to_vec()));
            cursor = end + ((4 - (value_len % 4)) % 4);
        }
        Ok(Self {
            kind,
            transaction_id,
            attributes,
        })
    }

    fn attribute(&self, attribute_type: u16) -> Option<&[u8]> {
        self.attributes
            .iter()
            .find(|(kind, _)| *kind == attribute_type)
            .map(|(_, value)| value.as_slice())
    }

    fn xor_address(&self, attribute_type: u16) -> Option<SocketAddr> {
        decode_xor_address(self.attribute(attribute_type)?, &self.transaction_id)
    }

    fn error_code(&self) -> Option<u16> {
        let value = self.attribute(ATTR_ERROR_CODE)?;
        let class = u16::from(*value.get(2)? & 0x7);
        let number = u16::from(*value.get(3)?);
        Some(class * 100 + number)
    }
}

fn server_error(context: &str, message: &Message) -> io::Error {
    let code = message.error_code().unwrap_or(0);
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{context} (error {code})"),
    )
}

fn encode_xor_address(address: SocketAddr, txid: &[u8; 12]) -> Vec<u8> {
    let cookie = MAGIC_COOKIE.to_be_bytes();
    let xport = address.port() ^ (MAGIC_COOKIE >> 16) as u16;
    match address {
        SocketAddr::V4(v4) => {
            let mut out = vec![0, 0x01];
            out.extend_from_slice(&xport.to_be_bytes());
            for (byte, mask) in v4.ip().octets().iter().zip(cookie) {
                out.push(byte ^ mask);
            }
            out
        }
        SocketAddr::V6(v6) => {
            let mut mask = [0u8; 16];
            mask[..4].copy_from_slice(&cookie);
            mask[4..].copy_from_slice(txid);
            let mut out = vec![0, 0x02];
            out.extend_from_slice(&xport.to_be_bytes());
            for (byte, m) in v6.ip().octets().iter().zip(mask) {
                out.push(byte ^ m);
            }
            out
        }
    }
}

fn decode_xor_address(value: &[u8], txid: &[u8; 12]) -> Option<SocketAddr> {
    if value.len() < 4 || value[0] != 0 {
        return None;
    }
    let port = u16::from_be_bytes([value[2], value[3]]) ^ (MAGIC_COOKIE >> 16) as u16;
    match value[1] {
        0x01 if value.len() == 8 => {
            let cookie = MAGIC_COOKIE.to_be_bytes();
            let ip = std::net::Ipv4Addr::new(
                value[4] ^ cookie[0],
                value[5] ^ cookie[1],
                value[6] ^ cookie[2],
                value[7] ^ cookie[3],
            );
            Some(SocketAddr::from((ip, port)))
        }
        0x02 if value.len() == 20 => {
            let mut mask = [0u8; 16];
            mask[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(txid);
            let mut bytes = [0u8; 16];
            for (out, (input, m)) in bytes.iter_mut().zip(value[4..].iter().zip(mask)) {
                *out = input ^ m;
            }
            Some(SocketAddr::from((std::net::Ipv6Addr::from(bytes), port)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_ipv4_address_round_trips() {
        let txid = [3u8; 12];
        let address: SocketAddr = "203.0.113.5:51234".parse().unwrap();
        let encoded = encode_xor_address(address, &txid);
        assert_eq!(decode_xor_address(&encoded, &txid), Some(address));
    }

    #[test]
    fn xor_ipv6_address_round_trips() {
        let txid = [9u8; 12];
        let address: SocketAddr = "[2001:db8::1]:9000".parse().unwrap();
        let encoded = encode_xor_address(address, &txid);
        assert_eq!(decode_xor_address(&encoded, &txid), Some(address));
    }

    #[test]
    fn message_integrity_matches_reference_hmac() {
        // Build an Allocate with a known key and verify the MI attribute equals
        // an independent HMAC-SHA1 over the header (with adjusted length) and
        // attributes preceding it.
        let mut builder = MessageBuilder::new(ALLOCATE_REQUEST);
        let mut transport = [0u8; 4];
        transport[0] = REQUESTED_TRANSPORT_UDP;
        builder.add_attribute(ATTR_REQUESTED_TRANSPORT, &transport);
        builder.add_attribute(ATTR_USERNAME, b"user");
        builder.add_attribute(ATTR_REALM, b"fluxcast");
        builder.add_attribute(ATTR_NONCE, b"nonce-value");
        let key = b"secret-key";
        builder.add_message_integrity(key);
        let message = builder.finish();

        let parsed = Message::parse(&message).unwrap();
        let mi = parsed.attribute(ATTR_MESSAGE_INTEGRITY).unwrap();
        assert_eq!(mi.len(), 20);

        // Recompute over everything up to the MI attribute.
        let mi_offset = message.len() - 24;
        let mut signed = message[..mi_offset].to_vec();
        let claimed_len = (mi_offset - 20) + 24;
        signed[2..4].copy_from_slice(&u16::try_from(claimed_len).unwrap().to_be_bytes());
        let mut mac = HmacSha1::new_from_slice(key).unwrap();
        mac.update(&signed);
        assert_eq!(mac.finalize().into_bytes().as_slice(), mi);
    }

    #[test]
    fn parses_error_code_from_a_401_challenge() {
        let mut builder = MessageBuilder::new(ALLOCATE_ERROR);
        builder.add_attribute(ATTR_ERROR_CODE, &[0, 0, 4, 1]); // class 4, number 01
        builder.add_attribute(ATTR_REALM, b"fluxcast");
        builder.add_attribute(ATTR_NONCE, b"abc123");
        let bytes = builder.finish();
        let message = Message::parse(&bytes).unwrap();
        assert_eq!(message.error_code(), Some(401));
        assert_eq!(message.attribute(ATTR_REALM), Some(b"fluxcast".as_slice()));
    }

    #[test]
    fn rejects_non_stun_bytes() {
        assert!(Message::parse(&[0u8; 8]).is_err());
        assert!(Message::parse(&[0xff; 40]).is_err());
    }

    #[test]
    fn channel_number_bounds_are_enforced() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut client = TurnClient::new(
            socket,
            "127.0.0.1:3478".parse().unwrap(),
            TurnCredentials {
                username: "u".into(),
                password: "p".into(),
            },
            Duration::from_millis(50),
        )
        .unwrap();
        assert!(
            client
                .channel_bind("127.0.0.1:5000".parse().unwrap(), 0x3fff)
                .is_err()
        );
    }
}
