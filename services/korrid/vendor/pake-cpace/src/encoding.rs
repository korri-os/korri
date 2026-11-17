use crate::{Error, Packet, SessionID, BIG_Y_BYTES, SESSION_ID_BYTES};
use alloc::vec::Vec;
use core::convert::TryInto;

pub trait NetworkEncoder: Sized {
    fn network_encode_with(&self, data: &Packet) -> Vec<u8>;
    fn network_decode_from(msg: &[u8]) -> Result<(Self, Packet), Error>;
}

#[derive(Debug, Copy, Clone)]
pub struct SessionAD {
    pub session_id: SessionID,
}

impl NetworkEncoder for SessionAD {
    fn network_encode_with(&self, data: &Packet) -> Vec<u8> {
        [&self.session_id[..], data].concat()
    }

    fn network_decode_from(msg: &[u8]) -> Result<(Self, Packet), Error> {
        if msg.len() != SESSION_ID_BYTES + BIG_Y_BYTES {
            return Err(Error::InvalidMsgSize);
        }

        let mut session_id = [0u8; SESSION_ID_BYTES];
        session_id.copy_from_slice(&msg[..SESSION_ID_BYTES]);

        let mut big_y = [0u8; BIG_Y_BYTES];
        big_y.copy_from_slice(&msg[SESSION_ID_BYTES..]);

        Ok((SessionAD { session_id }, big_y))
    }
}

pub struct NoAD {}

impl NetworkEncoder for NoAD {
    fn network_encode_with(&self, data: &Packet) -> Vec<u8> {
        data.to_vec()
    }

    fn network_decode_from(msg: &[u8]) -> Result<(Self, Packet), Error> {
        let big_y = msg.try_into().map_err(|_| Error::InvalidMsgSize)?;

        Ok((NoAD {}, big_y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_sizes_are_checked() {
        // Too short
        assert!(SessionAD::network_decode_from(&[1, 2, 3]).is_err());
        assert!(NoAD::network_decode_from(&[1, 2, 3]).is_err());

        // Too long
        assert!(SessionAD::network_decode_from(&[42u8; 200]).is_err());
        assert!(NoAD::network_decode_from(&[42u8; 200]).is_err());
    }
}
