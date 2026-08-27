#![forbid(unsafe_code)]

use core::fmt;

pub const HEADER_LEN: usize = 40;
pub const VERSION: u8 = 1;
pub const REGION_COUNT: u16 = 2700;
pub const MAX_FRAGMENTS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AtomType {
    Motion = 1,
    Surface = 2,
    Detail = 3,
    Refresh = 4,
    Repair = 5,
    GroupRepair = 6,
    AudioPcm = 0x20,
    Ping = 0x40,
    Pong = 0x41,
    StateDigest = 0x42,
    NetworkMetrics = 0x43,
}
impl TryFrom<u8> for AtomType {
    type Error = Error;
    fn try_from(v: u8) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::Motion),
            2 => Ok(Self::Surface),
            3 => Ok(Self::Detail),
            4 => Ok(Self::Refresh),
            5 => Ok(Self::Repair),
            6 => Ok(Self::GroupRepair),
            0x20 => Ok(Self::AudioPcm),
            0x40 => Ok(Self::Ping),
            0x41 => Ok(Self::Pong),
            0x42 => Ok(Self::StateDigest),
            0x43 => Ok(Self::NetworkMetrics),
            _ => Err(Error::UnknownAtomType),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub atom_type: AtomType,
    pub flags: u16,
    pub session_epoch: u32,
    pub atom_sequence: u32,
    pub frame_tick: u32,
    pub region_id: u16,
    pub fragment_index: u8,
    pub fragment_count: u8,
    pub state_id: u32,
    pub base_state_id: u32,
    pub capture_time_ms: u32,
    pub ttl_ms: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Magic,
    Version,
    HeaderLength,
    PayloadLength,
    Region,
    Fragment,
    Ttl,
    UnknownAtomType,
    Surface,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid FCST datagram: {self:?}")
    }
}
impl std::error::Error for Error {}

impl Header {
    pub fn decode(datagram: &[u8]) -> Result<(Self, &[u8]), Error> {
        if datagram.len() < HEADER_LEN {
            return Err(Error::Truncated);
        }
        if datagram[..2] != [0xfc, 0x01] {
            return Err(Error::Magic);
        }
        if datagram[2] != VERSION {
            return Err(Error::Version);
        }
        if u16::from_be_bytes([datagram[6], datagram[7]]) as usize != HEADER_LEN {
            return Err(Error::HeaderLength);
        }
        let payload_len = u16::from_be_bytes([datagram[38], datagram[39]]) as usize;
        if datagram.len() != HEADER_LEN + payload_len {
            return Err(Error::PayloadLength);
        }
        let header = Self {
            atom_type: AtomType::try_from(datagram[3])?,
            flags: u16::from_be_bytes([datagram[4], datagram[5]]),
            session_epoch: u32::from_be_bytes(datagram[8..12].try_into().expect("fixed range")),
            atom_sequence: u32::from_be_bytes(datagram[12..16].try_into().expect("fixed range")),
            frame_tick: u32::from_be_bytes(datagram[16..20].try_into().expect("fixed range")),
            region_id: u16::from_be_bytes([datagram[20], datagram[21]]),
            fragment_index: datagram[22],
            fragment_count: datagram[23],
            state_id: u32::from_be_bytes(datagram[24..28].try_into().expect("fixed range")),
            base_state_id: u32::from_be_bytes(datagram[28..32].try_into().expect("fixed range")),
            capture_time_ms: u32::from_be_bytes(datagram[32..36].try_into().expect("fixed range")),
            ttl_ms: u16::from_be_bytes([datagram[36], datagram[37]]),
        };
        header.validate()?;
        Ok((header, &datagram[HEADER_LEN..]))
    }
    pub fn validate(self) -> Result<(), Error> {
        if self.region_id >= REGION_COUNT {
            return Err(Error::Region);
        }
        if self.fragment_count == 0
            || self.fragment_count > MAX_FRAGMENTS
            || self.fragment_index >= self.fragment_count
        {
            return Err(Error::Fragment);
        }
        if self.ttl_ms == 0 {
            return Err(Error::Ttl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    pub quantization: u8,
    pub luma: [u8; 48],
    pub chroma_a: [i8; 12],
    pub chroma_b: [i8; 12],
    pub raw_rgb: Option<Vec<u8>>,
}
impl Surface {
    pub const LEN: usize = 73;
    pub fn decode(payload: &[u8]) -> Result<Self, Error> {
        if payload.first() == Some(&0xff) && payload.len() == 2305 {
            return Ok(Self {
                quantization: 0xff,
                luma: [0; 48],
                chroma_a: [0; 12],
                chroma_b: [0; 12],
                raw_rgb: Some(payload[1..].to_vec()),
            });
        }
        if payload.len() != Self::LEN {
            return Err(Error::Surface);
        }
        let mut luma = [0; 48];
        luma.copy_from_slice(&payload[1..49]);
        let mut chroma_a = [0; 12];
        let mut chroma_b = [0; 12];
        for (i, value) in chroma_a.iter_mut().enumerate() {
            *value = payload[49 + i] as i8;
        }
        for (i, value) in chroma_b.iter_mut().enumerate() {
            *value = payload[61 + i] as i8;
        }
        Ok(Self {
            quantization: payload[0],
            luma,
            chroma_a,
            chroma_b,
            raw_rgb: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_lengths() {
        assert_eq!(Header::decode(&[]), Err(Error::Truncated));
    }
    #[test]
    fn decodes_surface() {
        let mut payload = [0_u8; 73];
        payload[0] = 1;
        payload[1] = 8;
        assert_eq!(Surface::decode(&payload).unwrap().luma[0], 8);
    }
}
