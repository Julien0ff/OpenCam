use crate::types::CodecType;
use crate::ProtocolError;
use std::collections::HashMap;

pub const MAGIC_VIDEO: [u8; 4] = [0x4F, 0x43, 0x4C, 0x56]; // "OCLV"
pub const MAGIC_CONTROL: [u8; 4] = [0x4F, 0x43, 0x4C, 0x43]; // "OCLC"
pub const PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_CONTROL_PORT: u16 = 52520;
pub const DEFAULT_VIDEO_PORT: u16 = 52521;
pub const MDNS_SERVICE_TYPE: &str = "_opencamlink._tcp.local.";
pub const MAX_UDP_PAYLOAD_SIZE: usize = 1400;
pub const HEADER_SIZE: usize = 32;

/// Fixed-size binary header for UDP video packets (32 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoPacketHeader {
    pub magic: [u8; 4],
    pub stream_id: u16,
    pub frame_id: u32,
    pub chunk_index: u16,
    pub chunk_count: u16,
    pub codec: CodecType,
    pub is_keyframe: bool,
    pub width: u16,
    pub height: u16,
    pub timestamp_us: u64,
    pub payload_len: u32,
}

impl VideoPacketHeader {
    pub fn write_to(&self, out: &mut [u8]) {
        assert!(out.len() >= HEADER_SIZE, "Destination buffer too small for header");
        out[0..4].copy_from_slice(&self.magic);
        out[4..6].copy_from_slice(&self.stream_id.to_be_bytes());
        out[6..10].copy_from_slice(&self.frame_id.to_be_bytes());
        out[10..12].copy_from_slice(&self.chunk_index.to_be_bytes());
        out[12..14].copy_from_slice(&self.chunk_count.to_be_bytes());
        out[14] = self.codec.to_u8();
        out[15] = if self.is_keyframe { 0x01 } else { 0x00 };
        out[16..18].copy_from_slice(&self.width.to_be_bytes());
        out[18..20].copy_from_slice(&self.height.to_be_bytes());
        out[20..28].copy_from_slice(&self.timestamp_us.to_be_bytes());
        out[28..32].copy_from_slice(&self.payload_len.to_be_bytes());
    }

    pub fn read_from(src: &[u8]) -> Result<Self, ProtocolError> {
        if src.len() < HEADER_SIZE {
            return Err(ProtocolError::BufferTooShort {
                needed: HEADER_SIZE,
                actual: src.len(),
            });
        }

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&src[0..4]);
        if magic != MAGIC_VIDEO {
            return Err(ProtocolError::InvalidMagic {
                expected: MAGIC_VIDEO,
                actual: magic,
            });
        }

        let stream_id = u16::from_be_bytes([src[4], src[5]]);
        let frame_id = u32::from_be_bytes([src[6], src[7], src[8], src[9]]);
        let chunk_index = u16::from_be_bytes([src[10], src[11]]);
        let chunk_count = u16::from_be_bytes([src[12], src[13]]);
        let codec_byte = src[14];
        let codec = CodecType::from_u8(codec_byte)
            .ok_or(ProtocolError::InvalidCodec(codec_byte))?;
        let is_keyframe = (src[15] & 0x01) != 0;
        let width = u16::from_be_bytes([src[16], src[17]]);
        let height = u16::from_be_bytes([src[18], src[19]]);
        let timestamp_us = u64::from_be_bytes([
            src[20], src[21], src[22], src[23],
            src[24], src[25], src[26], src[27],
        ]);
        let payload_len = u32::from_be_bytes([src[28], src[29], src[30], src[31]]);

        Ok(Self {
            magic,
            stream_id,
            frame_id,
            chunk_index,
            chunk_count,
            codec,
            is_keyframe,
            width,
            height,
            timestamp_us,
            payload_len,
        })
    }
}

/// Slices full video frames into individual UDP datagrams with binary headers.
pub struct Packetizer {
    max_chunk_payload: usize,
}

