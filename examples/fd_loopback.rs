//! Example: Full-Duplex protocol (FD) loopback
//!
//! Demonstrates the FD protocol with connection management (SABM/UA),
//! bidirectional I-frame exchange, and acknowledgement. This is the most
//! complete protocol in tinyproto, implementing HDLC ABM mode.
//!
//! Run with: `cargo run --example fd_loopback`

use tinyproto::proto::crc::HdlcCrcT;
use tinyproto::proto::fd::protocol::{TinyFd, TinyFdConfig};
use tinyproto::proto::fd::defines::{FdMode, FD_PRIMARY_ADDR};
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== TinyProto FD Protocol Loopback Example ===\n");

    // Create primary station (addr=0)
    let mut primary = TinyFd::new(&TinyFdConfig {
        crc_type: HdlcCrcT::HdlcCrc16,
        window_frames: 4,
        mtu: 128,
        send_timeout: 1000,
        retry_timeout: 200,
        retries: 3,
        addr: 0, // primary station
        peers_count: 1,
        mode: FdMode::Abm,
        rx_buf_size: 512,
    }).expect("Failed to create primary");

    // Create secondary station (addr=1)
    let mut secondary = TinyFd::new(&TinyFdConfig {
        crc_type: HdlcCrcT::HdlcCrc16,
        window_frames: 4,
        mtu: 128,
        send_timeout: 1000,
        retry_timeout: 200,
        retries: 3,
        addr: 1, // secondary station
        peers_count: 1,
        mode: FdMode::Abm,
        rx_buf_size: 512,
    }).expect("Failed to create secondary");

    // Track received data on both sides
    let primary_received = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let secondary_received = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

    let pr = primary_received.clone();
    primary.set_on_read(move |addr, data: &[u8]| {
        println!("  Primary   <- received {} bytes from addr {}: {:02X?}", data.len(), addr, data);
        pr.lock().unwrap().push(data.to_vec());
    });

    let sr = secondary_received.clone();
    secondary.set_on_read(move |addr, data: &[u8]| {
        println!("  Secondary <- received {} bytes from addr {}: {:02X?}", data.len(), addr, data);
        sr.lock().unwrap().push(data.to_vec());
    });

    // Track connection events
    primary.set_on_connect_event(|addr, connected| {
        println!("  Primary   -- peer {} {}", addr, if connected { "CONNECTED" } else { "DISCONNECTED" });
    });
    secondary.set_on_connect_event(|addr, connected| {
        println!("  Secondary -- peer {} {}", addr, if connected { "CONNECTED" } else { "DISCONNECTED" });
    });

    let mut buf = vec![0u8; 512];

    // --- Step 1: Connection establishment ---
    println!("Step 1: Establishing connection (SABM/UA exchange)...");

    // Primary initiates connection via get_tx_data (triggers timeout → sends SABM)
    // Sleep briefly to ensure the retry timeout fires on the first call
    std::thread::sleep(std::time::Duration::from_millis(250));

    // Primary → Secondary: SABM
    let w = primary.get_tx_data(&mut buf, 0);
    println!("  Primary   -> sent {} bytes (SABM)", w);
    secondary.on_rx_data(&buf[..w]).expect("rx failed");

    // Secondary → Primary: UA
    let w = secondary.get_tx_data(&mut buf, 0);
    println!("  Secondary -> sent {} bytes (UA)", w);
    primary.on_rx_data(&buf[..w]).expect("rx failed");

    assert!(primary.get_status().is_ok(), "Primary should be connected");
    println!("  Connection established ✓\n");

    // --- Step 2: Send data primary → secondary ---
    println!("Step 2: Sending data primary → secondary...");

    let messages = vec![
        b"Hello from primary!".to_vec(),
        vec![0x01, 0x02, 0x03, 0x04, 0x05],
        b"Third frame".to_vec(),
    ];

    for msg in &messages {
        primary.send_packet(FD_PRIMARY_ADDR, msg, 0).expect("send failed");
        let w = primary.get_tx_data(&mut buf, 0);
        secondary.on_rx_data(&buf[..w]).expect("rx failed");

        // Get RR acknowledgement back
        let w = secondary.get_tx_data(&mut buf, 0);
        if w > 0 {
            primary.on_rx_data(&buf[..w]).expect("rx failed");
        }
    }

    let rx_frames = secondary_received.lock().unwrap();
    assert_eq!(rx_frames.len(), messages.len());
    for (i, msg) in messages.iter().enumerate() {
        assert_eq!(&rx_frames[i], msg);
    }
    drop(rx_frames);
    println!("  All {} frames received correctly ✓\n", messages.len());

    // --- Step 3: Send data secondary → primary ---
    println!("Step 3: Sending data secondary → primary...");

    let reply = b"Reply from secondary!".to_vec();
    secondary.send_packet(FD_PRIMARY_ADDR, &reply, 0).expect("send failed");
    let w = secondary.get_tx_data(&mut buf, 0);
    primary.on_rx_data(&buf[..w]).expect("rx failed");

    // Get RR acknowledgement
    let w = primary.get_tx_data(&mut buf, 0);
    if w > 0 {
        secondary.on_rx_data(&buf[..w]).expect("rx failed");
    }

    let rx_frames = primary_received.lock().unwrap();
    assert_eq!(rx_frames.len(), 1);
    assert_eq!(rx_frames[0], reply);
    drop(rx_frames);
    println!("  Reply received correctly ✓\n");

    // --- Step 4: Disconnect ---
    println!("Step 4: Disconnecting...");

    primary.disconnect().expect("disconnect failed");
    let w = primary.get_tx_data(&mut buf, 0);
    secondary.on_rx_data(&buf[..w]).expect("rx failed");

    // UA response
    let w = secondary.get_tx_data(&mut buf, 0);
    if w > 0 {
        primary.on_rx_data(&buf[..w]).expect("rx failed");
    }

    assert!(primary.get_status().is_err(), "Primary should be disconnected");
    println!("  Disconnected ✓\n");

    println!("=== FD Protocol loopback complete! ===");
}
