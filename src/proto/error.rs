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

/// Error codes matching the C library's TINY_ERR_* definitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TinyError {
    /// Generic failure (TINY_ERR_FAILED = -1)
    Failed,
    /// Timeout happened (TINY_ERR_TIMEOUT = -2)
    Timeout,
    /// Data too large to fit the buffer (TINY_ERR_DATA_TOO_LARGE = -3)
    DataTooLarge,
    /// Invalid data passed to API function (TINY_ERR_INVALID_DATA = -4)
    InvalidData,
    /// Operation cannot be performed right now (TINY_ERR_BUSY = -5)
    Busy,
    /// Received data not part of a frame (TINY_ERR_OUT_OF_SYNC = -6)
    OutOfSync,
    /// No data available, retry later (TINY_ERR_AGAIN = -7)
    Again,
    /// Invalid CRC field (TINY_ERR_WRONG_CRC = -8)
    WrongCrc,
    /// Out of memory (TINY_ERR_OUT_OF_MEMORY = -9)
    OutOfMemory,
    /// Unknown remote peer (TINY_ERR_UNKNOWN_PEER = -10)
    UnknownPeer,
    /// IO error (TINY_ERR_IO = -11)
    Io,
}

impl TinyError {
    /// Convert from C-style error code to TinyError
    pub fn from_code(code: i32) -> Option<TinyError> {
        match code {
            -1 => Some(TinyError::Failed),
            -2 => Some(TinyError::Timeout),
            -3 => Some(TinyError::DataTooLarge),
            -4 => Some(TinyError::InvalidData),
            -5 => Some(TinyError::Busy),
            -6 => Some(TinyError::OutOfSync),
            -7 => Some(TinyError::Again),
            -8 => Some(TinyError::WrongCrc),
            -9 => Some(TinyError::OutOfMemory),
            -10 => Some(TinyError::UnknownPeer),
            -11 => Some(TinyError::Io),
            _ => None,
        }
    }

    /// Convert to C-style error code
    pub fn to_code(self) -> i32 {
        match self {
            TinyError::Failed => -1,
            TinyError::Timeout => -2,
            TinyError::DataTooLarge => -3,
            TinyError::InvalidData => -4,
            TinyError::Busy => -5,
            TinyError::OutOfSync => -6,
            TinyError::Again => -7,
            TinyError::WrongCrc => -8,
            TinyError::OutOfMemory => -9,
            TinyError::UnknownPeer => -10,
            TinyError::Io => -11,
        }
    }
}

impl std::fmt::Display for TinyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TinyError::Failed => write!(f, "operation failed"),
            TinyError::Timeout => write!(f, "timeout"),
            TinyError::DataTooLarge => write!(f, "data too large"),
            TinyError::InvalidData => write!(f, "invalid data"),
            TinyError::Busy => write!(f, "busy"),
            TinyError::OutOfSync => write!(f, "out of sync"),
            TinyError::Again => write!(f, "try again"),
            TinyError::WrongCrc => write!(f, "wrong CRC"),
            TinyError::OutOfMemory => write!(f, "out of memory"),
            TinyError::UnknownPeer => write!(f, "unknown peer"),
            TinyError::Io => write!(f, "I/O error"),
        }
    }
}

impl std::error::Error for TinyError {}

/// Result type for tinyproto operations
pub type TinyResult<T> = Result<T, TinyError>;

/// Flags for API functions, matching C library's TINY_FLAG_* definitions
pub mod flags {
    /// Non-blocking operation
    pub const NO_WAIT: u32 = 0;
    /// Read entire frame even if it doesn't fit the buffer
    pub const READ_ALL: u32 = 1;
    /// Caller wants to start transmitting a new frame
    pub const LOCK_SEND: u32 = 2;
    /// Blocking operation (wait forever)
    pub const WAIT_FOREVER: u32 = 0x80;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_roundtrip() {
        let errors = [
            TinyError::Failed,
            TinyError::Timeout,
            TinyError::DataTooLarge,
            TinyError::InvalidData,
            TinyError::Busy,
            TinyError::OutOfSync,
            TinyError::Again,
            TinyError::WrongCrc,
            TinyError::OutOfMemory,
            TinyError::UnknownPeer,
            TinyError::Io,
        ];
        for err in errors {
            let code = err.to_code();
            assert_eq!(TinyError::from_code(code), Some(err));
        }
        assert_eq!(TinyError::from_code(0), None);
        assert_eq!(TinyError::from_code(1), None);
    }
}