impl Default for Packetizer {
    fn default() -> Self {
        Self {
            max_chunk_payload: MAX_UDP_PAYLOAD_SIZE,
        }
    }
}

impl Packetizer {
    pub fn new(max_chunk_payload: usize) -> Self {
        Self { max_chunk_payload }
    }

    /// Packetizes a video frame into a series of UDP datagrams.
    pub fn packetize(
        &self,
        stream_id: u16,
        frame_id: u32,
        codec: CodecType,
        is_keyframe: bool,
        width: u16,
        height: u16,
        timestamp_us: u64,
        payload: &[u8],
    ) -> Vec<Vec<u8>> {
        if payload.is_empty() {
            return Vec::new();
        }

        let chunk_count = ((payload.len() + self.max_chunk_payload - 1) / self.max_chunk_payload) as u16;
        let mut packets = Vec::with_capacity(chunk_count as usize);

        for (i, chunk) in payload.chunks(self.max_chunk_payload).enumerate() {
            let header = VideoPacketHeader {
                magic: MAGIC_VIDEO,
                stream_id,
                frame_id,
                chunk_index: i as u16,
                chunk_count,
                codec,
                is_keyframe,
                width,
                height,
                timestamp_us,
                payload_len: chunk.len() as u32,
            };

            let mut packet = vec![0u8; HEADER_SIZE + chunk.len()];
            header.write_to(&mut packet[0..HEADER_SIZE]);
            packet[HEADER_SIZE..].copy_from_slice(chunk);
            packets.push(packet);
        }

        packets
    }
}

/// Fully reassembled video frame ready for decoding or rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ReassembledFrame {
    pub stream_id: u16,
    pub frame_id: u32,
    pub codec: CodecType,
    pub is_keyframe: bool,
    pub width: u16,
    pub height: u16,
    pub timestamp_us: u64,
    pub payload: Vec<u8>,
}

struct PartialFrame {
    header: VideoPacketHeader,
    chunks: HashMap<u16, Vec<u8>>,
    received_bytes: usize,
}

/// Dynamic zero-lag frame reassembler for incoming UDP datagrams.
/// Automatically drops stale, incomplete frames when newer frames arrive.
pub struct FrameReassembler {
    /// Maps frame_id -> PartialFrame
    pending_frames: HashMap<u32, PartialFrame>,
    latest_completed_frame_id: u32,
    /// Maximum number of active frames kept in the reassembly window (prevents memory leaks)
    window_size: u32,
}

impl Default for FrameReassembler {
    fn default() -> Self {
        Self::new(5)
    }
}

impl FrameReassembler {
    pub fn new(window_size: u32) -> Self {
        Self {
            pending_frames: HashMap::new(),
            latest_completed_frame_id: 0,
            window_size,
        }
    }

