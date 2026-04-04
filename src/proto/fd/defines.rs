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

/// Connection states for the FD protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdState {
    /// Initial state — not yet started.
    Idle = 0,
    /// Link is down.
    Disconnected,
    /// Connection handshake in progress.
    Connecting,
    /// Link is established and data can flow.
    Connected,
    /// Graceful disconnect in progress.
    Disconnecting,
}

/// HDLC frame types (I, S, U).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// Information frame — carries user data.
    I = 0x00,
    /// Supervisory frame — flow/error control.
    S = 0x01,
    /// Unnumbered frame — link management.
    U = 0x02,
}

/// S-frame subtypes for supervisory frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SFrameSubtype {
    /// Receive Ready — positive acknowledgement.
    Rr = 0x00,
    /// Receive Not Ready — flow control busy.
    Rnr = 0x04,
    /// Reject — request retransmission from N(R).
    Rej = 0x08,
    /// Selective Reject — request single frame retransmission.
    Srej = 0x0C,
}

/// U-frame subtypes for unnumbered frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UFrameSubtype {
    /// Unnumbered Acknowledge.
    Ua = 0x60,
    /// Disconnected Mode.
    Dm = 0x0C,
    /// Frame Reject.
    Frmr = 0x84,
    /// Reset.
    Rset = 0x8C,
    /// Set Asynchronous Balanced Mode.
    Sabm = 0x2C,
    /// Set Normal Response Mode.
    Snrm = 0x80,
    /// Disconnect.
    Disc = 0x40,
    /// Unnumbered Information.
    Ui = 0x00,
}

/// Direction of a frame relative to the local station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDirection {
    /// Incoming (received) frame.
    In = 0x00,
    /// Outgoing (transmitted) frame.
    Out = 0x01,
}

// HDLC control field bit masks

/// I-frame identification bits.
pub const HDLC_I_FRAME_BITS: u8 = 0x00;
/// Mask to detect I-frame (bit 0 = 0).
pub const HDLC_I_FRAME_MASK: u8 = 0x01;

/// S-frame identification bits.
pub const HDLC_S_FRAME_BITS: u8 = 0x01;
/// Mask to detect S-frame (bits 0–1 = 01).
pub const HDLC_S_FRAME_MASK: u8 = 0x03;
/// S-frame subtype: Receive Ready.
pub const HDLC_S_FRAME_TYPE_RR: u8 = 0x00;
/// S-frame subtype: Receive Not Ready.
pub const HDLC_S_FRAME_TYPE_RNR: u8 = 0x04;
/// S-frame subtype: Reject.
pub const HDLC_S_FRAME_TYPE_REJ: u8 = 0x08;
/// S-frame subtype: Selective Reject.
pub const HDLC_S_FRAME_TYPE_SREJ: u8 = 0x0C;
/// Mask for extracting S-frame subtype.
pub const HDLC_S_FRAME_TYPE_MASK: u8 = 0x0C;

/// U-frame identification bits.
pub const HDLC_U_FRAME_BITS: u8 = 0x03;
/// Mask to detect U-frame (bits 0–1 = 11).
pub const HDLC_U_FRAME_MASK: u8 = 0x03;
/// U-frame type: Unnumbered Acknowledge.
pub const HDLC_U_FRAME_TYPE_UA: u8 = 0x60;
/// U-frame type: Disconnected Mode.
pub const HDLC_U_FRAME_TYPE_DM: u8 = 0x0C;
/// U-frame type: Frame Reject.
pub const HDLC_U_FRAME_TYPE_FRMR: u8 = 0x84;
/// U-frame type: Reset.
pub const HDLC_U_FRAME_TYPE_RSET: u8 = 0x8C;
/// U-frame type: Set Asynchronous Balanced Mode.
pub const HDLC_U_FRAME_TYPE_SABM: u8 = 0x2C;
/// U-frame type: Set Normal Response Mode.
pub const HDLC_U_FRAME_TYPE_SNRM: u8 = 0x80;
/// U-frame type: Disconnect.
pub const HDLC_U_FRAME_TYPE_DISC: u8 = 0x40;
/// U-frame type: Unnumbered Information.
pub const HDLC_U_FRAME_TYPE_UI: u8 = 0x00;
/// Mask for extracting U-frame subtype.
pub const HDLC_U_FRAME_TYPE_MASK: u8 = 0xEC;

/// Poll bit in the control field.
pub const HDLC_P_BIT: u8 = 0x10;
/// Final bit in the control field (same position as P bit).
pub const HDLC_F_BIT: u8 = 0x10;

/// Command/Response bit
pub const HDLC_CR_BIT: u8 = 0x02;
/// Extension bit — if set, address is 1 byte
pub const HDLC_E_BIT: u8 = 0x01;

/// Primary station HDLC address (shifted and with extension bit).
pub const HDLC_PRIMARY_ADDR: u8 = FD_PRIMARY_ADDR << 2;
/// Sentinel value indicating no valid peer was found.
pub const HDLC_INVALID_PEER_INDEX: u8 = 0xFF;

/// Mask for 3-bit sequence numbers (N(S) / N(R)).
pub const SEQ_BITS_MASK: u8 = 0x07;

// Event bits

/// TX engine is currently sending data.
pub const FD_EVENT_TX_SENDING: u8 = 0x01;
/// New TX data is available for the encoder.
pub const FD_EVENT_TX_DATA_AVAILABLE: u8 = 0x02;
/// I-frame queue has at least one free slot.
pub const FD_EVENT_QUEUE_HAS_FREE_SLOTS: u8 = 0x04;
/// Peer can accept I-frames (connected state).
pub const FD_EVENT_CAN_ACCEPT_I_FRAMES: u8 = 0x08;
/// Marker event for NRM polling.
pub const FD_EVENT_HAS_MARKER: u8 = 0x10;

/// Maximum number of U-frames in service queue
pub const U_QUEUE_MAX_SIZE: usize = 4;
