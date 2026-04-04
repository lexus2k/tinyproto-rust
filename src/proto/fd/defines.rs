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

//! FD protocol constants and definitions.

/// Primary station address
pub const FD_PRIMARY_ADDR: u8 = 0;

/// Protocol modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdMode {
    /// Asynchronous Balanced Mode — peer-to-peer
    Abm = 0x00,
    /// Normal Response Mode — primary/secondary (polling)
    Nrm = 0x01,
}

/// Connection states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdState {
    Idle = 0,
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// Frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    I = 0x00,
    S = 0x01,
    U = 0x02,
}

/// S-frame subtypes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SFrameSubtype {
    Rr = 0x00,
    Rnr = 0x04,
    Rej = 0x08,
    Srej = 0x0C,
}

/// U-frame subtypes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UFrameSubtype {
    Ua = 0x60,
    Dm = 0x0C,
    Frmr = 0x84,
    Rset = 0x8C,
    Sabm = 0x2C,
    Snrm = 0x80,
    Disc = 0x40,
    Ui = 0x00,
}

/// Frame direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDirection {
    In = 0x00,
    Out = 0x01,
}

// HDLC control field bit masks
pub const HDLC_I_FRAME_BITS: u8 = 0x00;
pub const HDLC_I_FRAME_MASK: u8 = 0x01;

pub const HDLC_S_FRAME_BITS: u8 = 0x01;
pub const HDLC_S_FRAME_MASK: u8 = 0x03;
pub const HDLC_S_FRAME_TYPE_RR: u8 = 0x00;
pub const HDLC_S_FRAME_TYPE_RNR: u8 = 0x04;
pub const HDLC_S_FRAME_TYPE_REJ: u8 = 0x08;
pub const HDLC_S_FRAME_TYPE_SREJ: u8 = 0x0C;
pub const HDLC_S_FRAME_TYPE_MASK: u8 = 0x0C;

pub const HDLC_U_FRAME_BITS: u8 = 0x03;
pub const HDLC_U_FRAME_MASK: u8 = 0x03;
pub const HDLC_U_FRAME_TYPE_UA: u8 = 0x60;
pub const HDLC_U_FRAME_TYPE_DM: u8 = 0x0C;
pub const HDLC_U_FRAME_TYPE_FRMR: u8 = 0x84;
pub const HDLC_U_FRAME_TYPE_RSET: u8 = 0x8C;
pub const HDLC_U_FRAME_TYPE_SABM: u8 = 0x2C;
pub const HDLC_U_FRAME_TYPE_SNRM: u8 = 0x80;
pub const HDLC_U_FRAME_TYPE_DISC: u8 = 0x40;
pub const HDLC_U_FRAME_TYPE_UI: u8 = 0x00;
pub const HDLC_U_FRAME_TYPE_MASK: u8 = 0xEC;

pub const HDLC_P_BIT: u8 = 0x10;
pub const HDLC_F_BIT: u8 = 0x10;

/// Command/Response bit
pub const HDLC_CR_BIT: u8 = 0x02;
/// Extension bit — if set, address is 1 byte
pub const HDLC_E_BIT: u8 = 0x01;

pub const HDLC_PRIMARY_ADDR: u8 = FD_PRIMARY_ADDR << 2;
pub const HDLC_INVALID_PEER_INDEX: u8 = 0xFF;

pub const SEQ_BITS_MASK: u8 = 0x07;

// Event bits
pub const FD_EVENT_TX_SENDING: u8 = 0x01;
pub const FD_EVENT_TX_DATA_AVAILABLE: u8 = 0x02;
pub const FD_EVENT_QUEUE_HAS_FREE_SLOTS: u8 = 0x04;
pub const FD_EVENT_CAN_ACCEPT_I_FRAMES: u8 = 0x08;
pub const FD_EVENT_HAS_MARKER: u8 = 0x10;

/// Maximum number of U-frames in service queue
pub const U_QUEUE_MAX_SIZE: usize = 4;
