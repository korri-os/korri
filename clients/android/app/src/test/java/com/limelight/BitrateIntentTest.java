package com.limelight;

import android.content.Context;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;

import org.junit.Before;
import org.junit.BeforeClass;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import static org.junit.Assert.*;

@Config(sdk = {33}, shadows = {
    com.limelight.shadows.ShadowMoonBridge.class,
    com.limelight.shadows.ShadowGameManager.class
})
@RunWith(RobolectricTestRunner.class)
public class BitrateIntentTest {
    private Context context;

    @BeforeClass
    public static void suppressLogs() {
        TestLogSuppressor.install();
    }

    @Before
    public void setUp() {
        context = ApplicationProvider.getApplicationContext();
    }

    @Test
    public void testBitrateExtrasConstant() {
        assertEquals("Bitrate", Game.EXTRA_BITRATE);
    }

    @Test
    public void testIntentWithBitrateExtra() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_BITRATE, 50000);

        assertEquals(50000, intent.getIntExtra(Game.EXTRA_BITRATE, -1));
    }

    @Test
    public void testIntentWithNoBitrateExtra() {
        Intent intent = new Intent(context, Game.class);

        assertEquals(-1, intent.getIntExtra(Game.EXTRA_BITRATE, -1));
    }

    @Test
    public void testBitrateOverrideLogic_ValidBitrate() {
        int intentBitrate = 50000;
        int prefBitrate = 20000;

        if (intentBitrate > 0) {
            prefBitrate = intentBitrate;
        }

        assertEquals(50000, prefBitrate);
    }

    @Test
    public void testBitrateOverrideLogic_ZeroValue() {
        int intentBitrate = 0;
        int prefBitrate = 20000;

        if (intentBitrate > 0) {
            prefBitrate = intentBitrate;
        }

        assertEquals(20000, prefBitrate);  // Should remain unchanged
    }

    @Test
    public void testBitrateOverrideLogic_NegativeValue() {
        int intentBitrate = -1;
        int prefBitrate = 20000;

        if (intentBitrate > 0) {
            prefBitrate = intentBitrate;
        }

        assertEquals(20000, prefBitrate);  // Should remain unchanged
    }
}
