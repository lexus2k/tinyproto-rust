//! Example: Light protocol loopback
//!
//! Demonstrates sending and receiving frames using the Light protocol
//! over a simulated in-memory wire. This is the simplest protocol in
//! the tinyproto stack — no acknowledgement, no flow control.
//!
//! Run with: `cargo run --example light_loopback`

use tinyproto::proto::crc::HdlcCrcT;
use tinyproto::proto::light::{Light, LightConfig};
use std::collections::VecDeque;
use std::cell::RefCell;
use std::rc::Rc;

/// Simulated bidirectional wire using an in-memory byte queue
struct FakeWire {
    data: Rc<RefCell<VecDeque<u8>>>,
}

impl FakeWire {
    fn new() -> Self {
        FakeWire {
            data: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    fn write_fn(&self) -> Box<dyn FnMut(&[u8]) -> i32> {
        let data = self.data.clone();
        Box::new(move |buf: &[u8]| -> i32 {
            let mut d = data.borrow_mut();
            for b in buf {
                d.push_back(*b);
            }
            buf.len() as i32
        })
    }

    fn read_fn(&self) -> Box<dyn FnMut(&mut [u8]) -> i32> {
        let data = self.data.clone();
        Box::new(move |buf: &mut [u8]| -> i32 {
            let mut d = data.borrow_mut();
            if let Some(b) = d.pop_front() {
                buf[0] = b;
                1
            } else {
                0
            }
        })
    }
}

fn main() {
    println!("=== TinyProto Light Protocol Loopback Example ===\n");

    let config = LightConfig {
        crc_type: HdlcCrcT::HdlcCrc16,
        timeout_ms: 1000,
    };

    let mut sender = Light::new(&config).expect("Failed to create sender");
    let mut receiver = Light::new(&config).expect("Failed to create receiver");

    let wire = FakeWire::new();

    // Send several messages
    let messages: Vec<Vec<u8>> = vec![
        b"Hello, world!".to_vec(),
        vec![0x01, 0x02, 0x03, 0x04, 0x05],
        vec![0x7E, 0x7D, 0xFF, 0x00], // includes HDLC special bytes
    ];

    for (i, msg) in messages.iter().enumerate() {
        // Send the frame
        let bytes_sent = sender.send(msg, &mut wire.write_fn()).expect("Send failed");
        println!("Frame {}: sent {} payload bytes", i + 1, bytes_sent);

        // Receive the frame
        let received = receiver.read(&mut wire.read_fn()).expect("Read failed");
        println!("Frame {}: received {} bytes: {:02X?}", i + 1, received.len(), received);

        assert_eq!(&received, msg, "Data mismatch!");
        println!("Frame {}: OK ✓\n", i + 1);
    }

    println!("All {} frames sent and received successfully!", messages.len());
}
