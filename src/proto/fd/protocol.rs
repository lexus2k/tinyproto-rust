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

//! Full-Duplex protocol core implementation.
//!
//! Implements the main TinyFd state machine with connection management,
//! I/S/U frame handling, sliding window, and timeout-based retransmission.

use crate::proto::crc::HdlcCrcT;
use crate::proto::error::{TinyError, TinyResult};
use crate::proto::hdlc::low_level_ll::{HdlcLl, HdlcLlInit};
use super::defines::*;
use super::frames::{FrameHeader, QueuedFrameType};
use super::frame_queue::FramesQueue;
use super::peers::PeerManager;
use std::sync::{Mutex, Condvar};
use std::time::{Duration, Instant};

/// Callback invoked when a data frame is received.
/// Arguments: `(peer_address, payload)`.
pub type OnFrameReadCb = Box<dyn FnMut(u8, &[u8]) + Send>;
/// Callback invoked when a data frame has been sent.
/// Arguments: `(peer_address, payload)`.
pub type OnFrameSendCb = Box<dyn FnMut(u8, &[u8]) + Send>;
/// Callback invoked on connect/disconnect events.
/// Arguments: `(peer_address, connected)` where `connected` is `true` on connect.
pub type OnConnectEventCb = Box<dyn FnMut(u8, bool) + Send>;

/// Configuration for the Full-Duplex protocol.
pub struct TinyFdConfig {
    /// CRC type for frame integrity checking.
    pub crc_type: HdlcCrcT,
    /// TX sliding-window size (1–7 frames).
    pub window_frames: u8,
    /// Maximum payload size per frame (0 = auto-detect).
    pub mtu: usize,
    /// Timeout in milliseconds for blocking send operations.
    pub send_timeout: u16,
    /// Timeout in milliseconds before retransmitting unacknowledged frames.
    pub retry_timeout: u16,
    /// Maximum number of retransmission attempts before disconnecting.
    pub retries: u8,
    /// Local station address (0 = primary station).
    pub addr: u8,
    /// Number of remote peers (only meaningful for primary stations).
    pub peers_count: u8,
    /// Protocol mode (ABM or NRM).
    pub mode: FdMode,
    /// Size of the internal RX buffer in bytes.
    pub rx_buf_size: usize,
}

impl Default for TinyFdConfig {
    fn default() -> Self {
        TinyFdConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            window_frames: 7,
            mtu: 64,
            send_timeout: 1000,
            retry_timeout: 200,
            retries: 2,
            addr: 0,
            peers_count: 1,
            mode: FdMode::Abm,
            rx_buf_size: 512,
        }
    }
}

/// Simple event group
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

    fn wait(&self, bits: u8, timeout_ms: u32) -> u8 {
        let mut guard = self.bits.lock().unwrap();
        if timeout_ms == 0 {
            let result = *guard & bits;
            return result;
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        while (*guard & bits) == 0 {
            let timeout = deadline.saturating_duration_since(Instant::now());
            if timeout.is_zero() {
                break;
            }
            let (new_guard, _) = self.condvar.wait_timeout(guard, timeout).unwrap();
            guard = new_guard;
        }
        *guard & bits
    }
}

/// Full-Duplex protocol handle.
///
/// Implements HDLC ABM/NRM with sliding window, I/S/U frames,
/// connection management, and timeout-based retransmission.
pub struct TinyFd {
    hdlc: HdlcLl,
    peer_mgr: PeerManager,
    frames: FramesQueue,
    events: Events,
    mode: FdMode,
    send_timeout: u16,
    retry_timeout: u16,
    retries: u8,
    on_read_cb: Option<OnFrameReadCb>,
    on_send_cb: Option<OnFrameSendCb>,
    on_connect_event_cb: Option<OnConnectEventCb>,
    mtu: usize,
}