    /// Feeds an incoming raw UDP datagram. Returns `Some(ReassembledFrame)` if this packet
    /// completed a frame, or `None` if more chunks are needed or if the packet is stale.
    pub fn process_packet(&mut self, packet: &[u8]) -> Result<Option<ReassembledFrame>, ProtocolError> {
        let header = VideoPacketHeader::read_from(packet)?;
        let payload = &packet[HEADER_SIZE..];

        if payload.len() != header.payload_len as usize {
            return Err(ProtocolError::PayloadMismatch {
                expected: header.payload_len as usize,
                actual: payload.len(),
            });
        }

        // Drop packets from frames that are older than our latest completed frame
        if self.latest_completed_frame_id > 0 && header.frame_id < self.latest_completed_frame_id {
            return Ok(None);
        }

        // Purge old pending frames if frame_id advanced significantly (Drop-on-late)
        if header.frame_id > self.latest_completed_frame_id + self.window_size {
            self.pending_frames.retain(|&fid, _| fid + self.window_size >= header.frame_id);
        }

        let partial = self.pending_frames.entry(header.frame_id).or_insert_with(|| PartialFrame {
            header,
            chunks: HashMap::with_capacity(header.chunk_count as usize),
            received_bytes: 0,
        });

        if !partial.chunks.contains_key(&header.chunk_index) {
            partial.received_bytes += payload.len();
            partial.chunks.insert(header.chunk_index, payload.to_vec());
        }

        // Check if all chunks have been received
        if partial.chunks.len() == header.chunk_count as usize {
            let mut assembled = Vec::with_capacity(partial.received_bytes);
            for idx in 0..header.chunk_count {
                if let Some(chunk_bytes) = partial.chunks.get(&idx) {
                    assembled.extend_from_slice(chunk_bytes);
                } else {
                    return Ok(None); // Should not happen, but safe guard
                }
            }

            self.latest_completed_frame_id = header.frame_id;
            let completed_header = partial.header;
            self.pending_frames.remove(&header.frame_id);

            // Clean up any remaining frames with fid <= latest_completed_frame_id
            self.pending_frames.retain(|&fid, _| fid > self.latest_completed_frame_id);

            Ok(Some(ReassembledFrame {
                stream_id: completed_header.stream_id,
                frame_id: completed_header.frame_id,
                codec: completed_header.codec,
                is_keyframe: completed_header.is_keyframe,
                width: completed_header.width,
                height: completed_header.height,
                timestamp_us: completed_header.timestamp_us,
                payload: assembled,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn reset(&mut self) {
        self.pending_frames.clear();
        self.latest_completed_frame_id = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let original = VideoPacketHeader {
            magic: MAGIC_VIDEO,
            stream_id: 42,
            frame_id: 1337,
            chunk_index: 2,
            chunk_count: 5,
            codec: CodecType::H264,
            is_keyframe: true,
            width: 1920,
            height: 1080,
            timestamp_us: 9876543210,
            payload_len: 1200,
        };

        let mut buffer = [0u8; HEADER_SIZE];
        original.write_to(&mut buffer);

        let parsed = VideoPacketHeader::read_from(&buffer).expect("Should parse");
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_packetizer_and_reassembler() {
        let packetizer = Packetizer::new(500);
        let mut reassembler = FrameReassembler::default();

        let dummy_payload = vec![0xAB; 1250]; // Will split into 3 chunks (500 + 500 + 250)
        let packets = packetizer.packetize(
            1,
            100,
            CodecType::YUYV,
            false,
            640,
            480,
            123456,
            &dummy_payload,
        );

        assert_eq!(packets.len(), 3);

        // Feed first 2 packets -> no frame yet
        assert!(reassembler.process_packet(&packets[0]).unwrap().is_none());
        assert!(reassembler.process_packet(&packets[1]).unwrap().is_none());

        // Feed 3rd packet -> complete frame!
        let completed = reassembler.process_packet(&packets[2]).unwrap();
        assert!(completed.is_some());
        let frame = completed.unwrap();
        assert_eq!(frame.frame_id, 100);
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert_eq!(frame.payload, dummy_payload);
    }

    #[test]
    fn test_drop_stale_packets() {
        let packetizer = Packetizer::new(500);
        let mut reassembler = FrameReassembler::default();

        let frame1_packets = packetizer.packetize(1, 1, CodecType::H264, true, 1920, 1080, 1000, &[1u8; 800]);
        let frame2_packets = packetizer.packetize(1, 2, CodecType::H264, false, 1920, 1080, 2000, &[2u8; 400]);

        // Frame 2 completes first (frame 1 dropped or delayed)
        let completed = reassembler.process_packet(&frame2_packets[0]).unwrap();
        assert!(completed.is_some());
        assert_eq!(completed.unwrap().frame_id, 2);

        // A late packet from frame 1 arrives -> must be ignored!
        let stale = reassembler.process_packet(&frame1_packets[0]).unwrap();
        assert!(stale.is_none());
    }
}
