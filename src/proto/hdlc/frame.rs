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

//use std::slice::SliceIndex;

pub struct HdlcFrame {
    data: Vec<u8>,
}

impl<Idx> std::ops::Index<Idx> for HdlcFrame
where
    Idx: std::slice::SliceIndex<[u8]>,
{
    type Output = Idx::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.data[index]
    }
}

impl HdlcFrame {
    pub fn new() -> HdlcFrame {
        HdlcFrame {
            data: Vec::new(),
        }
    }

    pub fn push(&mut self, byte: u8) {
        self.data.push(byte);
    }
}

#[cfg(test)]
mod unittest {
    use super::*;

    #[test]
    fn test_frame() {
        let mut frame = HdlcFrame::new();
        frame.push(0x7F);
        frame.push(0x7E);
        frame.push(0x7D);
        frame.push(0x00);
        assert_eq!(frame[0], 0x7F);
        assert_eq!(frame[1], 0x7E);
        assert_eq!(frame[2], 0x7D);
        assert_eq!(frame[3], 0x00);
    }
}

