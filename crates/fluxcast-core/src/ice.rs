//! Authenticated ICE connectivity checks (RFC 8445 short-term credentials).
//!
//! [`ordered_ice_pairs`](crate::ordered_ice_pairs) ranks candidate pairs; this
//! module performs the STUN Binding check that must succeed before a pair is
//! nominated. Each check and its response carry a `MESSAGE-INTEGRITY` keyed by
//! the peer's ICE password, so an off-path attacker cannot forge reachability
//! or steer a session onto a bogus path. The controlling agent sets
//! `USE-CANDIDATE` to nominate.
//!
//! The agent is transport-minimal: it drives one [`UdpSocket`] and exposes the
//! individual build/validate steps so an event loop can schedule checks itself.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use rand_core::{OsRng, RngCore};

use crate::stun::{
    ATTR_USERNAME, ATTR_XOR_MAPPED_ADDRESS, Message, MessageBuilder, encode_xor_address,
};

const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS: u16 = 0x0101;

const ATTR_PRIORITY: u16 = 0x0024;
const ATTR_USE_CANDIDATE: u16 = 0x0025;
const ATTR_ICE_CONTROLLED: u16 = 0x8029;
const ATTR_ICE_CONTROLLING: u16 = 0x802a;

/// One side's ICE short-term credentials, exchanged over the signalling channel.
#[derive(Debug, Clone)]
pub struct IceCredentials {
    pub ufrag: String,
    pub pwd: String,
}

impl IceCredentials {
    /// Generates a random ufrag/pwd pair (RFC 8445 sizes: 4-byte ufrag,
    /// 24-byte pwd, base64url-ish alphabet).
    #[must_use]
    pub fn random() -> Self {
        Self {
            ufrag: random_token(4),
            pwd: random_token(24),
        }
    }
}

fn random_token(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char)
        .collect()
}

/// The outcome of validating an inbound connectivity check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InboundCheck {
    /// True when the controlling peer asked to nominate this pair.
    pub nominated: bool,
}

/// An authenticated ICE connectivity-check agent over one UDP socket.
#[derive(Debug)]
pub struct IceAgent {
    socket: UdpSocket,
    local: IceCredentials,
    remote: IceCredentials,
    controlling: bool,
    tie_breaker: u64,
    timeout: Duration,
}

impl IceAgent {
    /// Binds an agent. `controlling` selects the role that may nominate.
    ///
    /// # Errors
    ///
    /// Returns the socket configuration error.
    pub fn new(
        socket: UdpSocket,
        local: IceCredentials,
        remote: IceCredentials,
        controlling: bool,
        timeout: Duration,
    ) -> io::Result<Self> {
        socket.set_read_timeout(Some(timeout))?;
        Ok(Self {
            socket,
            local,
            remote,
            controlling,
            tie_breaker: OsRng.next_u64(),
            timeout,
        })
    }

    /// The bound local address.
    ///
    /// # Errors
    ///
    /// Returns the socket query error.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Sends one Binding check to `remote` and waits for a valid success
    /// response, answering any interleaved inbound checks so a simultaneous
    /// peer check also completes. Returns this agent's mapped address as the
    /// peer observed it.
    ///
    /// # Errors
    ///
    /// Returns a timeout or I/O error, or `InvalidData` if the peer rejects or
    /// never validly answers the check.
    pub fn connectivity_check(&self, remote: SocketAddr, nominate: bool) -> io::Result<SocketAddr> {
        let (txid, request) = self.build_check(nominate);
        self.socket.set_read_timeout(Some(self.timeout))?;
        self.socket.send_to(&request, remote)?;
        let mut buffer = vec![0u8; 1500];
        loop {
            let (len, from) = self.socket.recv_from(&mut buffer)?;
            if from != remote {
                continue;
            }
            let Some(message) = Message::parse(&buffer[..len]) else {
                continue;
            };
            match message.kind {
                BINDING_SUCCESS => {
                    if message.transaction_id == txid
                        && message.verify_integrity(&buffer[..len], self.remote.pwd.as_bytes())
                    {
                        if let Some(mapped) = message.xor_address(ATTR_XOR_MAPPED_ADDRESS) {
                            return Ok(mapped);
                        }
                    }
                }
                BINDING_REQUEST => {
                    // Answer the peer's check so its side can also succeed.
                    if let Some(response) = self.build_response_for(&buffer[..len], from) {
                        self.socket.send_to(&response, from)?;
                    }
                }
                _ => {}
            }
        }
    }

