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

//! Frame queue for FD protocol.
//!
//! Manages queues of I-frames and S/U-frames pending transmission.

use super::frames::{FrameInfo, FrameHeader, QueuedFrameType};

/// Queue for storing frames pending transmission
pub struct FrameQueue {
    frames: Vec<Option<FrameInfo>>,
    mtu: usize,
    lookup_index: usize,
}

impl FrameQueue {
    /// Create a new frame queue
    pub fn new(max_frames: usize, mtu: usize) -> Self {
        let mut frames = Vec::with_capacity(max_frames);
        for _ in 0..max_frames {
            frames.push(None);
        }
        FrameQueue {
            frames,
            mtu,
            lookup_index: 0,
        }
    }

    /// Reset the queue, freeing all frames
    pub fn reset(&mut self) {
        for slot in self.frames.iter_mut() {
            *slot = None;
        }
        self.lookup_index = 0;
    }

    /// Reset only frames matching a specific address
    pub fn reset_for(&mut self, address: u8) {
        for slot in self.frames.iter_mut() {
            if let Some(ref frame) = slot {
                if frame.header.address == address {
                    *slot = None;
                }
            }
        }
    }

    /// Check if there are free slots
    pub fn has_free_slots(&self) -> bool {
        self.frames.iter().any(|s| s.is_none())
    }

    /// Get the MTU
    pub fn get_mtu(&self) -> usize {
        self.mtu
    }

    /// Allocate a slot and store a frame. Returns index or None if full.
    pub fn allocate(&mut self, frame_type: QueuedFrameType, header: FrameHeader, data: &[u8]) -> Option<usize> {
        let len = self.frames.len();
        for i in 0..len {
            let idx = (self.lookup_index + i) % len;
            if self.frames[idx].is_none() {
                self.frames[idx] = Some(FrameInfo::new(frame_type, header, data));
                self.lookup_index = (idx + 1) % len;
                return Some(idx);
            }
        }
        None
    }

    /// Get the next frame matching the specified type and address.
    /// For I-frames, `arg` is the frame sequence number to match against position.
    pub fn get_next(&self, frame_type: QueuedFrameType, address: u8, _arg: u8) -> Option<(usize, &FrameInfo)> {
        for (i, slot) in self.frames.iter().enumerate() {
            if let Some(ref frame) = slot {
                if frame.frame_type == frame_type && frame.header.address == address {
                    return Some((i, frame));
                }
            }
        }
        None
    }

    /// Get frame by index
    pub fn get(&self, index: usize) -> Option<&FrameInfo> {
        self.frames.get(index).and_then(|s| s.as_ref())
    }

    /// Get mutable frame by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut FrameInfo> {
        self.frames.get_mut(index).and_then(|s| s.as_mut())
    }

    /// Free a slot by index
    pub fn free(&mut self, index: usize) {
        if index < self.frames.len() {
            self.frames[index] = None;
        }
    }

    /// Free a slot by matching header address
    pub fn free_by_header(&mut self, address: u8, control: u8) {
        for slot in self.frames.iter_mut() {
            if let Some(ref frame) = slot {
                if frame.header.address == address && frame.header.control == control {
                    *slot = None;
                    return;
                }
            }
        }
    }

    /// Count occupied slots
    pub fn count(&self) -> usize {
        self.frames.iter().filter(|s| s.is_some()).count()
    }

    /// Total capacity
    pub fn capacity(&self) -> usize {
        self.frames.len()
    }
}

/// Combined frame queues for the FD protocol
pub struct FramesQueue {
    /// I-frame queue
    pub i_queue: FrameQueue,
    /// S/U-frame service queue
    pub s_queue: FrameQueue,
}

impl FramesQueue {
    pub fn new(i_queue_size: usize, s_queue_size: usize, mtu: usize) -> Self {
        FramesQueue {
            i_queue: FrameQueue::new(i_queue_size, mtu),
            s_queue: FrameQueue::new(s_queue_size, mtu),
        }
    }

    pub fn reset(&mut self) {
        self.i_queue.reset();
        self.s_queue.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_queue_basic() {
        let mut q = FrameQueue::new(4, 64);
        assert!(q.has_free_slots());

        let header = FrameHeader { address: 0x01, control: 0x00 };
        let idx = q.allocate(QueuedFrameType::IFrame, header, &[1, 2, 3]).unwrap();
        assert_eq!(q.count(), 1);

        let frame = q.get(idx).unwrap();
        assert_eq!(frame.payload, vec![1, 2, 3]);

        q.free(idx);
        assert_eq!(q.count(), 0);
    }

    #[test]
    fn test_frame_queue_full() {
        let mut q = FrameQueue::new(2, 64);
        let h = FrameHeader { address: 0x01, control: 0x00 };
        q.allocate(QueuedFrameType::IFrame, h, &[1]).unwrap();
        q.allocate(QueuedFrameType::IFrame, h, &[2]).unwrap();
        assert!(!q.has_free_slots());
        assert!(q.allocate(QueuedFrameType::IFrame, h, &[3]).is_none());
    }

    #[test]
    fn test_frame_queue_reset_for() {
        let mut q = FrameQueue::new(4, 64);
        let h1 = FrameHeader { address: 0x01, control: 0x00 };
        let h2 = FrameHeader { address: 0x05, control: 0x00 };
        q.allocate(QueuedFrameType::IFrame, h1, &[1]).unwrap();
        q.allocate(QueuedFrameType::IFrame, h2, &[2]).unwrap();
        q.allocate(QueuedFrameType::IFrame, h1, &[3]).unwrap();
        assert_eq!(q.count(), 3);

        q.reset_for(0x01);
        assert_eq!(q.count(), 1);
    }
}
