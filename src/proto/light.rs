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

//! Tiny Light protocol implementation.
//!
//! Simple blocking send/read protocol built on top of HDLC low-level framing.
//! No acknowledgement, minimal overhead.

use crate::proto::error::{TinyError, TinyResult};
use crate::proto::hdlc::low_level_ll::{HdlcLl, HdlcLlInit, ResetFlags};
use crate::proto::crc::HdlcCrcT;
use std::time::{Duration, Instant};

/// Write callback: sends bytes to hardware channel
/// Returns number of bytes written, or negative on error
pub type WriteFn = dyn FnMut(&[u8]) -> i32;

/// Read callback: reads bytes from hardware channel
/// Returns number of bytes read, or negative on error
pub type ReadFn = dyn FnMut(&mut [u8]) -> i32;

/// Configuration for Light protocol
pub struct LightConfig {
    /// CRC type to use
    pub crc_type: HdlcCrcT,
    /// Timeout in milliseconds for send/read operations
    pub timeout_ms: u32,
}

impl Default for LightConfig {
    fn default() -> Self {
        LightConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            timeout_ms: 1000,
        }
    }
}

/// Tiny Light protocol handle.
///
/// Provides simple blocking send/read on top of HDLC framing.
/// No flow control, no acknowledgement.
pub struct Light {
    ll: HdlcLl,
    timeout_ms: u32,
}

impl Light {
    /// Create a new Light protocol instance
    pub fn new(config: &LightConfig) -> TinyResult<Self> {
        let ll = HdlcLl::new(&HdlcLlInit {
            crc_type: config.crc_type,
            mtu: 0,
            rx_buf_size: 256,
        })?;

        Ok(Light {
            ll,
            timeout_ms: config.timeout_ms,
        })
    }

    /// Send a frame in blocking mode.
    ///
    /// Returns number of bytes sent (payload size), or error.
    pub fn send(&mut self, data: &[u8], write_fn: &mut WriteFn) -> TinyResult<usize> {
        self.ll.put_frame(data)?;

        let start = Instant::now();
        loop {
            let mut buf = [0u8; 1];
            let written = self.ll.run_tx(&mut buf);
            if written == 0 {
                // Frame fully sent
                let _ = self.ll.get_tx_sent_frame();
                return Ok(data.len());
            }

            let mut remaining = written;
            while remaining > 0 {
                let sent = write_fn(&buf[..remaining]);
                if sent < 0 {
                    return Err(TinyError::Failed);
                }
                remaining -= sent as usize;

                if start.elapsed() >= Duration::from_millis(self.timeout_ms as u64) {
                    self.ll.reset(ResetFlags::TxOnly);
                    return Err(TinyError::Timeout);
                }
            }
        }
    }

    /// Read a frame in blocking mode.
    ///
    /// Returns received payload data, or error.
    pub fn read(&mut self, read_fn: &mut ReadFn) -> TinyResult<Vec<u8>> {
        let start = Instant::now();
        loop {
            let mut buf = [0u8; 1];
            let bytes_read = read_fn(&mut buf);
            if bytes_read < 0 {
                return Err(TinyError::Failed);
            }
            if bytes_read > 0 {
                let (_, err) = self.ll.run_rx(&buf[..bytes_read as usize]);
                if let Some(e) = err {
                    if e != TinyError::WrongCrc {
                        return Err(e);
                    }
                    // On wrong CRC, continue reading
                }
            }

            if let Some(frame) = self.ll.get_rx_frame() {
                return Ok(frame);
            }

            if start.elapsed() >= Duration::from_millis(self.timeout_ms as u64) {
                self.ll.reset(ResetFlags::RxOnly);
                return Err(TinyError::Timeout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    use std::cell::RefCell;
    use std::rc::Rc;

    struct FakeWire {
        data: Rc<RefCell<VecDeque<u8>>>,
    }

    impl FakeWire {
        fn new() -> Self {
            FakeWire { data: Rc::new(RefCell::new(VecDeque::new())) }
        }

        fn write_fn(&self) -> Box<dyn FnMut(&[u8]) -> i32> {
            let data = self.data.clone();
            Box::new(move |buf: &[u8]| -> i32 {
                let mut d = data.borrow_mut();
                for b in buf {
                    d.push_back(*b);
                }
                buf.len() as i32
            })
        }

        fn read_fn(&self) -> Box<dyn FnMut(&mut [u8]) -> i32> {
            let data = self.data.clone();
            Box::new(move |buf: &mut [u8]| -> i32 {
                let mut d = data.borrow_mut();
                if let Some(b) = d.pop_front() {
                    buf[0] = b;
                    1
                } else {
                    0
                }
            })
        }
    }

    #[test]
    fn test_light_send_receive() {
        let config = LightConfig::default();
        let mut sender = Light::new(&config).unwrap();
        let mut receiver = Light::new(&config).unwrap();

        let wire = FakeWire::new();

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let sent = sender.send(&payload, &mut wire.write_fn()).unwrap();
        assert_eq!(sent, payload.len());

        let received = receiver.read(&mut wire.read_fn()).unwrap();
        assert_eq!(received, payload);
    }

    #[test]
    fn test_light_multiple_frames() {
        let config = LightConfig::default();
        let mut sender = Light::new(&config).unwrap();
        let mut receiver = Light::new(&config).unwrap();

        let wire = FakeWire::new();

        for i in 0..5 {
            let payload: Vec<u8> = vec![i, i + 1, i + 2];
            sender.send(&payload, &mut wire.write_fn()).unwrap();
            let received = receiver.read(&mut wire.read_fn()).unwrap();
            assert_eq!(received, payload);
        }
    }

    #[test]
    fn test_light_special_bytes() {
        let config = LightConfig::default();
        let mut sender = Light::new(&config).unwrap();
        let mut receiver = Light::new(&config).unwrap();

        let wire = FakeWire::new();

        let payload = vec![0x7E, 0x7D, 0xFF, 0x00, 0x7E];
        sender.send(&payload, &mut wire.write_fn()).unwrap();
        let received = receiver.read(&mut wire.read_fn()).unwrap();
        assert_eq!(received, payload);
    }

    #[test]
    fn test_light_timeout_on_empty() {
        let config = LightConfig {
            timeout_ms: 50,
            ..LightConfig::default()
        };
        let mut receiver = Light::new(&config).unwrap();

        let wire = FakeWire::new();
        // No data written to the wire — read should timeout
        let result = receiver.read(&mut wire.read_fn());
        assert_eq!(result, Err(TinyError::Timeout));
    }
}