impl TinyFd {
    /// Initialize the Full-Duplex protocol
    pub fn new(config: &TinyFdConfig) -> TinyResult<Self> {
        if config.window_frames == 0 || config.window_frames > 7 {
            return Err(TinyError::InvalidData);
        }

        let local_addr = if config.addr == 0 {
            HDLC_PRIMARY_ADDR | HDLC_E_BIT
        } else {
            (config.addr << 2) | HDLC_E_BIT
        };

        let mtu = if config.mtu > 0 { config.mtu } else { 64 };

        let hdlc = HdlcLl::new(&HdlcLlInit {
            crc_type: config.crc_type,
            mtu: mtu + 2, // +2 for frame header
            rx_buf_size: config.rx_buf_size,
        })?;

        let frames = FramesQueue::new(
            config.window_frames as usize,
            U_QUEUE_MAX_SIZE,
            mtu,
        );

        let peer_mgr = PeerManager::new(local_addr, config.peers_count);

        let send_timeout = if config.send_timeout == 0 { 1000 } else { config.send_timeout };
        let retry_timeout = if config.retry_timeout == 0 { 200 } else { config.retry_timeout };

        let events = Events::new();
        events.set(FD_EVENT_QUEUE_HAS_FREE_SLOTS);

        Ok(TinyFd {
            hdlc,
            peer_mgr,
            frames,
            events,
            mode: config.mode,
            send_timeout,
            retry_timeout,
            retries: config.retries,
            on_read_cb: None,
            on_send_cb: None,
            on_connect_event_cb: None,
            mtu,
        })
    }

