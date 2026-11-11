use camera_capture::create_default_capture_source;
use core_protocol::packet::{FrameReassembler, Packetizer};
use core_protocol::{CodecType, StreamConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::UdpSocket;
use vcam_sink::converter::{rgba_to_yuyv, yuyv_to_rgba};

#[tokio::test]
async fn test_full_streaming_pipeline_end_to_end() {
    // 1. Create UDP sockets for transmission and reception
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // 2. Start dynamic camera capture (real dynamic RGBA frames at 160x120 for unit testing)
    let mut capture = create_default_capture_source();
    let frames_captured = Arc::new(AtomicUsize::new(0));
    let frames_captured_clone = frames_captured.clone();

    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel(16);

    let config = StreamConfig {
        camera_id: "0".to_string(),
        width: 160,
        height: 120,
        fps: 30,
        codec: CodecType::RGBA,
        bitrate_kbps: 2000,
        udp_port: server_addr.port(),
    };

    capture
        .start(
            config,
            Box::new(move |frame| {
                frames_captured_clone.fetch_add(1, Ordering::SeqCst);
                let _ = frame_tx.try_send(frame);
            }),
        )
        .expect("Capture source should start");

    // 3. Sender task: packetizes frames into UDP datagrams
    let sender_handle = tokio::spawn(async move {
        let packetizer = Packetizer::default();
        let mut frame_id = 0u32;

        while let Some(frame) = frame_rx.recv().await {
            frame_id += 1;
            let packets = packetizer.packetize(
                1,
                frame_id,
                CodecType::RGBA,
                frame_id % 30 == 1,
                frame.width as u16,
                frame.height as u16,
                frame.timestamp_us,
                &frame.data,
            );

            for packet in packets {
                client_socket.send_to(&packet, server_addr).await.unwrap();
            }

            if frame_id >= 5 {
                break;
            }
        }
    });

    // 4. Receiver task: receives datagrams, reassembles frames, converts to YUYV and back to RGBA
    let reassembled_count = Arc::new(AtomicUsize::new(0));
    let reassembled_count_clone = reassembled_count.clone();

    let receiver_handle = tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        let mut reassembler = FrameReassembler::default();
        let mut count = 0;

        while count < 3 {
            let (len, _) = server_socket.recv_from(&mut buf).await.unwrap();
            if let Ok(Some(frame)) = reassembler.process_packet(&buf[..len]) {
                assert_eq!(frame.width, 160);
                assert_eq!(frame.height, 120);
                assert_eq!(frame.payload.len(), 160 * 120 * 4);

                // Convert RGBA -> YUYV (for Linux v4l2loopback sink)
                let mut yuyv = vec![0u8; 160 * 120 * 2];
                rgba_to_yuyv(&frame.payload, 160, 120, &mut yuyv);

                // Convert YUYV -> RGBA (for live egui preview)
                let mut restored_rgba = vec![0u8; 160 * 120 * 4];
                yuyv_to_rgba(&yuyv, 160, 120, &mut restored_rgba);

                count += 1;
                reassembled_count_clone.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    // Wait for receiver or timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            let _ = tokio::join!(sender_handle, receiver_handle);
        },
    )
    .await;

    assert!(result.is_ok(), "Streaming pipeline timed out");
    assert!(reassembled_count.load(Ordering::SeqCst) >= 3);

    capture.stop().expect("Capture should stop");
}
