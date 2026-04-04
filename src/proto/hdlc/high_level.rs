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

//! HDLC high-level protocol implementation.
//!
//! Adds event-based synchronization, timeout support, and blocking send
//! on top of the low-level HDLC framing layer.

use crate::proto::error::{TinyError, TinyResult};
use crate::proto::hdlc::low_level_ll::{HdlcLl, HdlcLlInit, ResetFlags};
use crate::proto::crc::HdlcCrcT;
use std::sync::{Mutex, Condvar, Arc};
use std::time::{Duration, Instant};

/// Callback invoked when a frame is received. Receives the frame payload.
type OnFrameReadCb = Box<dyn FnMut(&[u8]) + Send>;
/// Callback invoked when a frame has been fully sent. Receives the original payload.
type OnFrameSendCb = Box<dyn FnMut(&[u8]) + Send>;

/// Event bits for internal synchronization
const TX_ACCEPT_BIT: u8 = 0x01;
const TX_DATA_READY_BIT: u8 = 0x02;
const TX_DATA_SENT_BIT: u8 = 0x04;
const RX_DATA_READY_BIT: u8 = 0x08;

/// Simple event group (matching C HAL tiny_events_t)
struct Events {
    bits: Mutex<u8>,
    condvar: Condvar,
}

impl Events {
    fn new() -> Self {
        Events {
            bits: Mutex::new(0),
            condvar: Condvar::new(),
        }
    }

    fn set(&self, bits: u8) {
        let mut guard = self.bits.lock().unwrap();
        *guard |= bits;
        self.condvar.notify_all();
    }

    fn clear(&self, bits: u8) {
        let mut guard = self.bits.lock().unwrap();
        *guard &= !bits;
    }

    fn check_and_clear(&self, bits: u8) -> u8 {
        let mut guard = self.bits.lock().unwrap();
        let result = *guard & bits;
        *guard &= !bits;
        result
    }

    fn wait(&self, bits: u8, timeout_ms: u32) -> u8 {
        let mut guard = self.bits.lock().unwrap();
        if timeout_ms == 0xFFFFFFFF {
            while (*guard & bits) == 0 {
                guard = self.condvar.wait(guard).unwrap();
            }
            let result = *guard & bits;
            *guard &= !bits;
            result
        } else if timeout_ms == 0 {
            let result = *guard & bits;
            *guard &= !bits;
            result
        } else {
            let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
            while (*guard & bits) == 0 {
                let timeout = deadline.saturating_duration_since(Instant::now());
                if timeout.is_zero() {
                    break;
                }
                let (new_guard, _) = self.condvar.wait_timeout(guard, timeout).unwrap();
                guard = new_guard;
            }
            let result = *guard & bits;
            *guard &= !bits;
            result
        }
    }
}

/// Configuration for HDLC high-level protocol
pub struct HdlcConfig {
    /// CRC type
    pub crc_type: HdlcCrcT,
    /// RX buffer size
    pub rx_buf_size: usize,
    /// MTU (0 = auto)
    pub mtu: usize,
    /// Enable multithread mode (TX in separate thread)
    pub multithread_mode: bool,
}

impl Default for HdlcConfig {
    fn default() -> Self {
        HdlcConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            rx_buf_size: 256,
            mtu: 0,
            multithread_mode: false,
        }
    }
}

/// HDLC high-level protocol handle.
///
/// Wraps HdlcLl with event-based synchronization for blocking send operations
/// and callback notifications for received/sent frames.
pub struct Hdlc {
    ll: HdlcLl,
    events: Events,
    multithread_mode: bool,
    on_frame_read: Option<OnFrameReadCb>,
    on_frame_send: Option<OnFrameSendCb>,
    rx_len: usize,
}

impl Hdlc {
    /// Initialize a new HDLC high-level instance
    pub fn new(config: &HdlcConfig) -> TinyResult<Self> {
        let ll = HdlcLl::new(&HdlcLlInit {
            crc_type: config.crc_type,
            mtu: config.mtu,
            rx_buf_size: config.rx_buf_size,
        })?;

        let events = Events::new();
        events.set(TX_ACCEPT_BIT);

        Ok(Hdlc {
            ll,
            events,
            multithread_mode: config.multithread_mode,
            on_frame_read: None,
            on_frame_send: None,
            rx_len: 0,
        })
    }

