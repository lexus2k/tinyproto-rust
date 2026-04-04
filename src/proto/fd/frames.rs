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

//! Frame type definitions for FD protocol.

/// HDLC frame header (address + control fields).
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameHeader {
    /// HDLC address field (identifies the secondary station).
    pub address: u8,
    /// HDLC control field (frame type, sequence numbers, P/F bit).
    pub control: u8,
}

impl FrameHeader {
    /// Serialize the header to a 2-byte array `[address, control]`.
    pub fn to_bytes(&self) -> [u8; 2] {
        [self.address, self.control]
    }

    /// Parse a header from the first two bytes of `data`, or `None` if too short.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }
        Some(FrameHeader {
            address: data[0],
            control: data[1],
        })
    }
}

/// Type tag for a frame slot in a [`super::frame_queue::FrameQueue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedFrameType {
    /// Slot is available.
    Free = 0x01,
    /// Unnumbered frame.
    UFrame = 0x02,
    /// Supervisory frame.
    SFrame = 0x04,
    /// Information frame.
    IFrame = 0x08,
}

/// Metadata and payload for a queued HDLC frame.
#[derive(Debug, Clone)]
pub struct FrameInfo {
    /// The type tag (I, S, U, or Free).
    pub frame_type: QueuedFrameType,
    /// Address and control bytes.
    pub header: FrameHeader,
    /// User payload (without header).
    pub payload: Vec<u8>,
}

impl FrameInfo {
    /// Create a new `FrameInfo` by copying the payload slice.
    pub fn new(frame_type: QueuedFrameType, header: FrameHeader, payload: &[u8]) -> Self {
        FrameInfo {
            frame_type,
            header,
            payload: payload.to_vec(),
        }
    }

    /// Total frame length including header
    pub fn total_len(&self) -> usize {
        2 + self.payload.len()
    }

    /// Serialize to bytes (header + payload)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.total_len());
        data.push(self.header.address);
        data.push(self.header.control);
        data.extend_from_slice(&self.payload);
        data
    }
}

/// I-frame information with peer address tracking.
#[derive(Debug, Clone)]
pub struct IFrameInfo {
    /// The underlying frame data.
    pub frame: FrameInfo,
    /// HDLC address of the peer this frame belongs to.
    pub peer_address: u8,
}
