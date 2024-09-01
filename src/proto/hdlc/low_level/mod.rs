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

use crate::proto::crc;
use core::cmp::min;
use std::thread::sleep;
use std::time::Duration;

/** Byte to fill gap between frames */
const TINY_HDLC_FILL_BYTE: u8 = 0xFF;
const TINY_HDLC_ESCAPE_CHAR: u8 = 0x7D;
const TINY_HDLC_ESCAPE_BIT: u8 = 0x20;
const TINY_HDLC_FLAG_SEQUENCE: u8 = 0x7E;

pub trait RxProcessor {
    fn on_frame_read(&mut self, _pdata: *mut u8, _size: usize) {
    }
}

pub trait TxProcessor {
    fn on_frame_send(&mut self, _pdata: *const u8, _size: usize) {
    }
}

/**
 * Flags for hdlc_ll_reset function
 */
#[derive(PartialEq, Eq)]
pub enum ResultFlagsT {
    ResetBoth = 0x00,
    ResetTxOnly = 0x01,
    HwlcLowLevelResetRxOnly = 0x02,
}

#[derive(PartialEq, Eq, Debug)]
pub enum ResultT {
    Success,
    InvalidData,
    Error,
    Busy,
    DataTooLarge,
    WrongCrc,
}

struct Rx {
    rx_processor: *mut dyn RxProcessor,
    rx_buf: *mut u8,
    rx_buf_size: usize,
    crc_type: crc::HdlcCrcT,
    phys_mtu: isize,
    // state function for RX part
    state: fn(&mut Rx, *const u8, isize) -> (isize, ResultT),
    // data being processed
    data: *mut u8,
    // escape character variable
    escape: bool,
    // pointer to the start of the frame
    frame_buf: *mut u8,
}

struct Tx {
    tx_processor: *mut dyn TxProcessor,
    out_buffer: *mut u8,
    out_buffer_len: usize,
    origin_data: *const u8,
    data: *const u8,
    crc_type: crc::HdlcCrcT,
    len: isize,
    crc: u32,
    escape: bool,
    send_in_progress: bool,
    state: fn(&mut Tx) -> (isize, ResultT),
}

impl Rx {
    fn new(_rx_processor: *mut dyn RxProcessor, buf: *mut u8, buf_size: usize,
           crc_type: crc::HdlcCrcT, mtu: isize) -> Rx {
        Rx {
            rx_processor: _rx_processor,
            rx_buf: buf,
            rx_buf_size: buf_size,
            state: Rx::read_start,
            data: 0 as *mut u8,
            escape: false,
            frame_buf: buf,
            crc_type: crc_type,
            phys_mtu: if mtu == 0 { buf_size as isize } else { mtu + crc::get_crc_field_size(crc_type) as isize },
        }
    }

    pub fn reset(&mut self) {
        self.state = Rx::read_start;
        self.escape = false;
    }

    pub fn close(&mut self) {
        self.reset();
    }

    pub fn run_rx(&mut self, data: *const u8, len: isize, error: &mut ResultT) -> isize {
        let mut result = 0;
        let mut ptr : *const u8 = data;
        let mut _len = len;
        *error = ResultT::Success;
        while _len > 0 || self.state == Rx::read_end {
            let (temp_result, temp_error) = (self.state)(self, ptr, _len);
            if temp_error != ResultT::Success {
                *error = temp_error;
                break;
            }
            if temp_result <= 0 {
                break;
            }
            ptr = unsafe { ptr.offset(temp_result as isize) };
            _len -= temp_result;
            result += temp_result;
        }
        result
    }

    fn read_start(&mut self, _data: *const u8, _len: isize) -> (isize, ResultT) {
        if _len == 0 {
            return (0, ResultT::Success);
        }
        if unsafe { *_data } != TINY_HDLC_FLAG_SEQUENCE {
            if unsafe { *_data } != TINY_HDLC_FILL_BYTE {
                // For now we do nothing here,
                // But between frames we can receive fill bytes only
            }
            return (1, ResultT::Success);
        }
        self.data = self.frame_buf;
        self.escape = false;
        self.state = Rx::read_data;
        (1, ResultT::Success)
    }

    fn read_data(&mut self, _data: *const u8, _len: isize) -> (isize, ResultT) {
        let mut result = 0;
        let mut ptr = _data;
        let mut len = _len;
        while len > 0 {
            let byte = unsafe { *ptr };
            if byte == TINY_HDLC_FLAG_SEQUENCE {
                self.state = Rx::read_end;
                result += 1;
                break;
            }
            if byte == TINY_HDLC_ESCAPE_CHAR {
                self.escape = true;
            } else if unsafe { self.data.offset_from(self.frame_buf) } < self.phys_mtu {
                if self.escape == true {
                    self.escape = false;
                    unsafe { *self.data = byte ^ TINY_HDLC_ESCAPE_BIT }
                } else {
                    unsafe { *self.data = byte }
                }
                self.data = unsafe { self.data.offset(1) }
            }
            unsafe { ptr = ptr.offset(1) }
            result += 1;
            len -= 1;
        }
        (result, ResultT::Success)
    }