    /// Set callback for received frames
    pub fn set_on_frame_read<F: FnMut(&[u8]) + Send + 'static>(&mut self, cb: F) {
        self.on_frame_read = Some(Box::new(cb));
    }

    /// Set callback for sent frames
    pub fn set_on_frame_send<F: FnMut(&[u8]) + Send + 'static>(&mut self, cb: F) {
        self.on_frame_send = Some(Box::new(cb));
    }

    /// Reset the protocol state
    pub fn reset(&mut self) {
        self.ll.reset(ResetFlags::Both);
        self.events.clear(0xFF);
        self.events.set(TX_ACCEPT_BIT);
    }

    /// Process incoming data, returning number of bytes consumed
    pub fn run_rx(&mut self, data: &[u8]) -> (usize, Option<TinyError>) {
        let (consumed, err) = self.ll.run_rx(data);

        // Deliver received frames via callback
        while let Some(frame) = self.ll.get_rx_frame() {
            self.rx_len = frame.len();
            if let Some(ref mut cb) = self.on_frame_read {
                cb(&frame);
            }
            self.events.set(RX_DATA_READY_BIT);
        }

        (consumed, err)
    }

    /// Run TX processing, writing data via the provided callback.
    ///
    /// The callback should send bytes to the hardware channel.
    /// Returns total number of bytes sent, or negative on error.
    pub fn run_tx(&mut self, send_fn: &mut dyn FnMut(&[u8]) -> i32) -> i32 {
        let mut total = 0i32;
        loop {
            let mut buf = [0u8; 1];
            let written = self.ll.run_tx(&mut buf);
            if written == 0 {
                break;
            }
            let mut remaining = written;
            while remaining > 0 {
                let sent = send_fn(&buf[..remaining]);
                if sent < 0 {
                    return if total > 0 { total } else { sent };
                }
                remaining -= sent as usize;
                total += sent;
            }
        }

        // Deliver sent frame notifications
        while let Some(frame) = self.ll.get_tx_sent_frame() {
            if let Some(ref mut cb) = self.on_frame_send {
                cb(&frame);
            }
            self.events.set(TX_DATA_SENT_BIT);
            self.events.set(TX_ACCEPT_BIT);
        }

        total
    }

    /// Fill output buffer with TX data, returning number of bytes written.
    ///
    /// Alternative to `run_tx()` — lets the caller manage the output buffer directly.
    pub fn get_tx_data(&mut self, out: &mut [u8]) -> usize {
        let written = self.ll.run_tx(out);

        // Deliver sent frame notifications
        while let Some(frame) = self.ll.get_tx_sent_frame() {
            if let Some(ref mut cb) = self.on_frame_send {
                cb(&frame);
            }
            self.events.set(TX_DATA_SENT_BIT);
            self.events.set(TX_ACCEPT_BIT);
        }

        written
    }

    /// Send a frame with optional timeout.
    ///
    /// If `timeout_ms` is 0, the frame is queued but not sent immediately.
    /// If `multithread_mode` is enabled, waits for another thread to complete TX.
    /// Otherwise, drives TX itself until sent or timeout.
    pub fn send(&mut self, data: &[u8], timeout_ms: u32, send_fn: Option<&mut dyn FnMut(&[u8]) -> i32>) -> TinyResult<()> {
        // Wait for TX accept
        if self.events.wait(TX_ACCEPT_BIT, timeout_ms) == 0 {
            return Err(TinyError::Busy);
        }

        // Queue the frame
        self.ll.put_frame(data)?;
        self.events.set(TX_DATA_READY_BIT);

        if timeout_ms == 0 {
            return Ok(());
        }

        if self.multithread_mode {
            // Wait for external TX thread to complete
            if self.events.wait(TX_DATA_SENT_BIT, timeout_ms) == 0 {
                // Timeout — cancel
                self.ll.reset(ResetFlags::TxOnly);
                self.events.set(TX_ACCEPT_BIT);
                return Err(TinyError::Timeout);
            }
            Ok(())
        } else if let Some(send_fn) = send_fn {
            // Drive TX ourselves
            let start = Instant::now();
            loop {
                let mut buf = [0u8; 1];
                let written = self.ll.run_tx(&mut buf);
                if written > 0 {
                    let mut remaining = written;
                    while remaining > 0 {
                        let sent = send_fn(&buf[..remaining]);
                        if sent < 0 {
                            self.ll.reset(ResetFlags::TxOnly);
                            self.events.set(TX_ACCEPT_BIT);
                            return Err(TinyError::Failed);
                        }
                        remaining -= sent as usize;
                    }
                }

                // Check if frame was sent
                if let Some(frame) = self.ll.get_tx_sent_frame() {
                    if let Some(ref mut cb) = self.on_frame_send {
                        cb(&frame);
                    }
                    self.events.set(TX_DATA_SENT_BIT);
                    self.events.set(TX_ACCEPT_BIT);
                    return Ok(());
                }

                // Check timeout
                if timeout_ms != 0xFFFFFFFF {
                    if start.elapsed() >= Duration::from_millis(timeout_ms as u64) {
                        self.ll.reset(ResetFlags::TxOnly);
                        self.events.set(TX_ACCEPT_BIT);
                        return Err(TinyError::Timeout);
                    }
                }
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hdlc_send_receive() {
        let mut tx_hdlc = Hdlc::new(&HdlcConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            rx_buf_size: 256,
            ..Default::default()
        }).unwrap();

        let mut rx_hdlc = Hdlc::new(&HdlcConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            rx_buf_size: 256,
            ..Default::default()
        }).unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        rx_hdlc.set_on_frame_read(move |data: &[u8]| {
            received_clone.lock().unwrap().push(data.to_vec());
        });

        let payload = vec![0x01, 0x02, 0x03, 0x04];

        // Queue frame
        tx_hdlc.send(&payload, 0, None).unwrap();

        // Get TX data
        let mut buf = vec![0u8; 64];
        let written = tx_hdlc.get_tx_data(&mut buf);

        // Feed to RX
        let (consumed, err) = rx_hdlc.run_rx(&buf[..written]);
        assert_eq!(consumed, written);
        assert!(err.is_none());

        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], payload);
    }

    #[test]
    fn test_hdlc_blocking_send() {
        let mut hdlc = Hdlc::new(&HdlcConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            rx_buf_size: 256,
            ..Default::default()
        }).unwrap();

        let payload = vec![0x01, 0x02, 0x03];
        let mut output = Vec::new();

        hdlc.send(&payload, 1000, Some(&mut |data: &[u8]| -> i32 {
            output.extend_from_slice(data);
            data.len() as i32
        })).unwrap();

        assert!(!output.is_empty());
        assert_eq!(output[0], 0x7E);
        assert_eq!(*output.last().unwrap(), 0x7E);
    }

    #[test]
    fn test_hdlc_reset() {
        let mut hdlc = Hdlc::new(&HdlcConfig::default()).unwrap();
        hdlc.send(&[1, 2, 3], 0, None).unwrap();
        hdlc.reset();
        // Should be able to send again after reset
        hdlc.send(&[4, 5, 6], 0, None).unwrap();
    }

    #[test]
    fn test_hdlc_multiple_frames() {
        let mut tx_hdlc = Hdlc::new(&HdlcConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            rx_buf_size: 256,
            ..Default::default()
        }).unwrap();

        let mut rx_hdlc = Hdlc::new(&HdlcConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            rx_buf_size: 256,
            ..Default::default()
        }).unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        rx_hdlc.set_on_frame_read(move |data: &[u8]| {
            received_clone.lock().unwrap().push(data.to_vec());
        });

        let payloads: Vec<Vec<u8>> = vec![
            vec![0x01, 0x02],
            vec![0x03, 0x04, 0x05],
            vec![0xAA, 0xBB, 0xCC, 0xDD],
        ];

        for payload in &payloads {
            tx_hdlc.send(payload, 0, None).unwrap();

            let mut buf = vec![0u8; 128];
            let written = tx_hdlc.get_tx_data(&mut buf);
            rx_hdlc.run_rx(&buf[..written]);
        }

        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 3);
        for (i, payload) in payloads.iter().enumerate() {
            assert_eq!(&frames[i], payload);
        }
    }
}
