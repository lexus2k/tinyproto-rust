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

//! Peer management for FD protocol.
//!
//! Handles primary/secondary station logic, address-to-peer mapping,
//! and per-peer connection state.

use super::defines::*;
use super::i_queue_control::IQueueControl;
use std::time::{Duration, Instant};

/// Per-peer connection state
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Connection state
    pub state: FdState,
    /// Peer address (in address field format)
    pub addr: u8,
    /// Last N(R) we sent back to this peer
    pub sent_nr: u8,
    /// Whether we already sent a REJ for this peer
    pub sent_reject: bool,
    /// Bitmask of selective rejection requests
    pub srej_req_mask: u8,
    /// I-frame queue control for this peer
    pub i_queue_control: IQueueControl,
    /// Timestamp of last sent I-frame
    pub last_sent_i_ts: Instant,
    /// Timestamp of last sent frame (any type)
    pub last_sent_frame_ts: Instant,
    /// Timestamp of last received frame
    pub last_received_frame_ts: Instant,
    /// Whether keep-alive was confirmed
    pub ka_confirmed: bool,
    /// Number of retries remaining
    pub retries: u8,
    /// Event flags
    pub events: u8,
}

impl PeerInfo {
    pub fn new(addr: u8) -> Self {
        // Initialize timestamps far in the past so initial timeout triggers immediately
        let past = Instant::now() - Duration::from_secs(60);
        PeerInfo {
            state: FdState::Disconnected,
            addr,
            sent_nr: 0,
            sent_reject: false,
            srej_req_mask: 0,
            i_queue_control: IQueueControl::new(),
            last_sent_i_ts: past,
            last_sent_frame_ts: past,
            last_received_frame_ts: past,
            ka_confirmed: false,
            retries: 0,
            events: 0,
        }
    }

    /// Reset connection state
    pub fn reset_connection(&mut self) {
        self.i_queue_control.reset();
        self.sent_nr = 0;
        self.sent_reject = false;
        self.srej_req_mask = 0;
        let now = Instant::now();
        self.last_received_frame_ts = now;
        self.last_sent_frame_ts = now;
    }
}

/// Check if an address field represents the primary station
pub fn is_primary_address(address: u8) -> bool {
    (address & !HDLC_CR_BIT) == (HDLC_PRIMARY_ADDR | HDLC_E_BIT)
}

/// Peer management for the FD protocol
pub struct PeerManager {
    pub peers: Vec<PeerInfo>,
    pub local_addr: u8,
    pub next_peer: u8,
}

impl PeerManager {
    /// Create a new peer manager.
    /// For primary stations, `local_addr` is `HDLC_PRIMARY_ADDR | HDLC_E_BIT`.
    /// For secondary stations, `local_addr` is `(station_addr << 2) | HDLC_E_BIT`.
    pub fn new(local_addr: u8, peers_count: u8) -> Self {
        let count = std::cmp::max(peers_count, 1) as usize;
        let mut peers = Vec::with_capacity(count);

        if is_primary_address(local_addr) {
            // Primary: peers are secondary stations with sequential addresses
            for i in 0..count {
                let peer_addr = ((i as u8 + 1) << 2) | HDLC_E_BIT;
                peers.push(PeerInfo::new(peer_addr));
            }
        } else {
            // Secondary: single peer is the primary station
            peers.push(PeerInfo::new(HDLC_PRIMARY_ADDR | HDLC_E_BIT));
        }

        PeerManager {
            peers,
            local_addr,
            next_peer: 0,
        }
    }

    /// Check if local station is primary
    pub fn is_primary_station(&self) -> bool {
        is_primary_address(self.local_addr)
    }

    /// Check if local station is secondary
    pub fn is_secondary_station(&self) -> bool {
        !self.is_primary_station()
    }

    /// Convert peer index to address field value.
    /// In HDLC, the address field always identifies the secondary station.
    /// Primary: returns the peer's address (peer IS the secondary).
    /// Secondary: returns own address (identifies itself to primary).
    pub fn peer_to_address_field(&self, peer: u8) -> u8 {
        if self.is_primary_station() {
            self.peers[peer as usize].addr & !HDLC_CR_BIT
        } else {
            self.local_addr & !HDLC_CR_BIT
        }
    }

    /// Find peer index from address field, or HDLC_INVALID_PEER_INDEX
    pub fn address_field_to_peer(&self, address: u8) -> u8 {
        let addr_no_cr = address & !HDLC_CR_BIT;
        if self.is_primary_station() {
            // Primary: address field = secondary's address → look up which peer
            for (i, peer) in self.peers.iter().enumerate() {
                if (peer.addr & !HDLC_CR_BIT) == addr_no_cr {
                    return i as u8;
                }
            }
            HDLC_INVALID_PEER_INDEX
        } else {
            // Secondary: address field should be our own address (HDLC convention)
            // or the primary address in some cases
            let our_addr = self.local_addr & !HDLC_CR_BIT;
            if addr_no_cr == our_addr || is_primary_address(addr_no_cr) {
                return 0; // only peer is the primary
            }
            HDLC_INVALID_PEER_INDEX
        }
    }

    /// Advance to next peer (for round-robin in NRM mode)
    pub fn switch_to_next_peer(&mut self) -> u8 {
        self.next_peer = ((self.next_peer as usize + 1) % self.peers.len()) as u8;
        self.next_peer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_station() {
        let pm = PeerManager::new(HDLC_PRIMARY_ADDR | HDLC_E_BIT, 1);
        assert!(pm.is_primary_station());
        assert!(!pm.is_secondary_station());
    }

    #[test]
    fn test_secondary_station() {
        let addr = (1 << 2) | HDLC_E_BIT;
        let pm = PeerManager::new(addr, 1);
        assert!(!pm.is_primary_station());
        assert!(pm.is_secondary_station());
    }

    #[test]
    fn test_address_resolution_secondary() {
        let addr = (1 << 2) | HDLC_E_BIT;
        let pm = PeerManager::new(addr, 1);
        // Secondary should accept frames from primary
        let peer = pm.address_field_to_peer(HDLC_PRIMARY_ADDR | HDLC_E_BIT);
        assert_eq!(peer, 0);
        // Should reject frames from unknown
        let peer = pm.address_field_to_peer((5 << 2) | HDLC_E_BIT);
        assert_eq!(peer, HDLC_INVALID_PEER_INDEX);
    }
}