    fn read_end(&mut self, _data: *const u8, _len: isize) -> (isize, ResultT) {
        if self.data == self.frame_buf {
            // Impossible, maybe frame alignment is wrong, go to read data again
            self.escape = false;
            self.state = Rx::read_data;
            // That's OK, we actually didn't process anything from user bytes
            return (0, ResultT::Success);
        }
        self.state = Rx::read_start;
        let mut len = unsafe { self.data.offset_from(self.frame_buf) } as isize;
        if len > self.phys_mtu {
            // Buffer size issue, too long packet
            return (0, ResultT::DataTooLarge);
        }
        if len < crc::get_crc_field_size(self.crc_type) as isize {
            // CRC size issue
            return (0, ResultT::WrongCrc);
        }
        let mut calc_crc: u32 = 0;
        let mut read_crc: u32 = 0;
        match self.crc_type {
            crc::HdlcCrcT::HdlcCrc8 => {
                let mut crc = crc::Crc8::new();
                crc.sum_bytes(unsafe { &[*self.frame_buf] }, len as usize - 1);
                calc_crc = crc.get() as u32;
                read_crc = unsafe { *self.data.offset(-1) } as u32;
            }
            crc::HdlcCrcT::HdlcCrc16 => {
                let mut crc = crc::Crc16::new();
                crc.sum_bytes(unsafe { &[*self.frame_buf] }, len as usize - 2);
                calc_crc = crc.get() as u32;
                read_crc = ((unsafe { *self.data.offset(-2) } as u32) << 0) |
                           ((unsafe { *self.data.offset(-1) } as u32) << 8);
            }
            crc::HdlcCrcT::HdlcCrc32 => {
                let mut crc = crc::Crc32::new();
                crc.sum_bytes(unsafe { &[*self.frame_buf] }, len as usize - 4);
                calc_crc = crc.get();
                read_crc = ((unsafe { *self.data.offset(-4) } as u32) << 0)  |
                           ((unsafe { *self.data.offset(-3) } as u32) << 8)  |
                           ((unsafe { *self.data.offset(-2) } as u32) << 16) |
                           ((unsafe { *self.data.offset(-1) } as u32) << 24);
            }
            _ => {
            }
        }
        if calc_crc != read_crc {
            // CRC calculate issue
            return (0, ResultT::WrongCrc);
        }
        // Shift back data pointer, pointing to the last byte after payload
        len -= crc::get_crc_field_size(self.crc_type) as isize;
        // Call user callback to process received frame
        (unsafe { &mut *self.rx_processor }).on_frame_read(self.frame_buf, len as usize);
        self.frame_buf = unsafe { self.frame_buf.offset(self.phys_mtu) };
        if unsafe { self.frame_buf.offset_from(self.rx_buf) } + self.phys_mtu > self.rx_buf_size as isize {
            self.frame_buf = self.rx_buf;
        }
        (0, ResultT::Success)
    }

    fn rx_state_idle(&mut self, _data: *const u8, _len: isize) -> (isize, ResultT) {
        (0, ResultT::Success)
    }
}


impl Tx {
    pub fn new(_tx_processor: *mut dyn TxProcessor, _crc: crc::HdlcCrcT) -> Tx {
        Tx {
            tx_processor: _tx_processor,
            out_buffer: 0 as *mut u8,
            out_buffer_len: 0,
            origin_data: 0 as *const u8,
            data: 0 as *const u8,
            crc_type: _crc,
            len: 0,
            crc: 0,
            escape: false,
            send_in_progress: false,
            state: Tx::send_start,
        }
    }

    pub fn reset(&mut self) {
        self.escape = false;
        self.send_in_progress = false;
        self.state = Tx::send_start;
    }

    pub fn close(&mut self) {
        if self.send_in_progress == true
        {
            (unsafe { &mut *self.tx_processor }).on_frame_send(self.origin_data, self.data as usize - self.origin_data as usize);
        }
    }

    pub fn put(&mut self, data: *const u8, len: usize) -> ResultT {
        if self.send_in_progress == true {
            return ResultT::Busy;
        }
        if len == 0 {
            return ResultT::Success;
        }
        self.origin_data = data;
        self.data = data;
        self.len = len as isize;
        self.send_in_progress = true;
        ResultT::Success
    }

