// SPDX-License-Identifier: GPL-3.0-or-later
//! A sender that stops in the middle of a stream (#544). The stream loop
//! reads the network core's packet counts (`NetworkCore::received`) at each
//! turn, and `Watch` says when the video and the sound have both stopped.
//!
//! A stream that runs never stops both for long. Sunshine encodes at least
//! half the stream's frame rate when the picture does not change: its
//! `encode_run` waits at most `1000 / minimum_fps_target` ms for a new
//! image and then encodes the last one again, and `minimum_fps_target` is
//! `framerate / 2` unless configured (`src/video.cpp`). That is a frame at
//! least each 33 ms at 60 fps, and each 200 ms at 10 fps. Its sound capture
//! sends a packet each 5 ms (`x-nv-aqos.packetDuration`), silence included,
//! as its Linux capture reads the sink's monitor without a pause. The
//! longest gaps seen are a video pause of 0.1 to 3 s while a loaded host
//! makes a new encoder (#524's resolution spike); its sound runs on in that
//! time. So the watch takes both: it calls the link lost only when the video
//! and the sound have both been silent for `SILENCE`. A stream with no sound
//! is watched on its video alone.
//!
//! Before the first video packet nothing is watched: the core ends a stream
//! that never had video itself (nightjar-core's `FIRST_FRAME_TIMEOUT`, 10 s).
use korri_viewer_treaty::ViewerLinkLost;

/// How long the video and the sound must both be silent. The stream loop
/// reads the counts each 100 ms, so the link loss is seen at most 1.6 s
/// after the last packet: inside #544's 2 s.
pub const SILENCE_MS: f64 = 1500.0;

/// The packets that the core has received so far in this stream, counted
/// at its sockets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Received {
    pub video: u64,
    pub audio: u64,
}

/// One medium: when its count last changed, and its longest silence that
/// ended in this stream.
#[derive(Clone, Copy, Debug, Default)]
struct Medium {
    count: u64,
    /// CLOCK_MONOTONIC ms at which `count` last rose; none before a packet.
    last_ms: Option<f64>,
    longest_ms: f64,
}

impl Medium {
    fn observe(&mut self, count: u64, now_ms: f64) {
        if count == self.count {
            return;
        }
        if let Some(last) = self.last_ms {
            self.longest_ms = self.longest_ms.max(now_ms - last);
        }
        self.count = count;
        self.last_ms = Some(now_ms);
    }

    fn silent(&self, now_ms: f64) -> bool {
        self.last_ms.is_some_and(|last| now_ms - last >= SILENCE_MS)
    }
}

/// The watch of one stream.
#[derive(Debug, Default)]
pub struct Watch {
    video: Medium,
    audio: Medium,
}

impl Watch {
    /// The counts at `now_ms`: the link loss once the video, and the sound
    /// if any came, have been silent for `SILENCE_MS`.
    pub fn observe(&mut self, received: Received, now_ms: f64) -> Option<ViewerLinkLost> {
        self.video.observe(received.video, now_ms);
        self.audio.observe(received.audio, now_ms);
        let video_last_ms = self.video.last_ms?;
        let audio_silent = self.audio.last_ms.is_none() || self.audio.silent(now_ms);
        (self.video.silent(now_ms) && audio_silent).then_some(ViewerLinkLost {
            video_last_ms,
            audio_last_ms: self.audio.last_ms,
            detected_ms: now_ms,
        })
    }

    /// The longest silence of the video and of the sound that ended while
    /// the stream ran, in ms, as the loop's reads see it (to 100 ms). None
    /// before the first video packet.
    pub fn longest_ms(&self) -> Option<(f64, f64)> {
        self.video
            .last_ms
            .map(|_| (self.video.longest_ms, self.audio.longest_ms))
    }
}

/// The viewer's line for a link loss.
pub fn line(lost: &ViewerLinkLost) -> String {
    let audio = match lost.audio_last_ms {
        Some(at) => format!("no sound for {:.0} ms", lost.detected_ms - at),
        None => "no sound in this stream".into(),
    };
    format!(
        "link: no video for {:.0} ms and {audio}: the sender or the link to it is gone",
        lost.detected_ms - lost.video_last_ms
    )
}

/// The stream ended because its link was lost: the run's error.
#[derive(Debug)]
pub struct LinkLost(pub ViewerLinkLost);

impl std::fmt::Display for LinkLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the stream ended: {}", line(&self.0))
    }
}

