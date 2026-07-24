//! Cryptographic session primitives for `FluxCast`.
//!
//! This module composes audited primitives; it does not define a cipher. A
//! caller must authenticate the peer's long-term public key out of band before
//! accepting a session.
#![forbid(unsafe_code)]

use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, Tag};
use fluxcast_proto::Header;
use rand_core::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

/// Flag set on FCDP packets whose payload is protected by `Session`.
pub const ENCRYPTED_FLAG: u8 = 0b0000_0001;

/// A long-term X25519 identity. Persist its private bytes in a platform key
/// store; never serialize it into application logs or configuration files.
pub struct Identity {
    secret: StaticSecret,
    public: PublicKey,
}

impl Identity {
    #[must_use]
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }
    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }
    /// Creates a session only for an out-of-band authenticated peer identity.
    #[must_use]
    pub fn establish(&self, peer_public: [u8; 32], session_id: u64, epoch: u16) -> Session {
        let shared = self.secret.diffie_hellman(&PublicKey::from(peer_public));
        Session {
            cipher: ChaCha20Poly1305::new(Key::from_slice(shared.as_bytes())),
            id: session_id,
            epoch,
            replay: ReplayWindow::default(),
        }
    }
}

/// Encrypts data with a unique nonce derived from `(session_id, sequence)`.
/// A new session key is required whenever `epoch` changes.
pub struct Session {
    cipher: ChaCha20Poly1305,
    id: u64,
    epoch: u16,
    replay: ReplayWindow,
}

impl Session {
    #[must_use]
    pub const fn epoch(&self) -> u16 {
        self.epoch
    }
    /// Encrypts one payload and appends the 16-byte Poly1305 tag.
    ///
    /// # Errors
    ///
    /// Returns an error only when the AEAD operation fails.
    pub fn seal(
        &self,
        sequence: u32,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let mut ciphertext = plaintext.to_vec();
        let tag = self
            .cipher
            .encrypt_in_place_detached(&nonce(self.id, sequence), associated_data, &mut ciphertext)
            .map_err(|_| SecurityError::Encryption)?;
        ciphertext.extend_from_slice(&tag);
        Ok(ciphertext)
    }
    /// Authenticates, rejects replayed packets, and decrypts one payload.
    ///
    /// # Errors
    ///
    /// Returns an error for replayed, truncated, or unauthenticated packets.
    pub fn open(
        &mut self,
        sequence: u32,
        associated_data: &[u8],
        packet: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        if self.replay.contains(sequence) {
            return Err(SecurityError::Replay);
        }
        if packet.len() < Tag::default().len() {
            return Err(SecurityError::Truncated);
        }
        let split = packet.len() - Tag::default().len();
        let (ciphertext, tag) = packet.split_at(split);
        let mut plaintext = ciphertext.to_vec();
        self.cipher
            .decrypt_in_place_detached(
                &nonce(self.id, sequence),
                associated_data,
                &mut plaintext,
                Tag::from_slice(tag),
            )
            .map_err(|_| SecurityError::Authentication)?;
        self.replay.accept(sequence);
        Ok(plaintext)
    }

    /// Protects an FCDP payload and authenticates every header field except its
    /// corruption-detection CRC. Only MEDIA/FEC/control payloads may use this;
    /// a session handshake must remain separately specified.
    ///
    /// # Errors
    ///
    /// Returns an error if the resulting packet exceeds the FCDP datagram
    /// budget or encryption fails.
    pub fn seal_fcdp(
        &self,
        mut header: Header,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        header.flags |= ENCRYPTED_FLAG;
        let protected_len = plaintext.len().saturating_add(Tag::default().len());
        let associated_data = header
            .associated_data(protected_len)
            .map_err(|_| SecurityError::Packet)?;
        let ciphertext = self.seal(header.sequence_number, &associated_data, plaintext)?;
        let mut packet = Vec::new();
        header
            .encode(&ciphertext, &mut packet)
            .map_err(|_| SecurityError::Packet)?;
        Ok(packet)
    }

