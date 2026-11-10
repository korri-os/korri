package com.limelight.binding.video;

import org.junit.Test;
import static org.junit.Assert.*;

public class ViewerVideoStatsTest {
    @Test public void measuresEncodedBytesOverActualElapsedTimeAndCountsEachDiscard() {
        ViewerVideoStats stats = new ViewerVideoStats();
        assertNull(stats.snapshot(0));
        stats.encodedBytes(125000, 0);
        stats.displayDropped();
        stats.displayDropped();
        stats.encodedBytes(125000, 2000);
        ViewerVideoStats.Sample sample = stats.snapshot(2000);
        assertEquals(1000.0, sample.deliveredBitrateKbps, 0.0);
        assertEquals(2, sample.framesDropped);
        // Reading a sample does not consume it, even with two surfaces.
        assertSame(sample, stats.snapshot(2001));
        assertSame(sample, stats.snapshot(5000));
        assertNull(stats.snapshot(5001));
        stats.encodedBytes(125000, 3000);
        assertEquals(0, stats.snapshot(3000).framesDropped);
        assertEquals(1000.0, stats.snapshot(3000).deliveredBitrateKbps, 0.0);
    }

    @Test public void idleAndInvalidInputNeverManufactureAZeroBitrate() {
        ViewerVideoStats stats = new ViewerVideoStats();
        stats.encodedBytes(0, 0);
        stats.encodedBytes(-1, 10000);
        assertNull(stats.snapshot(10000));
        stats.encodedBytes(12, 10000);
        assertNull(stats.snapshot(10001));
    }
}
