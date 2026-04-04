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

/// HDLC frame header (address + control fields)
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameHeader {
    pub address: u8,
    pub control: u8,
}

impl FrameHeader {
    pub fn to_bytes(&self) -> [u8; 2] {
        [self.address, self.control]
    }

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

/// Type of queued frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedFrameType {
    Free = 0x01,
    UFrame = 0x02,
    SFrame = 0x04,
    IFrame = 0x08,
}

/// Information about a queued frame
#[derive(Debug, Clone)]
pub struct FrameInfo {
    pub frame_type: QueuedFrameType,
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

impl FrameInfo {
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

/// I-frame specific info (extends FrameInfo with sequence tracking)
#[derive(Debug, Clone)]
pub struct IFrameInfo {
    pub frame: FrameInfo,
    pub peer_address: u8,
}