    /// Parses, authenticates, replays-checks, and decrypts an encrypted FCDP packet.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid FCDP framing, unencrypted packets, replay,
    /// or authentication failure.
    pub fn open_fcdp(&mut self, packet: &[u8]) -> Result<(Header, Vec<u8>), SecurityError> {
        let (header, ciphertext) = Header::decode(packet).map_err(|_| SecurityError::Packet)?;
        if header.flags & ENCRYPTED_FLAG == 0 {
            return Err(SecurityError::Unencrypted);
        }
        let associated_data = header
            .associated_data(ciphertext.len())
            .map_err(|_| SecurityError::Packet)?;
        let plaintext = self.open(header.sequence_number, &associated_data, ciphertext)?;
        Ok((header, plaintext))
    }
}

fn nonce(session_id: u64, sequence: u32) -> Nonce {
    let mut value = [0_u8; 12];
    value[..8].copy_from_slice(&session_id.to_be_bytes());
    value[8..].copy_from_slice(&sequence.to_be_bytes());
    *Nonce::from_slice(&value)
}

/// A 64-packet sliding replay window. Packets outside the window or seen twice
/// are rejected after successful authentication.
#[derive(Debug, Default)]
pub struct ReplayWindow {
    newest: Option<u32>,
    bitmap: u64,
}

impl ReplayWindow {
    #[must_use]
    pub fn contains(&self, sequence: u32) -> bool {
        let Some(newest) = self.newest else {
            return false;
        };
        let distance = newest.wrapping_sub(sequence);
        if distance > (u32::MAX / 2) {
            return false;
        }
        distance >= 64 || (self.bitmap & (1_u64 << distance)) != 0
    }
    pub fn accept(&mut self, sequence: u32) {
        match self.newest {
            None => {
                self.newest = Some(sequence);
                self.bitmap = 1;
            }
            Some(newest)
                if sequence.wrapping_sub(newest) < (u32::MAX / 2) && sequence != newest =>
            {
                let shift = sequence.wrapping_sub(newest);
                self.bitmap = if shift >= 64 {
                    1
                } else {
                    (self.bitmap << shift) | 1
                };
                self.newest = Some(sequence);
            }
            Some(newest) => {
                let distance = newest.wrapping_sub(sequence);
                if distance < 64 {
                    self.bitmap |= 1_u64 << distance;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityError {
    Encryption,
    Authentication,
    Replay,
    Truncated,
    Packet,
    Unencrypted,
}
impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FluxCast security error: {self:?}")
    }
}
impl std::error::Error for SecurityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxcast_proto::PacketType;
    #[test]
    fn authenticated_peers_share_a_session_key() {
        let a = Identity::generate();
        let b = Identity::generate();
        let mut receiver = b.establish(a.public_key(), 4, 1);
        let sender = a.establish(b.public_key(), 4, 1);
        let encrypted = sender.seal(7, b"header", b"media").unwrap();
        assert_eq!(receiver.open(7, b"header", &encrypted).unwrap(), b"media");
    }
    #[test]
    fn modified_packets_and_replays_are_rejected() {
        let a = Identity::generate();
        let b = Identity::generate();
        let mut receiver = b.establish(a.public_key(), 4, 1);
        let sender = a.establish(b.public_key(), 4, 1);
        let encrypted = sender.seal(1, b"h", b"m").unwrap();
        assert_eq!(
            receiver.open(1, b"x", &encrypted),
            Err(SecurityError::Authentication)
        );
        assert_eq!(receiver.open(1, b"h", &encrypted).unwrap(), b"m");
        assert_eq!(
            receiver.open(1, b"h", &encrypted),
            Err(SecurityError::Replay)
        );
    }
    #[test]
    fn encrypted_fcdp_authenticates_its_header() {
        let a = Identity::generate();
        let b = Identity::generate();
        let sender = a.establish(b.public_key(), 4, 1);
        let mut receiver = b.establish(a.public_key(), 4, 1);
        let mut header = Header::new(PacketType::Media);
        header.session_id = 4;
        header.epoch = 1;
        header.sequence_number = 9;
        let packet = sender.seal_fcdp(header, b"video").unwrap();
        let mut altered = packet.clone();
        let last = altered.len() - 1;
        altered[last] ^= 1;
        assert_eq!(
            receiver.open_fcdp(&altered),
            Err(SecurityError::Authentication)
        );
        let (actual, payload) = receiver.open_fcdp(&packet).unwrap();
        assert_eq!(actual.sequence_number, 9);
        assert_eq!(payload, b"video");
    }
}
