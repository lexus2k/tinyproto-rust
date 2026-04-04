/*
    Copyright 2024 (C) Alexey Dynda

    This file is part of Tiny Protocol Library.

    GNU General Public License Usage

    Protocol Library is free software: you can redistribute it and/or modify
    it under the terms of the GNU Lesser General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    Protocol Library is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Lesser General Public License for more details.

    You should have received a copy of the GNU Lesser General Public License
    along with Protocol Library.  If not, see <http://www.gnu.org/licenses/>.

    Commercial License Usage

    Licensees holding valid commercial Tiny Protocol licenses may use this file in
    accordance with the commercial license agreement provided in accordance with
    the terms contained in a written agreement between you and Alexey Dynda.
    For further information contact via email on github account.
*/

use crate::proto::crc;
// use std::thread::sleep;
// use std::time::Duration;

/** Byte to fill gap between frames */
const TINY_HDLC_FILL_BYTE: u8 = 0xFF;
/** Escape character */
const TINY_HDLC_ESCAPE_CHAR: u8 = 0x7D;
/** Escape bit */
const TINY_HDLC_ESCAPE_BIT: u8 = 0x20;
/** Start or end of frame */
const TINY_HDLC_FLAG_SEQUENCE: u8 = 0x7E;

/// Result codes for one-shot HDLC encode/decode operations.
#[derive(PartialEq, Eq, Debug)]
pub enum ResultT {
    /// Operation completed successfully.
    Success,
    /// Input data is invalid or incomplete.
    InvalidData,
    /// Generic error.
    Error,
    /// Encoder/decoder is busy.
    Busy,
    /// Payload exceeds the configured MTU.
    DataTooLarge,
    /// CRC verification failed.
    WrongCrc,
}


/// One-shot HDLC encoder/decoder.
///
/// Encodes raw data into HDLC-framed bytes (with byte-stuffing and CRC),
/// and decodes HDLC-framed bytes back into raw payload.
/// Unlike [`super::low_level_ll::HdlcLl`], this operates on complete
/// buffers rather than streaming byte-by-byte.
pub struct HdlcEncoder {
    crc_type: crc::HdlcCrcT,
}

impl HdlcEncoder {
    /// Create a new HDLC encoder/decoder with the given CRC type and MTU.
    pub fn new(_crc_type: crc::HdlcCrcT, _mtu: isize) -> HdlcEncoder {
        HdlcEncoder {
            crc_type: _crc_type,
        }
    }

    ///
    /// This function encodes binary data to HDLC frame
    /// The output buffer should be at least 2 times bigger than input buffer
    /// The function returns number of bytes written to output buffer
    ///
    /// # Arguments
    /// * `data` - input data to encode
    /// * `result` - output buffer to store encoded data
    /// * `result_size` - size of output buffer
    ///
    pub fn encode(&self, data: &[u8], result: &mut Vec<u8>) -> ResultT {
        let mut crc: u32 = 0;
        result.truncate(0);
        match self.crc_type {
            crc::HdlcCrcT::HdlcCrc8 => {
                let mut crc8 = crc::Crc8::new();
                crc8.sum_bytes(data, data.len());
                crc = crc8.get() as u32;
            }
            crc::HdlcCrcT::HdlcCrc16 => {
                let mut crc16 = crc::Crc16::new();
                crc16.sum_bytes(data, data.len());
                crc = crc16.get() as u32;
            }
            crc::HdlcCrcT::HdlcCrc32 => {
                let mut crc32 = crc::Crc32::new();
                crc32.sum_bytes(data, data.len());
                crc = crc32.get();
            }
            _ => {
            }
        }
        result.push(TINY_HDLC_FLAG_SEQUENCE);
        for byte in data {
            if *byte == TINY_HDLC_FLAG_SEQUENCE || *byte == TINY_HDLC_ESCAPE_CHAR {
                result.push(TINY_HDLC_ESCAPE_CHAR);
                result.push(*byte ^ TINY_HDLC_ESCAPE_BIT);
            } else {
                result.push(*byte);
            }
        }
        let crc_size = crc::get_crc_field_size(self.crc_type);
        for i in 0..crc_size {
            let byte = (crc >> (i * 8)) as u8;
            if byte == TINY_HDLC_FLAG_SEQUENCE || byte == TINY_HDLC_ESCAPE_CHAR {
                result.push(TINY_HDLC_ESCAPE_CHAR);
                result.push(byte ^ TINY_HDLC_ESCAPE_BIT);
            } else {
                result.push(byte);
            }
        }
        result.push(TINY_HDLC_FLAG_SEQUENCE);
        ResultT::Success
    }

