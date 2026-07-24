//! Authenticated, forward-secret session primitives for `FluxCast`.
//!
//! The handshake uses Ed25519 identities to authenticate ephemeral X25519
//! keys.  Session traffic keys are derived with HKDF-SHA256 and are distinct
//! in each direction, so a packet nonce may safely be used by both peers.
#![forbid(unsafe_code)]

use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, Tag};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use fluxcast_proto::Header;
use hkdf::Hkdf;
use rand_core::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

/// Flag set on FCDP packets whose payload is protected by `Session`.
pub const ENCRYPTED_FLAG: u8 = 0b0000_0001;
const HANDSHAKE_LABEL: &[u8] = b"fluxcast/fcdp-handshake/v1";
const CLIENT_HELLO_LEN: usize = 8 + 32 + 32 + 64;
const SERVER_WELCOME_LEN: usize = 8 + 32 + 32 + 32 + 64;

/// Long-term Ed25519 signing identity. Persist its private bytes in a
/// platform key store; distribute [`Identity::public_key`] by an authenticated
/// channel or trust service.
pub struct Identity {
    signing_key: SigningKey,
}

impl Identity {
    #[must_use]
    pub fn generate() -> Self {
        Self {
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Starts an authenticated session handshake for `session_id`.
    #[must_use]
    pub fn begin_handshake(&self, session_id: u64) -> HandshakeInitiator {
        HandshakeInitiator::new(self, session_id)
    }

    /// Verifies a client hello and produces a signed server response.
    ///
    /// The returned client identity must be authorized by the application
    /// before media is accepted. Passing `expected_client` pins a known client
    /// key; `None` is appropriate only when the caller has another explicit
    /// authorization policy for the returned identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not expected, its record is invalid,
    /// or its signature cannot be verified.
    pub fn accept_handshake(
        &self,
        hello: &ClientHello,
        expected_client: Option<[u8; 32]>,
    ) -> Result<(ServerWelcome, Session, [u8; 32]), SecurityError> {
        if let Some(expected) = expected_client {
            if hello.identity != expected {
                return Err(SecurityError::UnexpectedPeer);
            }
        }
        hello.verify()?;
        let ephemeral_secret = StaticSecret::random_from_rng(OsRng);
        let ephemeral_public = PublicKey::from(&ephemeral_secret).to_bytes();
        let mut signed = server_transcript(hello, &ephemeral_public);
        let signature = self.signing_key.sign(&signed).to_bytes();
        signed.fill(0);
        let welcome = ServerWelcome {
            session_id: hello.session_id,
            client_ephemeral: hello.ephemeral,
            ephemeral: ephemeral_public,
            identity: self.public_key(),
            signature,
        };
        let shared = ephemeral_secret.diffie_hellman(&PublicKey::from(hello.ephemeral));
        let session = derive_session(
            shared.as_bytes(),
            hello.session_id,
            &handshake_transcript(hello, &welcome),
            Role::Server,
        )?;
        Ok((welcome, session, hello.identity))
    }
}

/// First signed message of the version-1 handshake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientHello {
    pub session_id: u64,
    pub ephemeral: [u8; 32],
    pub identity: [u8; 32],
    pub signature: [u8; 64],
}

impl ClientHello {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(CLIENT_HELLO_LEN);
        output.extend_from_slice(&self.session_id.to_be_bytes());
        output.extend_from_slice(&self.ephemeral);
        output.extend_from_slice(&self.identity);
        output.extend_from_slice(&self.signature);
        output
    }

    /// # Errors
    ///
    /// Returns an error unless `bytes` is exactly one encoded client hello.
    pub fn decode(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() != CLIENT_HELLO_LEN {
            return Err(SecurityError::HandshakeEncoding);
        }
        let mut ephemeral = [0; 32];
        ephemeral.copy_from_slice(&bytes[8..40]);
        let mut identity = [0; 32];
        identity.copy_from_slice(&bytes[40..72]);
        let mut signature = [0; 64];
        signature.copy_from_slice(&bytes[72..]);
        Ok(Self {
            session_id: u64::from_be_bytes(
                bytes[..8]
                    .try_into()
                    .map_err(|_| SecurityError::HandshakeEncoding)?,
            ),
            ephemeral,
            identity,
            signature,
        })
    }

