// SPDX-License-Identifier: GPL-3.0-only
//! One H.264 key frame as nightjar-core's own receiver completes it, for the
//! adapter's tests: a frame whose buffers the decoder table receives.
//!
//! The packetizer and the frame's bytes are ported from nightjar-core's
//! loopback test (`vendor/nightjar/protocol/client/tests/session_loopback.rs`,
//! `Packetizer` and `h264_idr`, GPL-3.0-only), so this file is GPL-3.0-only
//! too. Korri's adaptation (one frame into a `VideoReceiver`) is dated
//! 2026-11-17.
use crate::network as ffi;
use nightjar_core::{
    fec::ReedSolomon,
    video::{Frame, VideoConfig, VideoReceiver, VideoSignal, VideoSink},
};

const PACKET_SIZE: usize = 1024;

/// SPS, PPS and a picture, as a host sends a key frame.
fn h264_idr() -> Vec<u8> {
    let mut f = vec![
        0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1f, 0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80,
    ];
    f.extend_from_slice(&[0, 0, 0, 1, 0x65]);
    f.extend((0..3000u32).map(|i| (i % 251) as u8 | 1));
    f
}

/// The datagrams of one frame in one FEC block with 20 % parity, as the
/// host sends them: RTP header, the video packet header, the payload.
fn packetize(payload: &[u8], kind: u8, seq: u16, frame: u32) -> Vec<Vec<u8>> {
    let slice = PACKET_SIZE - 16;
    let mut data = vec![1, 0, 0, kind];
    let total = payload.len() + 8;
    let last = if total.is_multiple_of(slice) {
        slice
    } else {
        total % slice
    };
    data.extend_from_slice(&(last as u16).to_le_bytes());
    data.extend_from_slice(&[0, 0]);
    data.extend_from_slice(payload);
    let count = total.div_ceil(slice);
    let parity = (count * 20).div_ceil(100);
    let mut shards: Vec<Vec<u8>> = (0..count + parity)
        .map(|i| {
            let mut s = vec![0u8; PACKET_SIZE + 16];
            if i < count {
                s[16..20].copy_from_slice(&((i as u32) << 8).to_le_bytes());
                s[24] = 1 | if i == 0 { 4 } else { 0 } | if i == count - 1 { 2 } else { 0 };
                s[26] = 0x10;
                let chunk = &data[(i * slice).min(data.len())..((i + 1) * slice).min(data.len())];
                s[32..32 + chunk.len()].copy_from_slice(chunk);
            }
            s
        })
        .collect();
    let rs = ReedSolomon::new(count, parity).unwrap();
    let mut refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| &mut s[..]).collect();
    rs.encode(&mut refs, PACKET_SIZE + 16).unwrap();
    for (i, s) in shards.iter_mut().enumerate() {
        s[0] = 0x90;
        s[2..4].copy_from_slice(&seq.wrapping_add(i as u16).to_be_bytes());
        s[4..8].copy_from_slice(&(frame * 1500).to_be_bytes());
        s[20..24].copy_from_slice(&frame.to_le_bytes());
        let info = ((i as u32) << 12) | ((count as u32) << 22) | (20 << 4);
        s[28..32].copy_from_slice(&info.to_le_bytes());
    }
    shards
}

/// The frames a receiver completes.
#[derive(Default)]
struct Completed(Vec<Frame>);

impl VideoSink for Completed {
    fn frame(&mut self, frame: Frame) -> Result<(), Frame> {
        self.0.push(frame);
        Ok(())
    }

    fn signal(&mut self, _: VideoSignal) {}
}

/// Frame 1 of a stream from a Sunshine host: an H.264 key frame.
pub(super) fn an_h264_key_frame() -> Frame {
    let mut receiver = VideoReceiver::new(VideoConfig {
        format: ffi::VIDEO_FORMAT_H264,
        packet_size: PACKET_SIZE,
        app_version: [7, 1, 431, -1],
        rfi: false,
        pyrowave_records: false,
        colorspace: 1,
        encryption_key: None,
        early_chunks: false,
    });
    let mut completed = Completed::default();
    for datagram in packetize(&h264_idr(), 2, 100, 1) {
        receiver.receive(&datagram, 0, &mut completed);
    }
    completed
        .0
        .pop()
        .expect("the receiver completes the key frame")
}
