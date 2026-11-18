// SPDX-License-Identifier: GPL-3.0-only
//! The stream's health as the gameplay overlay shows it (#434, slice D11):
//! the encoded video that arrives and the frames that the display drops,
//! over each second or more, apart from any frame record. Pure; the decoder
//! counts (`decoder.rs`) and the snapshot reads (`status.rs`).
//!
//! Ported from Korri's copy of Artemis (`clients/android` at the tag `artemis-final`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `ViewerVideoStats.java`, Korri's
//! own addition to that copy, and its test. Artemis ships under the GNU GPL
//! version 3 with no "or later" grant, so this file does too. The Rust
//! adaptation is Korri work dated 2026-11-15.

/// A sample older than this is no reading (`STALE_AFTER_MS`).
pub(crate) const STALE_AFTER_MS: u64 = 3000;

/// One measure: the encoded bitrate that arrived and the frames that the
/// display dropped over its interval, and when it ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub(crate) delivered_bitrate_kbps: f64,
    pub(crate) frames_dropped: u64,
    /// CLOCK_MONOTONIC milliseconds.
    pub(crate) at_ms: u64,
}

#[derive(Debug, Default)]
pub(crate) struct VideoStats {
    start_ms: Option<u64>,
    encoded_bytes: u64,
    display_drops: u64,
    latest: Option<Sample>,
}

impl VideoStats {
    /// A decode unit of `bytes` arrived. Once a second or more has passed
    /// since the interval began, it ends with a new sample.
    pub(crate) fn encoded(&mut self, bytes: usize, now_ms: u64) {
        if bytes == 0 {
            return;
        }
        let start = *self.start_ms.get_or_insert(now_ms);
        self.encoded_bytes += bytes as u64;
        let elapsed = now_ms.saturating_sub(start);
        if elapsed >= 1000 {
            self.latest = Some(Sample {
                delivered_bitrate_kbps: self.encoded_bytes as f64 * 8.0 / elapsed as f64,
                frames_dropped: self.display_drops,
                at_ms: now_ms,
            });
            self.encoded_bytes = 0;
            self.display_drops = 0;
            self.start_ms = Some(now_ms);
        }
    }

    /// The display dropped a decoded frame.
    pub(crate) fn dropped(&mut self) {
        self.display_drops += 1;
    }

    /// The newest sample while it is not stale. Reading it does not consume
    /// it.
    pub(crate) fn sample(&self, now_ms: u64) -> Option<Sample> {
        self.latest
            .filter(|s| now_ms >= s.at_ms && now_ms - s.at_ms <= STALE_AFTER_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ViewerVideoStatsTest.measuresEncodedBytesOverActualElapsedTimeAndCountsEachDiscard.
    #[test]
    fn measures_encoded_bytes_over_the_actual_time_and_counts_each_drop() {
        let mut stats = VideoStats::default();
        assert_eq!(stats.sample(0), None);
        stats.encoded(125_000, 0);
        stats.dropped();
        stats.dropped();
        stats.encoded(125_000, 2000);
        let sample = stats.sample(2000).unwrap();
        assert_eq!(sample.delivered_bitrate_kbps, 1000.0);
        assert_eq!(sample.frames_dropped, 2);
        // Reading a sample does not consume it.
        assert_eq!(stats.sample(2001), Some(sample));
        assert_eq!(stats.sample(5000), Some(sample));
        assert_eq!(stats.sample(5001), None);
        stats.encoded(125_000, 3000);
        assert_eq!(stats.sample(3000).unwrap().frames_dropped, 0);
        assert_eq!(stats.sample(3000).unwrap().delivered_bitrate_kbps, 1000.0);
    }

    // ViewerVideoStatsTest.idleAndInvalidInputNeverManufactureAZeroBitrate.
    #[test]
    fn idle_and_empty_input_never_make_a_zero_bitrate() {
        let mut stats = VideoStats::default();
        stats.encoded(0, 0);
        stats.encoded(0, 10_000);
        assert_eq!(stats.sample(10_000), None);
        stats.encoded(12, 10_000);
        assert_eq!(stats.sample(10_001), None);
    }

    #[test]
    fn a_sample_from_a_later_clock_is_no_reading() {
        let mut stats = VideoStats::default();
        stats.encoded(1000, 5000);
        stats.encoded(1000, 6000);
        assert!(stats.sample(6000).is_some());
        assert_eq!(stats.sample(5999), None);
    }
}
