package com.limelight.binding.video;

/** Encoded video delivery and actual display discards, independent of the HUD. */
public final class ViewerVideoStats {
    public static final long STALE_AFTER_MS = 3000;
    public static final class Sample {
        public final double deliveredBitrateKbps;
        public final long framesDropped;
        final long atMs;
        Sample(double bitrate, long dropped, long at) {
            deliveredBitrateKbps = bitrate; framesDropped = dropped; atMs = at;
        }
    }
    private long startMs = -1;
    private long encodedBytes;
    private long displayDrops;
    private Sample latest;

    public synchronized void encodedBytes(int bytes, long nowMs) {
        if (bytes <= 0) return;
        if (startMs < 0) startMs = nowMs;
        encodedBytes += bytes;
        long elapsed = nowMs - startMs;
        if (elapsed >= 1000) {
            latest = new Sample(encodedBytes * 8.0 / elapsed, displayDrops, nowMs);
            encodedBytes = 0;
            displayDrops = 0;
            startMs = nowMs;
        }
    }

    public synchronized void displayDropped() { displayDrops++; }

    public synchronized Sample snapshot(long nowMs) {
        return latest != null && nowMs >= latest.atMs && nowMs - latest.atMs <= STALE_AFTER_MS ? latest : null;
    }
}
