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

//! I-frame queue control — N(S)/N(R) sequence tracking.
//!
//! Manages frame sequence numbering, acknowledgement tracking,
//! and retransmission for the sliding window protocol.

use super::defines::SEQ_BITS_MASK;

/// TX-side sequence state
#[derive(Debug, Clone)]
pub struct IQueueControlSend {
    /// Next free frame number to assign (sequence wraps at 8)
    pub last_ns: u8,
    /// Next sent frame number to be confirmed by remote
    pub confirm_ns: u8,
    /// Next frame number to send
    pub next_ns: u8,
}

/// RX-side sequence state
#[derive(Debug, Clone)]
pub struct IQueueControlRecv {
    /// Next expected frame number from remote
    pub next_nr: u8,
}

/// Combined I-frame queue control
#[derive(Debug, Clone)]
pub struct IQueueControl {
    pub tx_state: IQueueControlSend,
    pub rx_state: IQueueControlRecv,
}

impl IQueueControl {
    pub fn new() -> Self {
        IQueueControl {
            tx_state: IQueueControlSend {
                last_ns: 0,
                confirm_ns: 0,
                next_ns: 0,
            },
            rx_state: IQueueControlRecv {
                next_nr: 0,
            },
        }
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.tx_state.last_ns = 0;
        self.tx_state.confirm_ns = 0;
        self.tx_state.next_ns = 0;
        self.rx_state.next_nr = 0;
    }

    /// Get the next frame number to be confirmed
    pub fn get_next_frame_to_confirm(&self) -> u8 {
        self.tx_state.confirm_ns
    }

    /// Confirm sent frames up to the given N(R).
    /// Calls the callback for each confirmed frame.
    /// Returns true if any frames were confirmed.
    pub fn confirm_sent_frames<F>(&mut self, nr: u8, mut on_confirm: F) -> bool
    where
        F: FnMut(u8) -> bool,
    {
        let mut confirmed = false;
        while self.tx_state.confirm_ns != nr {
            if !on_confirm(self.tx_state.confirm_ns) {
                break;
            }
            self.tx_state.confirm_ns = (self.tx_state.confirm_ns + 1) & SEQ_BITS_MASK;
            confirmed = true;
        }
        confirmed
    }

    /// Request retransmission of frame with given N(R).
    /// Rewinds next_ns to the specified frame.
    pub fn retransmit_frame<F>(&mut self, nr: u8, mut on_retransmit: F) -> bool
    where
        F: FnMut(u8) -> bool,
    {
        if self.tx_state.next_ns == nr {
            return false;
        }
        // Rewind next_ns
        self.tx_state.next_ns = nr;
        on_retransmit(nr)
    }

    /// Get the next frame sequence number to send
    pub fn get_next_frame_to_send(&self) -> u8 {
        self.tx_state.next_ns
    }

    /// Get the next expected frame number to receive
    pub fn get_next_frame_to_receive(&self) -> u8 {
        self.rx_state.next_nr
    }

    /// Advance RX expected frame number
    pub fn move_to_next_frame_to_receive(&mut self) {
        self.rx_state.next_nr = (self.rx_state.next_nr + 1) & SEQ_BITS_MASK;
    }

    /// Move last_ns back by one (undo allocation)
    pub fn move_to_previous_ns(&mut self) {
        self.tx_state.last_ns = (self.tx_state.last_ns.wrapping_sub(1)) & SEQ_BITS_MASK;
    }

    /// Advance last_ns (allocate next slot)
    pub fn move_to_next_ns(&mut self) {
        self.tx_state.last_ns = (self.tx_state.last_ns + 1) & SEQ_BITS_MASK;
    }

    /// Advance next_ns (mark frame as ready to send)
    pub fn advance_next_ns(&mut self) {
        self.tx_state.next_ns = (self.tx_state.next_ns + 1) & SEQ_BITS_MASK;
    }

    /// Check if there are unconfirmed frames in flight
    pub fn has_unconfirmed_frames(&self) -> bool {
        self.tx_state.confirm_ns != self.tx_state.last_ns
    }

    /// Check if all allocated frames have been sent
    pub fn all_frames_are_sent(&self) -> bool {
        self.tx_state.next_ns == self.tx_state.last_ns
    }

    /// Check if TX window is full (can't allocate more)
    pub fn tx_full(&self) -> bool {
        ((self.tx_state.last_ns + 1) & SEQ_BITS_MASK) == self.tx_state.confirm_ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_sequence() {
        let mut ctrl = IQueueControl::new();
        assert_eq!(ctrl.get_next_frame_to_send(), 0);
        assert_eq!(ctrl.get_next_frame_to_confirm(), 0);
        assert!(!ctrl.has_unconfirmed_frames());

        // Allocate frame 0
        ctrl.move_to_next_ns();
        assert!(ctrl.has_unconfirmed_frames());
        assert!(!ctrl.all_frames_are_sent());

        // Send frame 0
        ctrl.advance_next_ns();
        assert!(ctrl.all_frames_are_sent());

        // Confirm frame 0
        let confirmed = ctrl.confirm_sent_frames(1, |_| true);
        assert!(confirmed);
        assert!(!ctrl.has_unconfirmed_frames());
    }

    #[test]
    fn test_tx_full() {
        let mut ctrl = IQueueControl::new();
        // Fill window (7 frames for 3-bit sequence)
        for _ in 0..7 {
            assert!(!ctrl.tx_full());
            ctrl.move_to_next_ns();
        }
        assert!(ctrl.tx_full());
    }

    #[test]
    fn test_sequence_wrap() {
        let mut ctrl = IQueueControl::new();
        for i in 0..16 {
            ctrl.move_to_next_ns();
            ctrl.advance_next_ns();
            let nr = (i + 1) & SEQ_BITS_MASK;
            ctrl.confirm_sent_frames(nr, |_| true);
        }
        assert_eq!(ctrl.get_next_frame_to_send(), 0);
    }

    #[test]
    fn test_rx_sequence() {
        let mut ctrl = IQueueControl::new();
        assert_eq!(ctrl.get_next_frame_to_receive(), 0);
        ctrl.move_to_next_frame_to_receive();
        assert_eq!(ctrl.get_next_frame_to_receive(), 1);
    }
}