    fn verify(&self) -> Result<(), SecurityError> {
        let key = VerifyingKey::from_bytes(&self.identity)
            .map_err(|_| SecurityError::HandshakeAuthentication)?;
        key.verify(
            &client_transcript(self.session_id, &self.ephemeral, &self.identity),
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| SecurityError::HandshakeAuthentication)
    }
}

/// Signed response from a server, bound to the exact client hello.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerWelcome {
    pub session_id: u64,
    pub client_ephemeral: [u8; 32],
    pub ephemeral: [u8; 32],
    pub identity: [u8; 32],
    pub signature: [u8; 64],
}

impl ServerWelcome {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(SERVER_WELCOME_LEN);
        output.extend_from_slice(&self.session_id.to_be_bytes());
        output.extend_from_slice(&self.client_ephemeral);
        output.extend_from_slice(&self.ephemeral);
        output.extend_from_slice(&self.identity);
        output.extend_from_slice(&self.signature);
        output
    }

    /// # Errors
    ///
    /// Returns an error unless `bytes` is exactly one encoded server welcome.
    pub fn decode(bytes: &[u8]) -> Result<Self, SecurityError> {
        if bytes.len() != SERVER_WELCOME_LEN {
            return Err(SecurityError::HandshakeEncoding);
        }
        let mut client_ephemeral = [0; 32];
        client_ephemeral.copy_from_slice(&bytes[8..40]);
        let mut ephemeral = [0; 32];
        ephemeral.copy_from_slice(&bytes[40..72]);
        let mut identity = [0; 32];
        identity.copy_from_slice(&bytes[72..104]);
        let mut signature = [0; 64];
        signature.copy_from_slice(&bytes[104..]);
        Ok(Self {
            session_id: u64::from_be_bytes(
                bytes[..8]
                    .try_into()
                    .map_err(|_| SecurityError::HandshakeEncoding)?,
            ),
            client_ephemeral,
            ephemeral,
            identity,
            signature,
        })
    }
}

/// State retained by a client until it receives the matching server welcome.
pub struct HandshakeInitiator {
    secret: StaticSecret,
    hello: ClientHello,
}

impl HandshakeInitiator {
    fn new(identity: &Identity, session_id: u64) -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let ephemeral = PublicKey::from(&secret).to_bytes();
        let public = identity.public_key();
        let signature = identity
            .signing_key
            .sign(&client_transcript(session_id, &ephemeral, &public))
            .to_bytes();
        Self {
            secret,
            hello: ClientHello {
                session_id,
                ephemeral,
                identity: public,
                signature,
            },
        }
    }

    #[must_use]
    pub fn hello(&self) -> &ClientHello {
        &self.hello
    }

    /// Validates the server identity/signature and derives client traffic keys.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer does not match the pinned identity, the
    /// response is not bound to this hello, or signature/key derivation fails.
    pub fn complete(
        self,
        welcome: &ServerWelcome,
        expected_server: [u8; 32],
    ) -> Result<Session, SecurityError> {
        if welcome.identity != expected_server {
            return Err(SecurityError::UnexpectedPeer);
        }
        if welcome.session_id != self.hello.session_id
            || welcome.client_ephemeral != self.hello.ephemeral
        {
            return Err(SecurityError::HandshakeBinding);
        }
        let key = VerifyingKey::from_bytes(&welcome.identity)
            .map_err(|_| SecurityError::HandshakeAuthentication)?;
        key.verify(
            &server_transcript(&self.hello, &welcome.ephemeral),
            &Signature::from_bytes(&welcome.signature),
        )
        .map_err(|_| SecurityError::HandshakeAuthentication)?;
        let shared = self
            .secret
            .diffie_hellman(&PublicKey::from(welcome.ephemeral));
        derive_session(
            shared.as_bytes(),
            self.hello.session_id,
            &handshake_transcript(&self.hello, welcome),
            Role::Client,
        )
    }
}

