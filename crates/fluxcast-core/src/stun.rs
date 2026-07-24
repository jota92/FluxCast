//! Shared STUN/TURN (RFC 5389 / RFC 8489) message framing.
//!
//! Both the TURN client and the ICE connectivity-check agent build and parse
//! the same message shape: a 20-byte header, type-length-value attributes padded
//! to four bytes, and an HMAC-SHA1 `MESSAGE-INTEGRITY` computed over the message
//! with its length field pointing past the integrity attribute. Keeping one
//! codec avoids two subtly different STUN implementations.

use std::net::SocketAddr;

use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha1::Sha1;

pub(crate) const MAGIC_COOKIE: u32 = 0x2112_A442;

pub(crate) const ATTR_USERNAME: u16 = 0x0006;
pub(crate) const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
pub(crate) const ATTR_ERROR_CODE: u16 = 0x0009;
pub(crate) const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

type HmacSha1 = Hmac<Sha1>;

/// Incremental STUN/TURN message builder.
pub(crate) struct MessageBuilder {
    message_type: u16,
    pub(crate) transaction_id: [u8; 12],
    attributes: Vec<u8>,
}

impl MessageBuilder {
    pub(crate) fn new(message_type: u16) -> Self {
        let mut transaction_id = [0u8; 12];
        OsRng.fill_bytes(&mut transaction_id);
        Self::with_transaction_id(message_type, transaction_id)
    }

    pub(crate) fn with_transaction_id(message_type: u16, transaction_id: [u8; 12]) -> Self {
        Self {
            message_type,
            transaction_id,
            attributes: Vec::new(),
        }
    }

    pub(crate) fn add_attribute(&mut self, attribute_type: u16, value: &[u8]) {
        self.attributes
            .extend_from_slice(&attribute_type.to_be_bytes());
        self.attributes
            .extend_from_slice(&u16::try_from(value.len()).unwrap_or(u16::MAX).to_be_bytes());
        self.attributes.extend_from_slice(value);
        while self.attributes.len() % 4 != 0 {
            self.attributes.push(0);
        }
    }

    pub(crate) fn add_xor_address(
        &mut self,
        attribute_type: u16,
        address: SocketAddr,
        txid: &[u8; 12],
    ) {
        self.add_attribute(attribute_type, &encode_xor_address(address, txid));
    }

    /// Appends MESSAGE-INTEGRITY (HMAC-SHA1) over the message so far, with the
    /// header length set to include the integrity attribute.
    pub(crate) fn add_message_integrity(&mut self, key: &[u8]) {
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

    pub(crate) fn finish(&self) -> Vec<u8> {
        let mut message = self.header(u16::try_from(self.attributes.len()).unwrap_or(u16::MAX));
        message.extend_from_slice(&self.attributes);
        message
    }
}

/// A parsed STUN/TURN message.
pub(crate) struct Message {
    pub(crate) kind: u16,
    pub(crate) transaction_id: [u8; 12],
    attributes: Vec<(u16, Vec<u8>)>,
}

impl Message {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 20
            || u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) != MAGIC_COOKIE
        {
            return None;
        }
        let kind = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
        let mut transaction_id = [0u8; 12];
        transaction_id.copy_from_slice(&bytes[8..20]);
        let body = bytes.get(20..20 + length)?;
        let mut attributes = Vec::new();
        let mut cursor = 0;
        while cursor + 4 <= body.len() {
            let attribute_type = u16::from_be_bytes([body[cursor], body[cursor + 1]]);
            let value_len = usize::from(u16::from_be_bytes([body[cursor + 2], body[cursor + 3]]));
            let start = cursor + 4;
            let end = start
                .checked_add(value_len)
                .filter(|end| *end <= body.len())?;
            attributes.push((attribute_type, body[start..end].to_vec()));
            cursor = end + ((4 - (value_len % 4)) % 4);
        }
        Some(Self {
            kind,
            transaction_id,
            attributes,
        })
    }

    pub(crate) fn attribute(&self, attribute_type: u16) -> Option<&[u8]> {
        self.attributes
            .iter()
            .find(|(kind, _)| *kind == attribute_type)
            .map(|(_, value)| value.as_slice())
    }

    pub(crate) fn xor_address(&self, attribute_type: u16) -> Option<SocketAddr> {
        decode_xor_address(self.attribute(attribute_type)?, &self.transaction_id)
    }

    pub(crate) fn error_code(&self) -> Option<u16> {
        let value = self.attribute(ATTR_ERROR_CODE)?;
        let class = u16::from(*value.get(2)? & 0x7);
        let number = u16::from(*value.get(3)?);
        Some(class * 100 + number)
    }

    /// Verifies MESSAGE-INTEGRITY over the raw `datagram` bytes using `key`.
    /// `datagram` must be the exact bytes this message was parsed from.
    pub(crate) fn verify_integrity(&self, datagram: &[u8], key: &[u8]) -> bool {
        let Some(mi) = self.attribute(ATTR_MESSAGE_INTEGRITY) else {
            return false;
        };
        if mi.len() != 20 {
            return false;
        }
        // The MESSAGE-INTEGRITY attribute is the last 24 bytes on the wire.
        let Some(mi_offset) = datagram.len().checked_sub(24) else {
            return false;
        };
        let mut signed = datagram[..mi_offset].to_vec();
        let claimed_len = (mi_offset - 20) + 24;
        signed[2..4].copy_from_slice(&u16::try_from(claimed_len).unwrap_or(u16::MAX).to_be_bytes());
        let mut mac = HmacSha1::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&signed);
        mac.verify_slice(mi).is_ok()
    }
}

pub(crate) fn encode_xor_address(address: SocketAddr, txid: &[u8; 12]) -> Vec<u8> {
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

pub(crate) fn decode_xor_address(value: &[u8], txid: &[u8; 12]) -> Option<SocketAddr> {
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
    fn xor_ipv4_round_trips() {
        let txid = [3u8; 12];
        let address: SocketAddr = "203.0.113.5:51234".parse().unwrap();
        assert_eq!(
            decode_xor_address(&encode_xor_address(address, &txid), &txid),
            Some(address)
        );
    }

    #[test]
    fn xor_ipv6_round_trips() {
        let txid = [9u8; 12];
        let address: SocketAddr = "[2001:db8::1]:9000".parse().unwrap();
        assert_eq!(
            decode_xor_address(&encode_xor_address(address, &txid), &txid),
            Some(address)
        );
    }

    #[test]
    fn message_integrity_builds_and_verifies() {
        let mut builder = MessageBuilder::new(0x0001);
        builder.add_attribute(ATTR_USERNAME, b"alice:bob");
        let key = b"password";
        builder.add_message_integrity(key);
        let bytes = builder.finish();
        let message = Message::parse(&bytes).unwrap();
        assert!(message.verify_integrity(&bytes, key));
        assert!(!message.verify_integrity(&bytes, b"wrong-key"));
    }

    #[test]
    fn parse_rejects_non_stun() {
        assert!(Message::parse(&[0u8; 8]).is_none());
        assert!(Message::parse(&[0xff; 40]).is_none());
    }
}