    /// Decode an HDLC-framed byte stream into a raw payload.
    ///
    /// Returns `(bytes_consumed, result)`. On success the decoded payload
    /// (without CRC) is placed into `result`. The frame must start and end
    /// with the HDLC flag sequence (`0x7E`).
    pub fn decode(&self, data: &[u8], result:&mut Vec<u8>) -> (usize, ResultT) {
        let mut consumed = 0;
        let mut escape: bool = false;
        let mut crc: u32 = 0;
        let crc_size = crc::get_crc_field_size(self.crc_type);
        result.truncate(0);
        for byte in data {
            consumed += 1;
            if *byte == TINY_HDLC_FLAG_SEQUENCE {
                if result.len() != 0 {
                    break;
                }
                continue;
            }
            if *byte == TINY_HDLC_ESCAPE_CHAR {
                escape = true;
                continue;
            }
            if escape {
                escape = false;
                result.push(*byte ^ TINY_HDLC_ESCAPE_BIT);
            } else {
                result.push(*byte);
            }
        }
        if result.len() < crc_size {
            return (consumed, ResultT::InvalidData);
        }
        match self.crc_type {
            crc::HdlcCrcT::HdlcCrc8 => {
                let mut crc8 = crc::Crc8::new();
                crc8.sum_bytes(result, result.len() - crc_size);
                crc = crc8.get() as u32;
            }
            crc::HdlcCrcT::HdlcCrc16 => {
                let mut crc16 = crc::Crc16::new();
                crc16.sum_bytes(result, result.len() - crc_size);
                crc = crc16.get() as u32;
            }
            crc::HdlcCrcT::HdlcCrc32 => {
                let mut crc32 = crc::Crc32::new();
                crc32.sum_bytes(result, result.len() - crc_size);
                crc = crc32.get();
            }
            _ => {
            }
        }
        let mut crc_data: u32 = 0;
        for i in 0..crc_size {
            crc_data |= (result[result.len() - crc_size + i] as u32) << (i * 8);
        }
        if crc != crc_data {
            // Return empty vector
            return (consumed, ResultT::WrongCrc);
        }
        result.truncate(result.len() - crc_size);
        // return data without crc
        // result.truncate(result_index - crc_size);
        (consumed, ResultT::Success)
    }
}

#[cfg(test)]
mod unittest {
    use super::*;
    // use std::cmp;

    #[test]
    fn test_encoder() {
        let encoder = HdlcEncoder::new(crc::HdlcCrcT::HdlcCrcOff, 0);
        let data: [u8; 4] = [0x7F, 0x7E, 0x7D, 0x00];
        let mut encoded = Vec::new();
        let result = encoder.encode(&data, &mut encoded);
        let expected: [u8; 8] = [0x7E, 0x7F, 0x7D, 0x5E, 0x7D, 0x5D, 0x00, 0x7E];
        assert_eq!(result, ResultT::Success, "Encoding failed");
        assert_eq!(encoded.len(), expected.len(), "Special bytes mismatch");
        assert_eq!(encoded, expected, "Arrays are not equal");

        let mut decoded = Vec::new();
        let (used,result) = encoder.decode(&encoded, &mut decoded);
        assert_eq!(result, ResultT::Success, "Decoding failed");
        assert_eq!(used, encoded.len(), "Decoding failed");
        assert_eq!(decoded.len(), data.len(), "Special bytes mismatch");
        assert_eq!(decoded, data, "Arrays are not equal");
    }
}