    /// Set callback for received data frames
    pub fn set_on_read<F: FnMut(u8, &[u8]) + Send + 'static>(&mut self, cb: F) {
        self.on_read_cb = Some(Box::new(cb));
    }

    /// Set callback for sent data frames
    pub fn set_on_send<F: FnMut(u8, &[u8]) + Send + 'static>(&mut self, cb: F) {
        self.on_send_cb = Some(Box::new(cb));
    }

    /// Set callback for connect/disconnect events
    pub fn set_on_connect_event<F: FnMut(u8, bool) + Send + 'static>(&mut self, cb: F) {
        self.on_connect_event_cb = Some(Box::new(cb));
    }

    /// Get connection status
    pub fn get_status(&self) -> TinyResult<()> {
        if self.peer_mgr.peers.is_empty() {
            return Err(TinyError::InvalidData);
        }
        match self.peer_mgr.peers[0].state {
            FdState::Connected => Ok(()),
            _ => Err(TinyError::Failed),
        }
    }

    /// Get MTU
    pub fn get_mtu(&self) -> usize {
        self.mtu
    }

    /// Send a disconnect command
    pub fn disconnect(&mut self) -> TinyResult<()> {
        let peer = 0u8;
        let address = self.peer_mgr.peer_to_address_field(peer);
        let header = FrameHeader {
            address,
            control: HDLC_U_FRAME_TYPE_DISC | HDLC_U_FRAME_BITS | HDLC_P_BIT,
        };
        self.put_u_s_frame(QueuedFrameType::UFrame, header, &[])?;
        if let Some(peer_info) = self.peer_mgr.peers.get_mut(peer as usize) {
            peer_info.state = FdState::Disconnecting;
        }
        Ok(())
    }

    /// Send a data packet to the specified address.
    ///
    /// For primary stations communicating with secondary, use the peer address.
    /// For secondary stations, use `FD_PRIMARY_ADDR`.
    pub fn send_packet(&mut self, address: u8, data: &[u8], timeout_ms: u32) -> TinyResult<()> {
        if data.len() > self.mtu {
            return Err(TinyError::DataTooLarge);
        }

        // Resolve peer index:
        // - address 0 (FD_PRIMARY_ADDR): default peer (peer 0)
        // - specific address: lookup by HDLC address field
        let peer = if address == FD_PRIMARY_ADDR {
            if self.peer_mgr.peers.is_empty() {
                return Err(TinyError::UnknownPeer);
            }
            0u8
        } else {
            let hdlc_addr = (address << 2) | HDLC_E_BIT;
            self.peer_mgr.address_field_to_peer(hdlc_addr)
        };
        if peer == HDLC_INVALID_PEER_INDEX {
            return Err(TinyError::UnknownPeer);
        }

        // Wait for free slot
        let bits = self.events.wait(FD_EVENT_QUEUE_HAS_FREE_SLOTS, timeout_ms);
        if bits == 0 {
            return Err(TinyError::Timeout);
        }

        let addr_field = self.peer_mgr.peer_to_address_field(peer);
        self.put_i_frame(peer, addr_field, data)?;

        self.events.set(FD_EVENT_TX_DATA_AVAILABLE);
        if !self.frames.i_queue.has_free_slots() {
            self.events.clear(FD_EVENT_QUEUE_HAS_FREE_SLOTS);
        }

        Ok(())
    }

    /// Process incoming data from the channel
    pub fn on_rx_data(&mut self, data: &[u8]) -> TinyResult<()> {
        let mut remaining = data;
        while !remaining.is_empty() {
            let (consumed, err) = self.hdlc.run_rx(remaining);
            if consumed == 0 && err.is_none() {
                break;
            }
            remaining = &remaining[consumed..];

            // Process received frames
            while let Some(frame) = self.hdlc.get_rx_frame() {
                self.on_frame_received(&frame);
            }

            if let Some(e) = err {
                if e == TinyError::WrongCrc {
                    continue; // Skip bad CRC, keep reading
                }
            }
        }
        Ok(())
    }

    /// Get TX data to send to the channel.
    /// First generates any pending protocol frames, then fills the output buffer.
    pub fn get_tx_data(&mut self, out: &mut [u8], timeout_ms: u32) -> usize {
        // Check if we need to send connection setup
        self.check_timeouts();
        self.generate_tx_frames();

        let written = self.hdlc.run_tx(out);

        // Track sent frame notifications
        while let Some(_frame) = self.hdlc.get_tx_sent_frame() {
            // Frame was sent to wire
        }

        if written == 0 && timeout_ms > 0 {
            let bits = self.events.wait(FD_EVENT_TX_DATA_AVAILABLE, timeout_ms);
            if bits != 0 {
                self.generate_tx_frames();
                return self.hdlc.run_tx(out);
            }
        }

        written
    }

    // --- Internal methods ---

    fn put_u_s_frame(&mut self, frame_type: QueuedFrameType, header: FrameHeader, data: &[u8]) -> TinyResult<()> {
        if self.frames.s_queue.allocate(frame_type, header, data).is_some() {
            self.events.set(FD_EVENT_TX_DATA_AVAILABLE);
            Ok(())
        } else {
            Err(TinyError::Busy)
        }
    }

    fn put_i_frame(&mut self, peer: u8, addr_field: u8, data: &[u8]) -> TinyResult<()> {
        let peer_info = &mut self.peer_mgr.peers[peer as usize];
        if peer_info.i_queue_control.tx_full() {
            return Err(TinyError::Busy);
        }

        let ns = peer_info.i_queue_control.get_last_ns();
        let nr = peer_info.i_queue_control.get_next_frame_to_receive();
        let control = (ns << 5) | (nr << 1) | HDLC_I_FRAME_BITS;

        let header = FrameHeader {
            address: addr_field,
            control,
        };

        if self.frames.i_queue.allocate(QueuedFrameType::IFrame, header, data).is_some() {
            peer_info.i_queue_control.move_to_next_ns();
            Ok(())
        } else {
            Err(TinyError::Busy)
        }
    }

    fn on_frame_received(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }

        let address = data[0];
        let control = data[1];
        let peer = self.peer_mgr.address_field_to_peer(address);
        if peer == HDLC_INVALID_PEER_INDEX {
            return;
        }

        self.peer_mgr.peers[peer as usize].last_received_frame_ts = Instant::now();
        self.peer_mgr.peers[peer as usize].ka_confirmed = true;

        if (control & HDLC_U_FRAME_MASK) == HDLC_U_FRAME_BITS {
            self.on_u_frame_received(peer, data);
        } else {
            let state = self.peer_mgr.peers[peer as usize].state;
            if state != FdState::Connected && state != FdState::Disconnecting {
                let addr = self.peer_mgr.peer_to_address_field(peer);
                let header = FrameHeader {
                    address: addr,
                    control: HDLC_U_FRAME_TYPE_DM | HDLC_U_FRAME_BITS,
                };
                let _ = self.put_u_s_frame(QueuedFrameType::UFrame, header, &[]);
            } else if (control & HDLC_I_FRAME_MASK) == HDLC_I_FRAME_BITS {
                self.on_i_frame_received(peer, data);
            } else if (control & HDLC_S_FRAME_MASK) == HDLC_S_FRAME_BITS {
                self.on_s_frame_received(peer, data);
            }
        }
    }

    fn on_u_frame_received(&mut self, peer: u8, data: &[u8]) {
        let control = data[1];
        let u_type = control & HDLC_U_FRAME_TYPE_MASK;

        match u_type {
            HDLC_U_FRAME_TYPE_SABM => {
                // Connection request — respond with UA
                let addr = self.peer_mgr.peer_to_address_field(peer);
                let header = FrameHeader {
                    address: addr,
                    control: HDLC_U_FRAME_TYPE_UA | HDLC_U_FRAME_BITS | HDLC_F_BIT,
                };
                let _ = self.put_u_s_frame(QueuedFrameType::UFrame, header, &[]);
                self.switch_to_connected(peer);
            }
            HDLC_U_FRAME_TYPE_UA => {
                let state = self.peer_mgr.peers[peer as usize].state;
                match state {
                    FdState::Connecting => {
                        self.switch_to_connected(peer);
                    }
                    FdState::Disconnecting => {
                        self.switch_to_disconnected(peer);
                    }
                    _ => {}
                }
            }
            HDLC_U_FRAME_TYPE_DISC => {
                // Disconnect request — respond with UA
                let addr = self.peer_mgr.peer_to_address_field(peer);
                let header = FrameHeader {
                    address: addr,
                    control: HDLC_U_FRAME_TYPE_UA | HDLC_U_FRAME_BITS | HDLC_F_BIT,
                };
                let _ = self.put_u_s_frame(QueuedFrameType::UFrame, header, &[]);
                self.switch_to_disconnected(peer);
            }
            HDLC_U_FRAME_TYPE_DM => {
                self.switch_to_disconnected(peer);
            }
            HDLC_U_FRAME_TYPE_SNRM => {
                // NRM connection request
                let addr = self.peer_mgr.peer_to_address_field(peer);
                let header = FrameHeader {
                    address: addr,
                    control: HDLC_U_FRAME_TYPE_UA | HDLC_U_FRAME_BITS | HDLC_F_BIT,
                };
                let _ = self.put_u_s_frame(QueuedFrameType::UFrame, header, &[]);
                self.switch_to_connected(peer);
            }
            _ => {}
        }
    }

    fn on_i_frame_received(&mut self, peer: u8, data: &[u8]) {
        let control = data[1];
        let ns = (control >> 5) & SEQ_BITS_MASK;
        let nr = (control >> 1) & SEQ_BITS_MASK;
        let payload = &data[2..];

        // Pre-compute values we need from peer_mgr before mutating
        let is_primary = self.peer_mgr.is_primary_station();
        let addr = self.peer_mgr.peer_to_address_field(peer);
        let user_address = if is_primary { addr >> 2 } else { FD_PRIMARY_ADDR };

        // Confirm our sent frames and collect confirmed seq numbers
        let mut confirmed_seqs = Vec::new();
        self.peer_mgr.peers[peer as usize].i_queue_control.confirm_sent_frames(nr, |seq| {
            confirmed_seqs.push(seq);
            true
        });
        // Free confirmed frames from i_queue
        for seq in confirmed_seqs {
            self.frames.i_queue.free_by_ns(addr, seq);
        }
        if self.frames.i_queue.has_free_slots() {
            self.events.set(FD_EVENT_QUEUE_HAS_FREE_SLOTS);
        }

        // Re-borrow after events call
        let peer_info = &mut self.peer_mgr.peers[peer as usize];

        // Check N(S) — expected sequence
        let expected_nr = peer_info.i_queue_control.get_next_frame_to_receive();
        if ns != expected_nr {
            // Out of sequence — send REJ
            if !peer_info.sent_reject {
                peer_info.sent_reject = true;
                let header = FrameHeader {
                    address: addr,
                    control: HDLC_S_FRAME_BITS | HDLC_S_FRAME_TYPE_REJ | (expected_nr << 5) | HDLC_P_BIT,
                };
                let _ = self.put_u_s_frame(QueuedFrameType::SFrame, header, &[]);
            }
            return;
        }

        // Accept frame
        peer_info.i_queue_control.move_to_next_frame_to_receive();
        peer_info.sent_reject = false;
        let new_nr = peer_info.i_queue_control.get_next_frame_to_receive();
        peer_info.sent_nr = new_nr;

        // Deliver to user callback
        if let Some(ref mut cb) = self.on_read_cb {
            cb(user_address, payload);
        }

        // Send RR to acknowledge
        let header = FrameHeader {
            address: addr,
            control: HDLC_S_FRAME_BITS | HDLC_S_FRAME_TYPE_RR | (new_nr << 5),
        };
        let _ = self.put_u_s_frame(QueuedFrameType::SFrame, header, &[]);
    }

    fn on_s_frame_received(&mut self, peer: u8, data: &[u8]) {
        let control = data[1];
        let nr = (control >> 5) & SEQ_BITS_MASK;
        let s_type = control & HDLC_S_FRAME_TYPE_MASK;

        let addr = self.peer_mgr.peer_to_address_field(peer);

        // Confirm frames and free from queue
        let mut confirmed_seqs = Vec::new();
        self.peer_mgr.peers[peer as usize].i_queue_control.confirm_sent_frames(nr, |seq| {
            confirmed_seqs.push(seq);
            true
        });
        for seq in confirmed_seqs {
            self.frames.i_queue.free_by_ns(addr, seq);
        }
        if self.frames.i_queue.has_free_slots() {
            self.events.set(FD_EVENT_QUEUE_HAS_FREE_SLOTS);
        }

        match s_type {
            HDLC_S_FRAME_TYPE_RR => {
                self.peer_mgr.peers[peer as usize].retries = self.retries;
            }
            HDLC_S_FRAME_TYPE_REJ => {
                self.peer_mgr.peers[peer as usize].i_queue_control.retransmit_frame(nr, |_| true);
                self.events.set(FD_EVENT_TX_DATA_AVAILABLE);
            }
            HDLC_S_FRAME_TYPE_SREJ => {
                self.peer_mgr.peers[peer as usize].i_queue_control.retransmit_frame(nr, |_| true);
                self.events.set(FD_EVENT_TX_DATA_AVAILABLE);
            }
            _ => {}
        }
    }

    fn switch_to_connected(&mut self, peer: u8) {
        let is_primary = self.peer_mgr.is_primary_station();
        let addr_field = self.peer_mgr.peer_to_address_field(peer);
        let user_address = if is_primary { addr_field >> 2 } else { FD_PRIMARY_ADDR };

        let state = self.peer_mgr.peers[peer as usize].state;
        if state != FdState::Connected {
            self.peer_mgr.peers[peer as usize].state = FdState::Connected;
            self.peer_mgr.peers[peer as usize].reset_connection();
            self.peer_mgr.peers[peer as usize].retries = self.retries;
            self.peer_mgr.peers[peer as usize].last_sent_i_ts = Instant::now();
            self.peer_mgr.peers[peer as usize].events |= FD_EVENT_CAN_ACCEPT_I_FRAMES;
            self.events.set(FD_EVENT_TX_DATA_AVAILABLE);
            if self.frames.i_queue.has_free_slots() {
                self.events.set(FD_EVENT_QUEUE_HAS_FREE_SLOTS);
            }

            if let Some(ref mut cb) = self.on_connect_event_cb {
                cb(user_address, true);
            }
        }
    }

    fn switch_to_disconnected(&mut self, peer: u8) {
        let is_primary = self.peer_mgr.is_primary_station();
        let addr_field = self.peer_mgr.peer_to_address_field(peer);
        let user_address = if is_primary { addr_field >> 2 } else { FD_PRIMARY_ADDR };

        let state = self.peer_mgr.peers[peer as usize].state;
        if state != FdState::Disconnected {
            self.peer_mgr.peers[peer as usize].state = FdState::Disconnected;
            self.peer_mgr.peers[peer as usize].reset_connection();
            self.peer_mgr.peers[peer as usize].events &= !FD_EVENT_CAN_ACCEPT_I_FRAMES;
            self.frames.i_queue.reset_for(addr_field);

            if let Some(ref mut cb) = self.on_connect_event_cb {
                cb(user_address, false);
            }
        }
    }

    fn check_timeouts(&mut self) {
        for peer_idx in 0..self.peer_mgr.peers.len() {
            // Copy out immutable state we need to inspect
            let state = self.peer_mgr.peers[peer_idx].state;
            let last_sent_frame_ts = self.peer_mgr.peers[peer_idx].last_sent_frame_ts;
            let last_sent_i_ts = self.peer_mgr.peers[peer_idx].last_sent_i_ts;
            let retries = self.peer_mgr.peers[peer_idx].retries;
            let has_unconfirmed = self.peer_mgr.peers[peer_idx].i_queue_control.has_unconfirmed_frames();
            let retry_dur = Duration::from_millis(self.retry_timeout as u64);

            match state {
                FdState::Disconnected | FdState::Idle => {
                    if self.mode == FdMode::Abm {
                        if last_sent_frame_ts.elapsed() >= retry_dur {
                            let addr = self.peer_mgr.peer_to_address_field(peer_idx as u8);
                            let header = FrameHeader {
                                address: addr,
                                control: HDLC_U_FRAME_TYPE_SABM | HDLC_U_FRAME_BITS | HDLC_P_BIT,
                            };
                            let _ = self.put_u_s_frame(QueuedFrameType::UFrame, header, &[]);
                            self.peer_mgr.peers[peer_idx].state = FdState::Connecting;
                            self.peer_mgr.peers[peer_idx].last_sent_frame_ts = Instant::now();
                        }
                    }
                }
                FdState::Connecting => {
                    if last_sent_frame_ts.elapsed() >= retry_dur {
                        if retries == 0 {
                            self.peer_mgr.peers[peer_idx].state = FdState::Disconnected;
                        } else {
                            self.peer_mgr.peers[peer_idx].retries -= 1;
                            let addr = self.peer_mgr.peer_to_address_field(peer_idx as u8);
                            let header = FrameHeader {
                                address: addr,
                                control: HDLC_U_FRAME_TYPE_SABM | HDLC_U_FRAME_BITS | HDLC_P_BIT,
                            };
                            let _ = self.put_u_s_frame(QueuedFrameType::UFrame, header, &[]);
                            self.peer_mgr.peers[peer_idx].last_sent_frame_ts = Instant::now();
                        }
                    }
                }
                FdState::Connected => {
                    if has_unconfirmed && last_sent_i_ts.elapsed() >= retry_dur {
                        if retries == 0 {
                            self.switch_to_disconnected(peer_idx as u8);
                        } else {
                            self.peer_mgr.peers[peer_idx].retries -= 1;
                            let confirm_ns = self.peer_mgr.peers[peer_idx].i_queue_control.get_next_frame_to_confirm();
                            self.peer_mgr.peers[peer_idx].i_queue_control.retransmit_frame(confirm_ns, |_| true);
                            self.peer_mgr.peers[peer_idx].last_sent_i_ts = Instant::now();
                            self.events.set(FD_EVENT_TX_DATA_AVAILABLE);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn generate_tx_frames(&mut self) {
        if self.hdlc.is_tx_busy() {
            return;
        }

        // Priority 1: S/U frames
        for i in 0..self.frames.s_queue.capacity() {
            if let Some(frame) = self.frames.s_queue.get(i) {
                let data = frame.to_bytes();
                if self.hdlc.put_frame(&data).is_ok() {
                    self.frames.s_queue.free(i);
                    return;
                }
            }
        }

        // Priority 2: I-frames
        for peer_idx in 0..self.peer_mgr.peers.len() {
            let state = self.peer_mgr.peers[peer_idx].state;
            if state != FdState::Connected {
                continue;
            }

            let all_sent = self.peer_mgr.peers[peer_idx].i_queue_control.all_frames_are_sent();
            if all_sent {
                continue;
            }

            let ns = self.peer_mgr.peers[peer_idx].i_queue_control.get_next_frame_to_send();
            let nr = self.peer_mgr.peers[peer_idx].i_queue_control.get_next_frame_to_receive();
            let addr = self.peer_mgr.peer_to_address_field(peer_idx as u8);

            if let Some((_idx, frame)) = self.frames.i_queue.get_next(QueuedFrameType::IFrame, addr, ns) {
                let mut data = frame.to_bytes();
                data[1] = (ns << 5) | (nr << 1) | HDLC_I_FRAME_BITS;

                if self.hdlc.put_frame(&data).is_ok() {
                    self.peer_mgr.peers[peer_idx].i_queue_control.advance_next_ns();
                    self.peer_mgr.peers[peer_idx].last_sent_i_ts = Instant::now();
                    self.peer_mgr.peers[peer_idx].last_sent_frame_ts = Instant::now();
                    self.peer_mgr.peers[peer_idx].retries = self.retries;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn default_config() -> TinyFdConfig {
        TinyFdConfig {
            crc_type: HdlcCrcT::HdlcCrc16,
            window_frames: 4,
            mtu: 64,
            send_timeout: 1000,
            retry_timeout: 100,
            retries: 2,
            addr: 0,
            peers_count: 1,
            mode: FdMode::Abm,
            rx_buf_size: 512,
        }
    }

    #[test]
    fn test_fd_init() {
        let fd = TinyFd::new(&default_config()).unwrap();
        assert_eq!(fd.get_mtu(), 64);
        assert!(fd.get_status().is_err()); // Not connected
    }

    #[test]
    fn test_fd_connect_abm() {
        // Simulate two endpoints connecting via SABM/UA exchange
        let mut primary = TinyFd::new(&default_config()).unwrap();
        let mut secondary = TinyFd::new(&TinyFdConfig {
            addr: 1,
            ..default_config()
        }).unwrap();

        let connected_primary = Arc::new(Mutex::new(false));
        let connected_secondary = Arc::new(Mutex::new(false));

        let cp = connected_primary.clone();
        primary.set_on_connect_event(move |_addr, connected| {
            *cp.lock().unwrap() = connected;
        });
        let cs = connected_secondary.clone();
        secondary.set_on_connect_event(move |_addr, connected| {
            *cs.lock().unwrap() = connected;
        });

        // Primary sends SABM (via timeout check)
        primary.check_timeouts();

        // Get primary's TX data (SABM frame)
        let mut buf = vec![0u8; 256];
        let written = primary.get_tx_data(&mut buf, 0);
        assert!(written > 0, "Primary should have SABM to send");
        let sabm_data = buf[..written].to_vec();

        // Feed to secondary
        secondary.on_rx_data(&sabm_data).unwrap();

        // Secondary should respond with UA
        let written = secondary.get_tx_data(&mut buf, 0);
        assert!(written > 0, "Secondary should have UA to send");
        let ua_data = buf[..written].to_vec();

        // Secondary should be connected now
        assert!(*connected_secondary.lock().unwrap());

        // Feed UA to primary
        primary.on_rx_data(&ua_data).unwrap();

        // Primary should be connected now
        assert!(*connected_primary.lock().unwrap());
        assert!(primary.get_status().is_ok());
    }

    #[test]
    fn test_fd_send_receive() {
        // Set up connected pair
        let mut primary = TinyFd::new(&default_config()).unwrap();
        let mut secondary = TinyFd::new(&TinyFdConfig {
            addr: 1,
            ..default_config()
        }).unwrap();

        let received_data = Arc::new(Mutex::new(Vec::new()));
        let rd = received_data.clone();
        secondary.set_on_read(move |_addr, data: &[u8]| {
            rd.lock().unwrap().push(data.to_vec());
        });

        // Establish connection
        let mut buf = vec![0u8; 256];
        primary.check_timeouts();
        let w = primary.get_tx_data(&mut buf, 0);
        secondary.on_rx_data(&buf[..w]).unwrap();
        let w = secondary.get_tx_data(&mut buf, 0);
        primary.on_rx_data(&buf[..w]).unwrap();

        // Send I-frame from primary
        let payload = vec![0xAA, 0xBB, 0xCC];
        primary.send_packet(FD_PRIMARY_ADDR, &payload, 0).unwrap();

        let w = primary.get_tx_data(&mut buf, 0);
        assert!(w > 0);
        secondary.on_rx_data(&buf[..w]).unwrap();

        let frames = received_data.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], payload);
    }

    #[test]
    fn test_fd_invalid_window() {
        let result = TinyFd::new(&TinyFdConfig {
            window_frames: 0,
            ..default_config()
        });
        assert!(result.is_err());

        let result = TinyFd::new(&TinyFdConfig {
            window_frames: 8,
            ..default_config()
        });
        assert!(result.is_err());
    }

    fn connect_pair() -> (TinyFd, TinyFd) {
        let mut primary = TinyFd::new(&default_config()).unwrap();
        let mut secondary = TinyFd::new(&TinyFdConfig {
            addr: 1,
            ..default_config()
        }).unwrap();
        let mut buf = vec![0u8; 256];
        primary.check_timeouts();
        let w = primary.get_tx_data(&mut buf, 0);
        secondary.on_rx_data(&buf[..w]).unwrap();
        let w = secondary.get_tx_data(&mut buf, 0);
        primary.on_rx_data(&buf[..w]).unwrap();
        (primary, secondary)
    }

    /// Exchange all pending TX data between two peers in both directions.
    fn exchange(a: &mut TinyFd, b: &mut TinyFd) {
        let mut buf = vec![0u8; 512];
        loop {
            let w1 = a.get_tx_data(&mut buf, 0);
            if w1 > 0 {
                b.on_rx_data(&buf[..w1]).unwrap();
            }
            let w2 = b.get_tx_data(&mut buf, 0);
            if w2 > 0 {
                a.on_rx_data(&buf[..w2]).unwrap();
            }
            if w1 == 0 && w2 == 0 {
                break;
            }
        }
    }

    #[test]
    fn test_fd_disconnect() {
        let (mut primary, mut secondary) = connect_pair();
        assert!(primary.get_status().is_ok());
        assert!(secondary.get_status().is_ok());

        let disconnected_primary = Arc::new(Mutex::new(false));
        let disconnected_secondary = Arc::new(Mutex::new(false));

        let dp = disconnected_primary.clone();
        primary.set_on_connect_event(move |_addr, connected| {
            if !connected { *dp.lock().unwrap() = true; }
        });
        let ds = disconnected_secondary.clone();
        secondary.set_on_connect_event(move |_addr, connected| {
            if !connected { *ds.lock().unwrap() = true; }
        });

        primary.disconnect().unwrap();
        exchange(&mut primary, &mut secondary);

        assert!(*disconnected_secondary.lock().unwrap());
        assert!(*disconnected_primary.lock().unwrap());
        assert!(primary.get_status().is_err());
        assert!(secondary.get_status().is_err());
    }

    #[test]
    fn test_fd_bidirectional_data() {
        let (mut primary, mut secondary) = connect_pair();

        let primary_received = Arc::new(Mutex::new(Vec::new()));
        let secondary_received = Arc::new(Mutex::new(Vec::new()));

        let pr = primary_received.clone();
        primary.set_on_read(move |_addr, data: &[u8]| {
            pr.lock().unwrap().push(data.to_vec());
        });
        let sr = secondary_received.clone();
        secondary.set_on_read(move |_addr, data: &[u8]| {
            sr.lock().unwrap().push(data.to_vec());
        });

        let payload_to_secondary = vec![0x11, 0x22, 0x33];
        let payload_to_primary = vec![0xAA, 0xBB, 0xCC];

        primary.send_packet(FD_PRIMARY_ADDR, &payload_to_secondary, 0).unwrap();
        secondary.send_packet(FD_PRIMARY_ADDR, &payload_to_primary, 0).unwrap();

        exchange(&mut primary, &mut secondary);

        let sr_frames = secondary_received.lock().unwrap();
        assert_eq!(sr_frames.len(), 1);
        assert_eq!(sr_frames[0], payload_to_secondary);

        let pr_frames = primary_received.lock().unwrap();
        assert_eq!(pr_frames.len(), 1);
        assert_eq!(pr_frames[0], payload_to_primary);
    }

    #[test]
    fn test_fd_multiple_frames() {
        let (mut primary, mut secondary) = connect_pair();

        let received = Arc::new(Mutex::new(Vec::new()));
        let rd = received.clone();
        secondary.set_on_read(move |_addr, data: &[u8]| {
            rd.lock().unwrap().push(data.to_vec());
        });

        // Queue 3 frames then exchange — tests the sliding window path
        let payload1 = vec![0x10, 0x20, 0x30];
        let payload2 = vec![0x40, 0x50, 0x60];
        let payload3 = vec![0x70, 0x80, 0x90];

        primary.send_packet(FD_PRIMARY_ADDR, &payload1, 0).unwrap();
        primary.send_packet(FD_PRIMARY_ADDR, &payload2, 0).unwrap();
        primary.send_packet(FD_PRIMARY_ADDR, &payload3, 0).unwrap();

        exchange(&mut primary, &mut secondary);

        let frames = received.lock().unwrap();
        // All 3 I-frames should be delivered to the peer
        assert_eq!(frames.len(), 3);
        // First frame payload must be correct
        assert_eq!(frames[0], payload1);
    }

    #[test]
    fn test_fd_send_data_too_large() {
        let (mut primary, _secondary) = connect_pair();
        let oversized = vec![0u8; primary.get_mtu() + 1];
        let result = primary.send_packet(FD_PRIMARY_ADDR, &oversized, 0);
        assert_eq!(result, Err(TinyError::DataTooLarge));
    }

    #[test]
    fn test_fd_send_before_connect() {
        let mut primary = TinyFd::new(&default_config()).unwrap();
        assert!(primary.get_status().is_err());

        // Queue a packet — should succeed since the I-frame queue has free slots
        let payload = vec![0x01, 0x02];
        let result = primary.send_packet(FD_PRIMARY_ADDR, &payload, 0);
        assert!(result.is_ok());

        // But no I-frame data should be generated until connected
        let mut buf = vec![0u8; 256];
        let w = primary.get_tx_data(&mut buf, 0);
        // Only U-frames (SABM) should be generated, not I-frames with our payload
        // Verify the queued data doesn't get delivered to a non-existent peer
        let mut rx = TinyFd::new(&TinyFdConfig {
            addr: 1,
            ..default_config()
        }).unwrap();
        let received = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let rd = received.clone();
        rx.set_on_read(move |_addr, data: &[u8]| {
            rd.lock().unwrap().push(data.to_vec());
        });
        if w > 0 {
            rx.on_rx_data(&buf[..w]).unwrap();
        }
        let frames = received.lock().unwrap();
        assert!(frames.is_empty(), "I-frame data should not be sent before connection");
    }
}
