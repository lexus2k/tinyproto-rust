/*
    Copyright 2024-2026 (C) Alexey Dynda

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

//! Streaming HDLC low-level protocol implementation.
//!
//! This module implements the stateful HDLC framing layer matching the C library's
//! `hdlc_ll_*` API. It processes data byte-by-byte via `run_rx()` and `run_tx()`,
//! supporting streaming operation suitable for use with hardware channels.
//!
//! This is the foundation layer used by HDLC high-level, Light, and FD protocols.

use crate::proto::crc::{self, HdlcCrcT};
use crate::proto::error::{TinyError, TinyResult};

const FLAG_SEQUENCE: u8 = 0x7E;
const FILL_BYTE: u8 = 0xFF;
const ESCAPE_CHAR: u8 = 0x7D;
const ESCAPE_BIT: u8 = 0x20;

/// Reset flags for HdlcLl
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetFlags {
    Both,
    TxOnly,
    RxOnly,
}

/// RX state machine phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RxState {
    Start,
    Data,
    End,
}

/// TX state machine phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxState {
    Start,
    Data,
    Crc,
    End,
}

/// Configuration for initializing HdlcLl
pub struct HdlcLlInit {
    /// CRC type to use
    pub crc_type: HdlcCrcT,
    /// Maximum transmission unit (0 = use buffer size)
    pub mtu: usize,
    /// RX buffer size
    pub rx_buf_size: usize,
}

impl Default for HdlcLlInit {
    fn default() -> Self {
        HdlcLlInit {
            crc_type: HdlcCrcT::HdlcCrc16,
            mtu: 0,
            rx_buf_size: 256,
        }
    }
}

/// Low-level HDLC framing state machine.
///
/// Processes bytes incrementally via `run_rx()` and `run_tx()`.
/// Calls user callbacks when frames are received or sent.
pub struct HdlcLl {
    // CRC configuration
    crc_type: HdlcCrcT,
    // Physical MTU including CRC
    phys_mtu: usize,

    // RX state
    rx_buf: Vec<u8>,
    rx_state: RxState,
    rx_ptr: usize,
    rx_escape: bool,

    // TX state
    tx_state: TxState,
    tx_data: Option<Vec<u8>>,
    tx_pos: usize,
    tx_crc: u32,
    tx_crc_bits_sent: u8,
    tx_escape: bool,

    // Received frames queue
    rx_frames: Vec<Vec<u8>>,
    // Sent frame notifications
    tx_sent_frames: Vec<Vec<u8>>,
}

impl HdlcLl {
    /// Create a new HDLC low-level instance
    pub fn new(init: &HdlcLlInit) -> TinyResult<HdlcLl> {
        if init.rx_buf_size == 0 {
            return Err(TinyError::InvalidData);
        }

        let crc_size = crc::get_crc_field_size(init.crc_type);
        let phys_mtu = if init.mtu > 0 {
            init.mtu + crc_size
        } else {
            init.rx_buf_size
        };

        Ok(HdlcLl {
            crc_type: init.crc_type,
            phys_mtu,
            rx_buf: Vec::with_capacity(init.rx_buf_size),
            rx_state: RxState::Start,
            rx_ptr: 0,
            rx_escape: false,
            tx_state: TxState::Start,
            tx_data: None,
            tx_pos: 0,
            tx_crc: 0,
            tx_crc_bits_sent: 0,
            tx_escape: false,
            rx_frames: Vec::new(),
            tx_sent_frames: Vec::new(),
        })
    }

    /// Reset the state machine
    pub fn reset(&mut self, flags: ResetFlags) {
        if flags != ResetFlags::TxOnly {
            self.rx_state = RxState::Start;
            self.rx_buf.clear();
            self.rx_ptr = 0;
            self.rx_escape = false;
        }
        if flags != ResetFlags::RxOnly {
            self.tx_data = None;
            self.tx_pos = 0;
            self.tx_escape = false;
            self.tx_state = TxState::Start;
            self.tx_crc_bits_sent = 0;
        }
    }

    /// Queue a frame for transmission.
    ///
    /// The frame data is copied internally. Returns `Err(Busy)` if a frame
    /// is already queued for sending.
    pub fn put_frame(&mut self, data: &[u8]) -> TinyResult<()> {
        if data.is_empty() {
            return Ok(());
        }
        if self.tx_data.is_some() {
            return Err(TinyError::Busy);
        }
        self.tx_data = Some(data.to_vec());
        self.tx_pos = 0;
        Ok(())
    }

    /// Check if TX is busy sending a frame
    pub fn is_tx_busy(&self) -> bool {
        self.tx_data.is_some()
    }

    /// Process incoming data, returning number of bytes consumed.
    ///
    /// Received frames are buffered internally. Use `get_rx_frame()` to retrieve them.
    pub fn run_rx(&mut self, data: &[u8]) -> (usize, Option<TinyError>) {
        let mut consumed = 0;
        let mut pos = 0;
        let mut error = None;

        while pos < data.len() || self.rx_state == RxState::End {
            match self.rx_state {
                RxState::Start => {
                    if pos >= data.len() {
                        break;
                    }
                    let byte = data[pos];
                    pos += 1;
                    consumed += 1;
                    if byte == FLAG_SEQUENCE {
                        self.rx_escape = false;
                        self.rx_buf.clear();
                        self.rx_ptr = 0;
                        self.rx_state = RxState::Data;
                    }
                    // Skip fill bytes and other non-flag bytes
                }
                RxState::Data => {
                    if pos >= data.len() {
                        break;
                    }
                    let byte = data[pos];
                    pos += 1;
                    consumed += 1;

                    if byte == FLAG_SEQUENCE {
                        self.rx_state = RxState::End;
                        continue;
                    }
                    if byte == ESCAPE_CHAR {
                        self.rx_escape = true;
                        continue;
                    }
                    let decoded = if self.rx_escape {
                        self.rx_escape = false;
                        byte ^ ESCAPE_BIT
                    } else {
                        byte
                    };

                    if self.rx_ptr < self.phys_mtu {
                        self.rx_buf.push(decoded);
                        self.rx_ptr += 1;
                    }
                    // else: silently drop bytes exceeding MTU
                }
                RxState::End => {
                    if self.rx_buf.is_empty() {
                        // Empty frame — alignment issue, go back to data state
                        self.rx_escape = false;
                        self.rx_state = RxState::Data;
                        continue;
                    }
                    self.rx_state = RxState::Start;

                    let len = self.rx_buf.len();
                    if len > self.phys_mtu {
                        error = Some(TinyError::DataTooLarge);
                        break;
                    }

                    let crc_size = crc::get_crc_field_size(self.crc_type);
                    if len < crc_size {
                        error = Some(TinyError::WrongCrc);
                        break;
                    }

                    // Verify CRC
                    let payload_len = len - crc_size;
                    let calc_crc = self.calculate_crc(&self.rx_buf[..payload_len]);
                    let read_crc = self.read_crc_from_buffer(payload_len);

                    if calc_crc != read_crc {
                        error = Some(TinyError::WrongCrc);
                        break;
                    }

                    // Frame is valid - strip CRC and store
                    let mut frame = std::mem::take(&mut self.rx_buf);
                    frame.truncate(payload_len);
                    self.rx_frames.push(frame);
                    self.rx_buf = Vec::with_capacity(self.phys_mtu);
                }
            }
        }
        (consumed, error)
    }

    /// Retrieve the next received frame, if any
    pub fn get_rx_frame(&mut self) -> Option<Vec<u8>> {
        if self.rx_frames.is_empty() {
            None
        } else {
            Some(self.rx_frames.remove(0))
        }
    }

    /// Fill output buffer with TX data, returning the number of bytes written.
    ///
    /// Call this repeatedly to get encoded frame bytes to send to the hardware channel.
    pub fn run_tx(&mut self, out: &mut [u8]) -> usize {
        let mut written = 0;

        while written < out.len() {
            let prev_state = self.tx_state;
            let result = match self.tx_state {
                TxState::Start => self.tx_send_start(&mut out[written..]),
                TxState::Data => self.tx_send_data(&mut out[written..]),
                TxState::Crc => self.tx_send_crc(&mut out[written..]),
                TxState::End => self.tx_send_end(&mut out[written..]),
            };

            if result == 0 {
                // Only break if state didn't change (truly stuck)
                if self.tx_state == prev_state {
                    break;
                }
                // State transitioned without output — keep going
            } else {
                written += result;
            }
        }
        written
    }

    /// Retrieve notification of a sent frame (the original data), if any
    pub fn get_tx_sent_frame(&mut self) -> Option<Vec<u8>> {
        if self.tx_sent_frames.is_empty() {
            None
        } else {
            Some(self.tx_sent_frames.remove(0))
        }
    }

    // --- TX state machine internal methods ---

    fn tx_send_start(&mut self, out: &mut [u8]) -> usize {
        if self.tx_data.is_none() {
            return 0;
        }
        if out.is_empty() {
            return 0;
        }

        // Calculate CRC for the frame
        let data = self.tx_data.as_ref().unwrap();
        self.tx_crc = self.calculate_crc(data);

        out[0] = FLAG_SEQUENCE;
        self.tx_state = TxState::Data;
        self.tx_pos = 0;
        self.tx_escape = false;
        1
    }

    fn tx_send_data(&mut self, out: &mut [u8]) -> usize {
        if out.is_empty() {
            return 0;
        }
        let data = self.tx_data.as_ref().unwrap();
        if self.tx_pos >= data.len() {
            self.tx_state = TxState::Crc;
            self.tx_crc_bits_sent = 0;
            self.tx_escape = false;
            return 0;
        }

        let byte = data[self.tx_pos];
        if byte == FLAG_SEQUENCE || byte == ESCAPE_CHAR {
            if !self.tx_escape {
                out[0] = ESCAPE_CHAR;
                self.tx_escape = true;
                return 1;
            } else {
                out[0] = byte ^ ESCAPE_BIT;
                self.tx_escape = false;
                self.tx_pos += 1;
                return 1;
            }
        }

        // Try to write a run of non-special bytes
        let mut written = 0;
        while written < out.len() && self.tx_pos < data.len() {
            let b = data[self.tx_pos];
            if b == FLAG_SEQUENCE || b == ESCAPE_CHAR {
                break;
            }
            out[written] = b;
            written += 1;
            self.tx_pos += 1;
        }
        written
    }

    fn tx_send_crc(&mut self, out: &mut [u8]) -> usize {
        if out.is_empty() {
            return 0;
        }
        let crc_size = crc::get_crc_field_size(self.crc_type) as u8;
        if self.tx_crc_bits_sent >= crc_size * 8 {
            self.tx_state = TxState::End;
            return 0;
        }

        let byte = (self.tx_crc >> self.tx_crc_bits_sent) as u8;
        if byte == FLAG_SEQUENCE || byte == ESCAPE_CHAR {
            if !self.tx_escape {
                out[0] = ESCAPE_CHAR;
                self.tx_escape = true;
                return 1;
            } else {
                out[0] = byte ^ ESCAPE_BIT;
                self.tx_escape = false;
                self.tx_crc_bits_sent += 8;
                return 1;
            }
        }

        out[0] = byte;
        self.tx_crc_bits_sent += 8;
        1
    }

    fn tx_send_end(&mut self, out: &mut [u8]) -> usize {
        if out.is_empty() {
            return 0;
        }
        out[0] = FLAG_SEQUENCE;
        self.tx_state = TxState::Start;
        self.tx_escape = false;

        // Notify frame sent
        if let Some(frame) = self.tx_data.take() {
            self.tx_sent_frames.push(frame);
        }
        1
    }

    // --- CRC helpers ---

    fn calculate_crc(&self, data: &[u8]) -> u32 {
        match self.crc_type {
            HdlcCrcT::HdlcCrc8 => {
                let mut crc = crc::Crc8::new();
                crc.sum_bytes(data, data.len());
                crc.get() as u32
            }
            HdlcCrcT::HdlcCrc16 => {
                let mut crc = crc::Crc16::new();
                crc.sum_bytes(data, data.len());
                crc.get() as u32
            }
            HdlcCrcT::HdlcCrc32 | HdlcCrcT::HdlcCrcDefault => {
                let mut crc = crc::Crc32::new();
                crc.sum_bytes(data, data.len());
                crc.get()
            }
            HdlcCrcT::HdlcCrcOff => 0,
        }
    }

    fn read_crc_from_buffer(&self, payload_len: usize) -> u32 {
        let crc_size = crc::get_crc_field_size(self.crc_type);
        if crc_size == 0 {
            return 0;
        }
        let mut crc_val: u32 = 0;
        for i in 0..crc_size {
            crc_val |= (self.rx_buf[payload_len + i] as u32) << (i * 8);
        }
        crc_val
    }

    /// Returns the minimum buffer size required for a given MTU and CRC type
    pub fn get_buf_size(mtu: usize, crc_type: HdlcCrcT) -> usize {
        mtu + crc::get_crc_field_size(crc_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_ll(crc_type: HdlcCrcT) -> HdlcLl {
        HdlcLl::new(&HdlcLlInit {
            crc_type,
            mtu: 128,
            rx_buf_size: 256,
        }).unwrap()
    }

    #[test]
    fn test_basic_tx_rx_no_crc() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrcOff);
        let mut rx = create_ll(HdlcCrcT::HdlcCrcOff);

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        // Encoded: 7E 01 02 03 04 7E
        assert_eq!(buf[0], FLAG_SEQUENCE);
        assert_eq!(*buf.last().unwrap(), FLAG_SEQUENCE);

        let (consumed, err) = rx.run_rx(&buf);
        assert_eq!(consumed, written);
        assert!(err.is_none());

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    #[test]
    fn test_tx_rx_with_crc16() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrc16);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc16);

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        let (consumed, err) = rx.run_rx(&buf);
        assert_eq!(consumed, written);
        assert!(err.is_none());

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    #[test]
    fn test_tx_rx_with_crc32() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrc32);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc32);

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        let (consumed, err) = rx.run_rx(&buf);
        assert_eq!(consumed, written);
        assert!(err.is_none());

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    #[test]
    fn test_tx_rx_with_crc8() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrc8);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc8);

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        let (consumed, err) = rx.run_rx(&buf);
        assert_eq!(consumed, written);
        assert!(err.is_none());

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    #[test]
    fn test_escape_bytes() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrcOff);
        let mut rx = create_ll(HdlcCrcT::HdlcCrcOff);

        // Payload containing special bytes 0x7E and 0x7D
        let payload = vec![0x7F, 0x7E, 0x7D, 0x00];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        // Should be: 7E 7F 7D-5E 7D-5D 00 7E (8 bytes)
        assert_eq!(written, 8);
        assert_eq!(buf, vec![0x7E, 0x7F, 0x7D, 0x5E, 0x7D, 0x5D, 0x00, 0x7E]);

        let (consumed, err) = rx.run_rx(&buf);
        assert_eq!(consumed, written);
        assert!(err.is_none());

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    #[test]
    fn test_put_frame_busy() {
        let mut ll = create_ll(HdlcCrcT::HdlcCrcOff);
        ll.put_frame(&[1, 2, 3]).unwrap();
        assert_eq!(ll.put_frame(&[4, 5, 6]), Err(TinyError::Busy));
    }

    #[test]
    fn test_wrong_crc() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrc16);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc16);

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        // Corrupt a data byte
        buf[2] ^= 0xFF;

        let (_consumed, err) = rx.run_rx(&buf);
        assert_eq!(err, Some(TinyError::WrongCrc));
        assert!(rx.get_rx_frame().is_none());
    }

    #[test]
    fn test_byte_by_byte_rx() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrc16);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc16);

        let payload = vec![0xAA, 0xBB, 0xCC];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);

        // Feed byte-by-byte to RX
        let mut total_consumed = 0;
        for i in 0..written {
            let (consumed, err) = rx.run_rx(&buf[i..i + 1]);
            total_consumed += consumed;
            if err.is_some() {
                panic!("Unexpected error: {:?}", err);
            }
        }
        assert_eq!(total_consumed, written);

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    #[test]
    fn test_byte_by_byte_tx() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrcOff);
        let payload = vec![0x01, 0x02];
        tx.put_frame(&payload).unwrap();

        // Read TX one byte at a time
        let mut out = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let written = tx.run_tx(&mut byte);
            if written == 0 {
                break;
            }
            out.push(byte[0]);
        }
        // 7E 01 02 7E
        assert_eq!(out, vec![0x7E, 0x01, 0x02, 0x7E]);
    }

    #[test]
    fn test_reset() {
        let mut ll = create_ll(HdlcCrcT::HdlcCrcOff);
        ll.put_frame(&[1, 2, 3]).unwrap();
        assert!(ll.is_tx_busy());
        ll.reset(ResetFlags::TxOnly);
        assert!(!ll.is_tx_busy());
        ll.put_frame(&[4, 5, 6]).unwrap();
        assert!(ll.is_tx_busy());
    }

    #[test]
    fn test_multiple_frames() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrc16);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc16);

        let mut all_tx_bytes = Vec::new();

        // Send frame 1
        tx.put_frame(&[0x01, 0x02]).unwrap();
        let mut buf = vec![0u8; 64];
        let w = tx.run_tx(&mut buf);
        all_tx_bytes.extend_from_slice(&buf[..w]);

        // Send frame 2
        tx.put_frame(&[0x03, 0x04]).unwrap();
        let w = tx.run_tx(&mut buf);
        all_tx_bytes.extend_from_slice(&buf[..w]);

        // Feed all bytes to RX at once
        let (consumed, err) = rx.run_rx(&all_tx_bytes);
        assert!(err.is_none());
        // First frame
        let f1 = rx.get_rx_frame().unwrap();
        assert_eq!(f1, vec![0x01, 0x02]);

        // Feed remaining bytes
        if consumed < all_tx_bytes.len() {
            let (_, err) = rx.run_rx(&all_tx_bytes[consumed..]);
            assert!(err.is_none());
        }
        let f2 = rx.get_rx_frame().unwrap();
        assert_eq!(f2, vec![0x03, 0x04]);
    }

    #[test]
    fn test_tx_sent_notification() {
        let mut tx = create_ll(HdlcCrcT::HdlcCrcOff);
        let payload = vec![0x01, 0x02, 0x03];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let _ = tx.run_tx(&mut buf);

        let sent = tx.get_tx_sent_frame().unwrap();
        assert_eq!(sent, payload);
    }

    #[test]
    fn test_large_payload_all_crc_types() {
        for crc_type in [HdlcCrcT::HdlcCrcOff, HdlcCrcT::HdlcCrc8, HdlcCrcT::HdlcCrc16, HdlcCrcT::HdlcCrc32] {
            let mut tx = HdlcLl::new(&HdlcLlInit {
                crc_type,
                mtu: 256,
                rx_buf_size: 512,
            }).unwrap();
            let mut rx = HdlcLl::new(&HdlcLlInit {
                crc_type,
                mtu: 256,
                rx_buf_size: 512,
            }).unwrap();

            let payload: Vec<u8> = (0..128).collect();
            tx.put_frame(&payload).unwrap();

            let mut buf = vec![0u8; 1024];
            let written = tx.run_tx(&mut buf);
            buf.truncate(written);

            let (_, err) = rx.run_rx(&buf);
            assert!(err.is_none(), "CRC {:?} failed", crc_type);

            let frame = rx.get_rx_frame().unwrap();
            assert_eq!(frame, payload);
        }
    }

    /// Cross-validate: encode with old HdlcEncoder, decode with new HdlcLl
    #[test]
    fn test_interop_old_encoder_new_decoder() {
        use crate::proto::hdlc::low_level::HdlcEncoder;

        let encoder = HdlcEncoder::new(HdlcCrcT::HdlcCrc16, 0);
        let mut rx = create_ll(HdlcCrcT::HdlcCrc16);

        let payload = vec![0x7F, 0x7E, 0x7D, 0x00];
        let mut encoded = Vec::new();
        encoder.encode(&payload, &mut encoded);

        let (consumed, err) = rx.run_rx(&encoded);
        assert_eq!(consumed, encoded.len());
        assert!(err.is_none());

        let frame = rx.get_rx_frame().unwrap();
        assert_eq!(frame, payload);
    }

    /// Cross-validate: encode with new HdlcLl, decode with old HdlcEncoder
    #[test]
    fn test_interop_new_encoder_old_decoder() {
        use crate::proto::hdlc::low_level::HdlcEncoder;

        let mut tx = create_ll(HdlcCrcT::HdlcCrc16);
        let decoder = HdlcEncoder::new(HdlcCrcT::HdlcCrc16, 0);

        let payload = vec![0x7F, 0x7E, 0x7D, 0x00];
        tx.put_frame(&payload).unwrap();

        let mut buf = vec![0u8; 64];
        let written = tx.run_tx(&mut buf);
        buf.truncate(written);

        let mut decoded = Vec::new();
        let (_, result) = decoder.decode(&buf, &mut decoded);
        assert_eq!(result, crate::proto::hdlc::low_level::ResultT::Success);
        assert_eq!(decoded, payload);
    }
}