    fn send_start(&mut self) -> (isize, ResultT)
    {
        if !self.send_in_progress {
            return (0, ResultT::Success);
        }
        match self.crc_type {
            crc::HdlcCrcT::HdlcCrc8 => {
                let mut crc = crc::Crc8::new();
                crc.sum_bytes(unsafe { &[*self.data] }, self.len as usize);
                self.crc = crc.get() as u32;
            }
            crc::HdlcCrcT::HdlcCrc16 => {
                let mut crc = crc::Crc16::new();
                crc.sum_bytes(unsafe { &[*self.data] }, self.len as usize);
                self.crc = crc.get() as u32;
            }
            crc::HdlcCrcT::HdlcCrc32 => {
                let mut crc = crc::Crc32::new();
                crc.sum_bytes(unsafe { &[*self.data] }, self.len as usize);
                self.crc = crc.get();
            }
            _ => {
            }
        }
        let buf: u8 = TINY_HDLC_FLAG_SEQUENCE;
        let result = self.send_tx_internal(&buf, 1);
        if result == 1 {
            self.state = Tx::send_data;
            self.escape = false;
        }
        (result, ResultT::Success)
    }

    fn send_data(&mut self) -> (isize, ResultT) {
        let mut pos: isize = 0;
        while unsafe { *self.data.offset(pos) } != TINY_HDLC_FLAG_SEQUENCE && unsafe { *self.data.offset(pos) } != TINY_HDLC_ESCAPE_CHAR  && pos < self.len {
            pos += 1;
        }
        let result;
        if pos > 0 {
            result = self.send_tx_internal(self.data, pos);
            if result > 0 {
                self.data = unsafe { self.data.offset(pos) };
                self.len -= pos;
            }
        } else {
            let buf =  if self.escape { (unsafe { *self.data }) ^ TINY_HDLC_ESCAPE_BIT } else {TINY_HDLC_ESCAPE_CHAR};
            result = self.send_tx_internal(&buf, 1);
            if result > 0 {
                self.escape = !self.escape;
                if !self.escape {
                    self.data = unsafe { self.data.offset(1) };
                    self.len -= 1;
                }
            }
        }
        if self.len == 0 {
            self.state = Tx::send_crc;
        }
        (result, ResultT::Success)
    }

    fn send_crc(&mut self) -> (isize, ResultT) {
        let result;
        if self.len == crc::get_crc_field_size(self.crc_type) as isize {
            self.state = Tx::send_end;
            result = 0;
        }
        else {
            let mut byte: u8 = (self.crc >> self.len) as u8;
            if byte != TINY_HDLC_ESCAPE_CHAR && byte != TINY_HDLC_FLAG_SEQUENCE {
                result = self.send_tx_internal(&byte, 1);
                if result > 0 {
                    self.len += 8;
                }
            } else {
                byte = if self.escape { byte ^ TINY_HDLC_ESCAPE_BIT } else { TINY_HDLC_ESCAPE_CHAR };
                result = self.send_tx_internal(&byte, 1);
                if result > 0 {
                    self.escape = !self.escape;
                    if !self.escape {
                        self.len += 8;
                    }
                }
            }
        }
        (result, ResultT::Success)
    }

    fn send_end(&mut self) -> (isize, ResultT) {
        let result = self.send_tx_internal(&TINY_HDLC_FLAG_SEQUENCE, 1);
        if result > 0 {
            self.state = Tx::send_start;
            self.send_in_progress = false;
            self.escape = false;
            let len = unsafe { self.data.offset_from(self.origin_data) } as usize;
            (unsafe { &mut *self.tx_processor }).on_frame_send(self.origin_data, len);
        }
        (result, ResultT::Success)
    }

    fn send_tx_internal(&mut self, _data: *const u8, _len: isize) -> isize {
        let mut ptr: *const u8 = _data;
        let mut sent: isize = 0;
        let mut len: isize = _len;
        while len > 0 && self.out_buffer_len > 0 {
            len -= 1;
            unsafe { *self.out_buffer = *ptr };
            self.out_buffer_len -= 1;
            self.out_buffer = unsafe { self.out_buffer.offset(1) };
            sent += 1;
            ptr = unsafe { ptr.offset(1) };
        }
        sent
    }

    pub fn run_tx(&mut self, data: *mut u8, len: isize) -> isize {
        let mut repeated_empty_data: bool = false;
        self.out_buffer = data;
        self.out_buffer_len = len as usize;
        while self.out_buffer_len > 0 {
            let (result, _error) = (self.state)(self);
            if result < 0 {
                break;
            }
            else if result == 0 {
                if repeated_empty_data {
                    break;
                }
                repeated_empty_data = true;
            }
            else {
                repeated_empty_data = false;
            }
        }
        len - self.out_buffer_len as isize
    }

}

pub struct Hdlc {
    rx: Rx,
    tx: Tx,
}

pub struct HdlcInitT {
    pub rx_processor: *mut dyn RxProcessor,
    pub tx_processor: *mut dyn TxProcessor,
    pub buf: *mut u8,
    pub buf_size: usize,
    pub crc_type: crc::HdlcCrcT,
    pub mtu: usize,
}

