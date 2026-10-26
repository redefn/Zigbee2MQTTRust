//! Z-Stack ZNP (Monitor and Test API) framing.
//!
//! Wire format: `0xFE | len | cmd0 | cmd1 | data[len] | fcs`, where `fcs` is the
//! XOR of every byte from `len` through the last data byte. `cmd0` carries the
//! frame type in its top 3 bits and the subsystem in the low 5 bits.

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

pub const SOF: u8 = 0xFE;
pub const MAX_DATA_LEN: usize = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Poll = 0,
    Sreq = 1,
    Areq = 2,
    Srsp = 3,
}

impl FrameType {
    fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::Poll),
            1 => Some(Self::Sreq),
            2 => Some(Self::Areq),
            3 => Some(Self::Srsp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsystem {
    Rpc = 0,
    Sys = 1,
    Mac = 2,
    Nwk = 3,
    Af = 4,
    Zdo = 5,
    Sapi = 6,
    Util = 7,
    Debug = 8,
    App = 9,
    AppCnf = 15,
    Zgp = 21,
}

impl Subsystem {
    fn from_bits(bits: u8) -> Option<Self> {
        Some(match bits {
            0 => Self::Rpc,
            1 => Self::Sys,
            2 => Self::Mac,
            3 => Self::Nwk,
            4 => Self::Af,
            5 => Self::Zdo,
            6 => Self::Sapi,
            7 => Self::Util,
            8 => Self::Debug,
            9 => Self::App,
            15 => Self::AppCnf,
            21 => Self::Zgp,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub frame_type: FrameType,
    pub subsystem: Subsystem,
    pub command: u8,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(frame_type: FrameType, subsystem: Subsystem, command: u8, data: Vec<u8>) -> Self {
        Self {
            frame_type,
            subsystem,
            command,
            data,
        }
    }

    fn cmd0(&self) -> u8 {
        ((self.frame_type as u8) << 5) | (self.subsystem as u8)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("frame data too long: {0} bytes")]
    TooLong(usize),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

fn fcs(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |acc, b| acc ^ b)
}

/// Codec for a byte stream to/from the coordinator. Corrupt or unknown frames
/// are skipped by resyncing on the next `SOF`, as Z-Stack hosts usually do.
#[derive(Debug, Default)]
pub struct ZnpCodec;

impl Decoder for ZnpCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, Error> {
        loop {
            match src.iter().position(|&b| b == SOF) {
                Some(pos) => src.advance(pos),
                None => {
                    src.clear();
                    return Ok(None);
                }
            }
            if src.len() < 5 {
                return Ok(None);
            }
            let len = src[1] as usize;
            let total = 5 + len;
            if src.len() < total {
                src.reserve(total - src.len());
                return Ok(None);
            }
            let (cmd0, cmd1) = (src[2], src[3]);
            let valid = fcs(&src[1..total - 1]) == src[total - 1];
            let frame_type = FrameType::from_bits(cmd0 >> 5);
            let subsystem = Subsystem::from_bits(cmd0 & 0x1F);
            match (valid, frame_type, subsystem) {
                (true, Some(frame_type), Some(subsystem)) => {
                    let data = src[4..4 + len].to_vec();
                    src.advance(total);
                    return Ok(Some(Frame::new(frame_type, subsystem, cmd1, data)));
                }
                _ => src.advance(1),
            }
        }
    }
}

impl Encoder<Frame> for ZnpCodec {
    type Error = Error;

    fn encode(&mut self, frame: Frame, dst: &mut BytesMut) -> Result<(), Error> {
        if frame.data.len() > MAX_DATA_LEN {
            return Err(Error::TooLong(frame.data.len()));
        }
        let start = dst.len();
        dst.reserve(5 + frame.data.len());
        dst.put_u8(SOF);
        dst.put_u8(frame.data.len() as u8);
        dst.put_u8(frame.cmd0());
        dst.put_u8(frame.command);
        dst.put_slice(&frame.data);
        let checksum = fcs(&dst[start + 1..]);
        dst.put_u8(checksum);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SYS_PING request as sent by zigbee-herdsman.
    const SYS_PING: [u8; 5] = [0xFE, 0x00, 0x21, 0x01, 0x20];

    #[test]
    fn encodes_sys_ping() {
        let mut buf = BytesMut::new();
        ZnpCodec
            .encode(
                Frame::new(FrameType::Sreq, Subsystem::Sys, 0x01, vec![]),
                &mut buf,
            )
            .unwrap();
        assert_eq!(&buf[..], &SYS_PING);
    }

    #[test]
    fn decodes_after_garbage_and_waits_for_partial_frame() {
        let mut buf = BytesMut::from(&[0x00, 0x13, 0xFE, 0x02, 0x61, 0x01][..]);
        assert_eq!(ZnpCodec.decode(&mut buf).unwrap(), None);
        // SYS_PING response with capabilities 0x0179.
        buf.extend_from_slice(&[0x79, 0x01, 0x1A]);
        let frame = ZnpCodec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(
            frame,
            Frame::new(FrameType::Srsp, Subsystem::Sys, 0x01, vec![0x79, 0x01])
        );
        assert!(buf.is_empty());
    }

    #[test]
    fn skips_frame_with_bad_checksum() {
        let mut buf = BytesMut::from(&[0xFE, 0x00, 0x21, 0x01, 0x99][..]);
        buf.extend_from_slice(&SYS_PING);
        let frame = ZnpCodec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame.command, 0x01);
        assert!(buf.is_empty());
    }
}
