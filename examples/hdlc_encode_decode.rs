//! Example: HDLC low-level encode/decode
//!
//! Demonstrates using the HDLC low-level streaming API to encode and decode
//! frames byte-by-byte. This is the foundation layer of tinyproto.
//!
//! Run with: `cargo run --example hdlc_encode_decode`

use tinyproto::proto::crc::HdlcCrcT;
use tinyproto::proto::hdlc::low_level_ll::{HdlcLl, HdlcLlInit};

fn main() {
    println!("=== TinyProto HDLC Low-Level Encode/Decode Example ===\n");

    // Create encoder and decoder with CRC-16
    let mut encoder = HdlcLl::new(&HdlcLlInit {
        crc_type: HdlcCrcT::HdlcCrc16,
        mtu: 128,
        rx_buf_size: 256,
    }).expect("Failed to create encoder");

    let mut decoder = HdlcLl::new(&HdlcLlInit {
        crc_type: HdlcCrcT::HdlcCrc16,
        mtu: 128,
        rx_buf_size: 256,
    }).expect("Failed to create decoder");

    // Encode a frame
    let payload = b"Hello HDLC!";
    println!("Original payload: {:02X?} ({:?})", payload, std::str::from_utf8(payload).unwrap());

    encoder.put_frame(payload).expect("put_frame failed");

    // Stream out encoded bytes
    let mut wire_data = Vec::new();
    let mut buf = [0u8; 64];
    loop {
        let written = encoder.run_tx(&mut buf);
        if written == 0 {
            break;
        }
        wire_data.extend_from_slice(&buf[..written]);
    }
    // Consume sent frame notification
    let _ = encoder.get_tx_sent_frame();

    println!("Encoded wire data: {:02X?} ({} bytes)", wire_data, wire_data.len());
    println!("  First byte 0x7E = HDLC flag, last byte 0x7E = HDLC flag");
    println!("  Overhead: {} bytes (flags + CRC-16 + escaping)\n", wire_data.len() - payload.len());

    // Decode the frame
    let (consumed, err) = decoder.run_rx(&wire_data);
    println!("Decoder consumed: {} bytes, error: {:?}", consumed, err);

    if let Some(frame) = decoder.get_rx_frame() {
        println!("Decoded payload: {:02X?} ({:?})", frame, std::str::from_utf8(&frame).unwrap());
        assert_eq!(&frame, payload);
        println!("\nRoundtrip OK ✓");
    } else {
        println!("ERROR: No frame decoded!");
    }

    // Demonstrate CRC error detection
    println!("\n--- CRC Error Detection ---");
    let mut corrupted = wire_data.clone();
    // Corrupt a byte in the middle (skip flag bytes)
    if corrupted.len() > 4 {
        corrupted[3] ^= 0xFF;
    }
    let (_, err) = decoder.run_rx(&corrupted);
    match err {
        Some(e) => println!("Corrupted data detected: {:?} ✓", e),
        None => {
            if decoder.get_rx_frame().is_none() {
                println!("Corrupted data rejected (no valid frame) ✓");
            }
        }
    }
}