impl Hdlc {
    pub fn new(init: HdlcInitT) -> Hdlc {
        Hdlc {
            rx: Rx::new(init.rx_processor, init.buf, init.buf_size, init.crc_type, init.mtu as isize),
            tx: Tx::new(init.tx_processor, init.crc_type),
        }
    }

    pub fn reset(&mut self, flags: ResultFlagsT) {
        if flags != ResultFlagsT::ResetTxOnly {
            self.rx.reset();
        }
        if flags != ResultFlagsT::HwlcLowLevelResetRxOnly {
            self.tx.reset();
        }
    }

    pub fn close(&mut self) {
        self.rx.close();
        self.tx.close();
    }

    pub fn put(&mut self, data: *const u8, len: usize) -> ResultT {
        self.tx.put(data, len)
    }
}

pub mod hdlc_ll {

    pub fn add(a: i32, b: i32) -> i32 {
        a + b
    }

}

pub fn sub(a: i32, b:i32) -> i32 {
    a - b
}

#[cfg(test)]
mod unittest {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(hdlc_ll::add(1,2), 3);
    }

    #[test]
    fn test_sub() {
        assert_eq!(sub(5,4), 1);
    }

    struct TestRxProcessor {
        closure: fn(*mut u8, usize),
        counter: usize,
    }

    impl RxProcessor for TestRxProcessor {
        fn on_frame_read(&mut self, _pdata: *mut u8, _size: usize) {
            self.counter += 1;
            (self.closure)(_pdata, _size);
        }
    }

    impl TestRxProcessor {
        fn new(closure: fn(*mut u8, usize)) -> TestRxProcessor {
            TestRxProcessor {
                closure,
                counter: 0,
            }
        }

        pub fn get_counter(&self) -> usize {
            self.counter
        }
    }

    struct TestTxProcessor {
    }

    impl TxProcessor for TestTxProcessor {
        fn on_frame_send(&mut self, _pdata: *const u8, _size: usize) {
            // println!("on_frame_send");
        }
    }

    #[test]
    fn test_hdlc_send() {
        let mut tx_processor = TestTxProcessor {};
        let mut tx = Tx::new(&mut tx_processor, crc::HdlcCrcT::HdlcCrcOff);
        let data: [u8; 4] = [0x7F, 0x7E, 0x7D, 0x00];
        tx.put(data.as_ptr(), data.len());
        let mut out_buffer: [u8; 10] = [0; 10];
        let len = tx.run_tx(out_buffer.as_mut_ptr(), out_buffer.len() as isize);
        let expected: [u8; 8] = [0x7E, 0x7F, 0x7D, 0x5E, 0x7D, 0x5D, 0x00, 0x7E];
        assert_eq!(len, expected.len() as isize, "Special bytes mismatch");
        assert_eq!(out_buffer[0..expected.len()], expected, "Arrays are not equal");

        tx.put(data.as_ptr(), data.len());
        let mut out_buf_short: [u8; 5] = [0; 5];
        let mut index = 0;
        while index < expected.len() {
            let len = tx.run_tx(out_buf_short.as_mut_ptr(), out_buf_short.len() as isize);
            let expected_len = min(expected.len() - index, out_buf_short.len());
            assert_eq!(len, expected_len as isize, "Length mismatch");
            assert_eq!(out_buf_short[0..len as usize], expected[index..index + len as usize], "Arrays are not equal");
            index += len as usize;
            if len <= 0 {
                break;
            }
        }
    }

    #[test]
    fn test_hdlc_recv() {
        let mut rx_processor = TestRxProcessor::new(|_pdata: *mut u8, _size: usize| {
            let expected: [u8; 4] = [0x7F, 0x7E, 0x7D, 0x00];
            assert_eq!(_size, expected.len(), "Special bytes mismatch");
            assert_eq!(unsafe { std::slice::from_raw_parts(_pdata, expected.len()) },
                        expected, "Arrays are not equal");
        });
        let mut buffer: [u8; 10] = [0; 10];
        let mut rx = Rx::new(&mut rx_processor, buffer.as_mut_ptr(), buffer.len(), crc::HdlcCrcT::HdlcCrcOff, 0);
        let data_from_rx: [u8; 8] = [0x7E, 0x7F, 0x7D, 0x5E, 0x7D, 0x5D, 0x00, 0x7E];
        let mut error = ResultT::Error;
        let len = rx.run_rx(data_from_rx.as_ptr(), data_from_rx.len() as isize, &mut error);
        assert_eq!(len, data_from_rx.len() as isize, "Length mismatch");
        assert_eq!(error, ResultT::Success, "Error mismatch");
        assert_eq!(rx_processor.get_counter(), 1, "Data not received");
    }

}