#[derive(Clone, Copy)]
enum Role {
    Client,
    Server,
}

fn client_transcript(session_id: u64, ephemeral: &[u8; 32], identity: &[u8; 32]) -> Vec<u8> {
    [
        HANDSHAKE_LABEL,
        b" client-hello ",
        &session_id.to_be_bytes(),
        ephemeral,
        identity,
    ]
    .concat()
}
fn server_transcript(hello: &ClientHello, ephemeral: &[u8; 32]) -> Vec<u8> {
    [
        HANDSHAKE_LABEL,
        b" server-welcome ",
        &hello.encode(),
        ephemeral,
    ]
    .concat()
}
fn handshake_transcript(hello: &ClientHello, welcome: &ServerWelcome) -> Vec<u8> {
    [
        HANDSHAKE_LABEL,
        b" traffic-keys ",
        &hello.encode(),
        &welcome.encode(),
    ]
    .concat()
}
fn derive_session(
    shared: &[u8],
    session_id: u64,
    transcript: &[u8],
    role: Role,
) -> Result<Session, SecurityError> {
    if shared.iter().all(|byte| *byte == 0) {
        return Err(SecurityError::HandshakeAuthentication);
    }
    let hkdf = Hkdf::<Sha256>::new(Some(&session_id.to_be_bytes()), shared);
    let mut material = [0_u8; 64];
    hkdf.expand(transcript, &mut material)
        .map_err(|_| SecurityError::KeyDerivation)?;
    let (client_key, server_key) = material.split_at(32);
    let (send, receive) = match role {
        Role::Client => (client_key, server_key),
        Role::Server => (server_key, client_key),
    };
    Ok(Session {
        send_cipher: ChaCha20Poly1305::new(Key::from_slice(send)),
        receive_cipher: ChaCha20Poly1305::new(Key::from_slice(receive)),
        id: session_id,
        epoch: 0,
        replay: ReplayWindow::default(),
    })
}

/// Encrypts data with a unique nonce derived from `(session_id, sequence)`.
pub struct Session {
    send_cipher: ChaCha20Poly1305,
    receive_cipher: ChaCha20Poly1305,
    id: u64,
    epoch: u16,
    replay: ReplayWindow,
}
impl Session {
    #[must_use]
    pub const fn epoch(&self) -> u16 {
        self.epoch
    }
    /// # Errors
    ///
    /// Returns an error only when authenticated encryption fails.
    pub fn seal(
        &self,
        sequence: u32,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let mut ciphertext = plaintext.to_vec();
        let tag = self
            .send_cipher
            .encrypt_in_place_detached(&nonce(self.id, sequence), associated_data, &mut ciphertext)
            .map_err(|_| SecurityError::Encryption)?;
        ciphertext.extend_from_slice(&tag);
        Ok(ciphertext)
    }
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
        self.receive_cipher
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
    /// # Errors
    ///
    /// Returns an error if packet framing or authenticated encryption fails.
    pub fn seal_fcdp(
        &self,
        mut header: Header,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        if header.session_id != self.id || header.epoch != self.epoch {
            return Err(SecurityError::SessionBinding);
        }
        header.flags |= ENCRYPTED_FLAG;
        let protected_len = plaintext.len().saturating_add(Tag::default().len());
        let aad = header
            .associated_data(protected_len)
            .map_err(|_| SecurityError::Packet)?;
        let ciphertext = self.seal(header.sequence_number, &aad, plaintext)?;
        let mut packet = Vec::new();
        header
            .encode(&ciphertext, &mut packet)
            .map_err(|_| SecurityError::Packet)?;
        Ok(packet)
    }
    /// # Errors
    ///
    /// Returns an error for invalid framing, plaintext, replay, or a failed tag.
    pub fn open_fcdp(&mut self, packet: &[u8]) -> Result<(Header, Vec<u8>), SecurityError> {
        let (header, ciphertext) = Header::decode(packet).map_err(|_| SecurityError::Packet)?;
        if header.session_id != self.id || header.epoch != self.epoch {
            return Err(SecurityError::SessionBinding);
        }
        if header.flags & ENCRYPTED_FLAG == 0 {
            return Err(SecurityError::Unencrypted);
        }
        let aad = header
            .associated_data(ciphertext.len())
            .map_err(|_| SecurityError::Packet)?;
        let plaintext = self.open(header.sequence_number, &aad, ciphertext)?;
        Ok((header, plaintext))
    }
}
fn nonce(session_id: u64, sequence: u32) -> Nonce {
    let mut value = [0_u8; 12];
    value[..8].copy_from_slice(&session_id.to_be_bytes());
    value[8..].copy_from_slice(&sequence.to_be_bytes());
    *Nonce::from_slice(&value)
}

