/*
    Copyright 2024 (C) Alexey Dynda

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

//! # tinyproto — Rust HDLC Protocol Library
//!
//! `tinyproto` is a Rust implementation of the
//! [TinyProto](https://github.com/lexus2k/tinyproto) HDLC protocol library,
//! originally written in C/C++ for embedded systems.
//!
//! The library provides three protocol layers of increasing complexity:
//!
//! - **HDLC low-level** (`proto::hdlc`) — Raw HDLC framing with byte-stuffing
//!   and CRC verification. Processes data byte-by-byte via streaming state
//!   machines, suitable for integration with any transport.
//!
//! - **Light** (`proto::light`) — A simple blocking send/receive protocol built
//!   on top of HDLC framing. No acknowledgement or flow control — ideal for
//!   point-to-point links where simplicity is preferred.
//!
//! - **Full-Duplex (FD)** (`proto::fd`) — A full-featured protocol implementing
//!   HDLC ABM (Asynchronous Balanced Mode) and NRM (Normal Response Mode) with
//!   I/S/U frames, sliding-window flow control, connection management, and
//!   timeout-based retransmission.
//!
//! All layers support configurable CRC modes: **CRC-8** (simple checksum),
//! **CRC-16** (CCITT-16 / FCS-16), and **CRC-32** (CCITT-32 / FCS-32),
//! as well as no-CRC mode for testing.

/// Protocol implementations for TinyProto.
pub mod proto {
    /// Error types and result aliases used throughout the library.
    pub mod error;
    /// CRC computation (CRC-8, CRC-16, CRC-32) for HDLC frame integrity.
    pub mod crc;
    /// HDLC framing layer — low-level encoding/decoding and high-level wrappers.
    pub mod hdlc {
        /// HDLC frame data container.
        pub mod frame;
        /// Pipeline node abstraction for chaining protocol stages.
        pub mod node;
        /// One-shot HDLC encoder/decoder (non-streaming).
        pub mod low_level;
        /// Streaming HDLC low-level state machine (byte-by-byte processing).
        pub mod low_level_ll;
        /// High-level HDLC protocol with event-based synchronization.
        pub mod high_level;
    }
    /// Light protocol — simple blocking send/receive over HDLC framing.
    pub mod light;
    /// Full-Duplex protocol — connection-oriented HDLC with sliding window.
    pub mod fd;
}