impl std::error::Error for LinkLost {}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(video: u64, audio: u64) -> Received {
        Received { video, audio }
    }

    /// A stream at 60 fps whose sound comes each 5 ms, read each 100 ms
    /// from `from_ms` for `for_ms`; the counts go on from `at`.
    fn flowing(watch: &mut Watch, at: &mut Received, from_ms: f64, for_ms: f64) {
        let mut now = from_ms;
        while now < from_ms + for_ms {
            at.video += 6;
            at.audio += 20;
            assert_eq!(
                watch.observe(*at, now),
                None,
                "a flowing stream at {now} ms"
            );
            now += 100.0;
        }
    }

    #[test]
    fn a_sender_that_stops_is_seen_within_two_seconds_of_its_last_packet() {
        let mut watch = Watch::default();
        let mut at = Received::default();
        flowing(&mut watch, &mut at, 0.0, 5000.0);
        // The last packets came at 4900 ms; then nothing.
        let mut now = 5000.0;
        let lost = loop {
            if let Some(lost) = watch.observe(at, now) {
                break lost;
            }
            assert!(now < 8000.0, "never seen");
            now += 100.0;
        };
        assert_eq!(lost.video_last_ms, 4900.0);
        assert_eq!(lost.audio_last_ms, Some(4900.0));
        assert!(lost.detected_ms - 4900.0 >= SILENCE_MS);
        // The loop's reads are 100 ms apart: seen by the read after 1.5 s.
        assert!(lost.detected_ms - 4900.0 <= 2000.0, "{lost:?}");
        assert_eq!(
            line(&lost),
            "link: no video for 1500 ms and no sound for 1500 ms: the sender or the link to it is gone"
        );
    }

    #[test]
    fn a_static_picture_at_the_senders_minimum_rate_is_not_a_loss() {
        // Sunshine's floor at 10 fps: one frame each 200 ms. No sound.
        let mut watch = Watch::default();
        for step in 0..100u64 {
            let video = step / 2 + 1;
            assert_eq!(watch.observe(counts(video, 0), step as f64 * 100.0), None);
        }
    }

    #[test]
    fn a_long_video_pause_with_sound_is_not_a_loss() {
        // A loaded host makes a new encoder: 3 s with no picture, the
        // sound on (#524's resolution spike). The last frame came at 900 ms
        // and the next at 4000 ms.
        let mut watch = Watch::default();
        let mut at = Received::default();
        flowing(&mut watch, &mut at, 0.0, 1000.0);
        for step in 0..30 {
            at.audio += 20;
            assert_eq!(watch.observe(at, 1000.0 + step as f64 * 100.0), None);
        }
        flowing(&mut watch, &mut at, 4000.0, 1000.0);
        let (video, audio) = watch.longest_ms().expect("watched");
        assert_eq!(video, 3100.0);
        assert_eq!(audio, 100.0);
    }

    #[test]
    fn sound_alone_keeps_a_stream_whose_video_stopped() {
        let mut watch = Watch::default();
        let mut at = Received::default();
        flowing(&mut watch, &mut at, 0.0, 1000.0);
        for step in 0..100 {
            at.audio += 20;
            assert_eq!(watch.observe(at, 1000.0 + step as f64 * 100.0), None);
        }
    }

    #[test]
    fn a_stream_with_no_sound_is_watched_on_its_video() {
        let mut watch = Watch::default();
        assert_eq!(watch.observe(counts(5, 0), 0.0), None);
        assert_eq!(watch.observe(counts(5, 0), 1400.0), None);
        let lost = watch.observe(counts(5, 0), 1500.0).expect("lost");
        assert_eq!(lost.audio_last_ms, None);
        assert_eq!(
            line(&lost),
            "link: no video for 1500 ms and no sound in this stream: the sender or the link to it is gone"
        );
    }

    #[test]
    fn nothing_is_watched_before_the_first_video_packet() {
        // Sound alone, then silence: the core's own first-frame timeout
        // ends a stream that never had video.
        let mut watch = Watch::default();
        assert_eq!(watch.observe(counts(0, 10), 0.0), None);
        assert_eq!(watch.observe(counts(0, 10), 60_000.0), None);
        assert_eq!(watch.longest_ms(), None);
    }

    #[test]
    fn the_link_loss_is_reported_with_each_read_after_it() {
        // The loop ends at the first; a read after it says the same.
        let mut watch = Watch::default();
        watch.observe(counts(1, 1), 0.0);
        let first = watch.observe(counts(1, 1), 1500.0).expect("lost");
        let again = watch.observe(counts(1, 1), 1600.0).expect("lost");
        assert_eq!(first.video_last_ms, again.video_last_ms);
    }
}