/// A 64-packet sliding replay window.
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
    HandshakeEncoding,
    HandshakeAuthentication,
    HandshakeBinding,
    UnexpectedPeer,
    KeyDerivation,
    SessionBinding,
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
    fn connected() -> (Session, Session) {
        let client = Identity::generate();
        let server = Identity::generate();
        let initiator = client.begin_handshake(4);
        let (welcome, server_session, _) = server
            .accept_handshake(initiator.hello(), Some(client.public_key()))
            .unwrap();
        let client_session = initiator.complete(&welcome, server.public_key()).unwrap();
        (client_session, server_session)
    }
    #[test]
    fn handshake_authenticates_and_encrypts() {
        let (sender, mut receiver) = connected();
        let encrypted = sender.seal(7, b"header", b"media").unwrap();
        assert_eq!(receiver.open(7, b"header", &encrypted).unwrap(), b"media");
    }
    #[test]
    fn altered_or_wrong_peer_handshakes_are_rejected() {
        let client = Identity::generate();
        let server = Identity::generate();
        let imposter = Identity::generate();
        let initiator = client.begin_handshake(4);
        let (mut welcome, _, _) = server
            .accept_handshake(initiator.hello(), Some(client.public_key()))
            .unwrap();
        assert!(matches!(
            initiator.complete(&welcome, imposter.public_key()),
            Err(SecurityError::UnexpectedPeer)
        ));
        welcome.ephemeral[0] ^= 1;
        let next = client.begin_handshake(4);
        assert!(matches!(
            next.complete(&welcome, server.public_key()),
            Err(SecurityError::HandshakeBinding)
        ));
    }
    #[test]
    fn handshake_records_round_trip_and_signature_is_checked() {
        let client = Identity::generate();
        let server = Identity::generate();
        let initiator = client.begin_handshake(14);
        let decoded = ClientHello::decode(&initiator.hello().encode()).unwrap();
        let (welcome, _, _) = server
            .accept_handshake(&decoded, Some(client.public_key()))
            .unwrap();
        let decoded_welcome = ServerWelcome::decode(&welcome.encode()).unwrap();
        assert!(
            initiator
                .complete(&decoded_welcome, server.public_key())
                .is_ok()
        );
        let mut modified = decoded;
        modified.signature[0] ^= 1;
        assert!(matches!(
            server.accept_handshake(&modified, Some(client.public_key())),
            Err(SecurityError::HandshakeAuthentication)
        ));
    }
    #[test]
    fn modified_packets_and_replays_are_rejected() {
        let (sender, mut receiver) = connected();
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
        let (sender, mut receiver) = connected();
        let mut header = Header::new(PacketType::Media);
        header.session_id = 4;
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
    #[test]
    fn packet_must_be_bound_to_its_session() {
        let (sender, _) = connected();
        let mut header = Header::new(PacketType::Media);
        header.session_id = 5;
        assert!(matches!(
            sender.seal_fcdp(header, b"video"),
            Err(SecurityError::SessionBinding)
        ));
    }
}
