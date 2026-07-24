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

use md5::{Digest, Md5};

use crate::stun::{Message, MessageBuilder};

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

// TURN-specific attribute types (shared ones live in the `stun` module).
const ATTR_USERNAME: u16 = 0x0006;
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
        let message = parse_message(&response)?;
        if message.kind == ALLOCATE_ERROR {
            self.absorb_challenge(&message)?;
        } else if message.kind == ALLOCATE_SUCCESS {
            return Self::parse_allocation(&message);
        }

        // Authenticated retry with USERNAME/REALM/NONCE/MESSAGE-INTEGRITY.
        let request = self.build_allocate(true);
        let response = self.transact(&request)?;
        let message = parse_message(&response)?;
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
        let message = parse_message(&response)?;
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
        let message = parse_message(&response)?;
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
        let message = parse_message(&response)?;
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
            let Some(message) = Message::parse(frame) else {
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

fn parse_message(bytes: &[u8]) -> io::Result<Message> {
    Message::parse(bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed STUN/TURN message"))
}

fn server_error(context: &str, message: &Message) -> io::Error {
    let code = message.error_code().unwrap_or(0);
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{context} (error {code})"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTR_ERROR_CODE: u16 = 0x0009;

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