    /// Waits for one inbound Binding check, validates it, and replies with a
    /// success response. Returns whether the peer nominated the pair.
    ///
    /// # Errors
    ///
    /// Returns a timeout or I/O error.
    pub fn serve_once(&self) -> io::Result<InboundCheck> {
        let mut buffer = vec![0u8; 1500];
        loop {
            let (len, from) = self.socket.recv_from(&mut buffer)?;
            let Some(message) = Message::parse(&buffer[..len]) else {
                continue;
            };
            if message.kind != BINDING_REQUEST {
                continue;
            }
            if !self.request_is_valid(&message, &buffer[..len]) {
                continue;
            }
            let nominated = message.attribute(ATTR_USE_CANDIDATE).is_some();
            let response = self.build_response(message.transaction_id, from);
            self.socket.send_to(&response, from)?;
            return Ok(InboundCheck { nominated });
        }
    }

    /// Builds a Binding request for a pair, returning its transaction id.
    #[must_use]
    fn build_check(&self, nominate: bool) -> ([u8; 12], Vec<u8>) {
        let mut builder = MessageBuilder::new(BINDING_REQUEST);
        let txid = builder.transaction_id;
        // USERNAME = "peer-ufrag:local-ufrag" (RFC 8445 §7.2.2).
        let username = format!("{}:{}", self.remote.ufrag, self.local.ufrag);
        builder.add_attribute(ATTR_USERNAME, username.as_bytes());
        builder.add_attribute(ATTR_PRIORITY, &0x7e00_00ffu32.to_be_bytes());
        let role = if self.controlling {
            ATTR_ICE_CONTROLLING
        } else {
            ATTR_ICE_CONTROLLED
        };
        builder.add_attribute(role, &self.tie_breaker.to_be_bytes());
        if nominate && self.controlling {
            builder.add_attribute(ATTR_USE_CANDIDATE, &[]);
        }
        builder.add_message_integrity(self.remote.pwd.as_bytes());
        (txid, builder.finish())
    }

    fn request_is_valid(&self, message: &Message, datagram: &[u8]) -> bool {
        let expected = format!("{}:{}", self.local.ufrag, self.remote.ufrag);
        message.attribute(ATTR_USERNAME) == Some(expected.as_bytes())
            && message.verify_integrity(datagram, self.local.pwd.as_bytes())
    }

    fn build_response(&self, request_txid: [u8; 12], mapped: SocketAddr) -> Vec<u8> {
        let mut builder = MessageBuilder::with_transaction_id(BINDING_SUCCESS, request_txid);
        builder.add_attribute(
            ATTR_XOR_MAPPED_ADDRESS,
            &encode_xor_address(mapped, &request_txid),
        );
        builder.add_message_integrity(self.local.pwd.as_bytes());
        builder.finish()
    }

    fn build_response_for(&self, datagram: &[u8], from: SocketAddr) -> Option<Vec<u8>> {
        let message = Message::parse(datagram)?;
        if message.kind != BINDING_REQUEST || !self.request_is_valid(&message, datagram) {
            return None;
        }
        Some(self.build_response(message.transaction_id, from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn agents() -> (IceAgent, IceAgent, SocketAddr, SocketAddr) {
        let a_cred = IceCredentials::random();
        let b_cred = IceCredentials::random();
        let a_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let a_addr = a_sock.local_addr().unwrap();
        let b_addr = b_sock.local_addr().unwrap();
        let a = IceAgent::new(
            a_sock,
            a_cred.clone(),
            b_cred.clone(),
            true,
            Duration::from_secs(2),
        )
        .unwrap();
        let b = IceAgent::new(b_sock, b_cred, a_cred, false, Duration::from_secs(2)).unwrap();
        (a, b, a_addr, b_addr)
    }

    #[test]
    fn authenticated_check_succeeds_and_nominates() {
        let (a, b, a_addr, b_addr) = agents();
        // B answers one inbound check on a background thread.
        let responder = thread::spawn(move || b.serve_once());
        let mapped = a.connectivity_check(b_addr, true).unwrap();
        // A's check reached B, which observed A's address.
        assert_eq!(mapped, a_addr);
        let inbound = responder.join().unwrap().unwrap();
        assert!(inbound.nominated);
    }

    #[test]
    fn a_check_with_the_wrong_password_is_rejected() {
        let a_cred = IceCredentials::random();
        let b_cred = IceCredentials::random();
        let wrong = IceCredentials {
            ufrag: b_cred.ufrag.clone(),
            pwd: "wrong-password".into(),
        };
        let a_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b_addr = b_sock.local_addr().unwrap();
        // A believes the peer password is `wrong`, so its MESSAGE-INTEGRITY
        // will not validate at B.
        let a = IceAgent::new(
            a_sock,
            a_cred.clone(),
            wrong,
            true,
            Duration::from_millis(300),
        )
        .unwrap();
        let b = IceAgent::new(b_sock, b_cred, a_cred, false, Duration::from_millis(300)).unwrap();
        let responder = thread::spawn(move || b.serve_once());
        // B never accepts the forged check, so A times out waiting for a reply.
        assert!(a.connectivity_check(b_addr, true).is_err());
        assert!(responder.join().unwrap().is_err());
    }
}
