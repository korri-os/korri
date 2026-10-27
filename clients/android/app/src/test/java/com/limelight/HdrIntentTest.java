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
public class HdrIntentTest {
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
    public void testHdrExtrasConstant() {
        assertEquals("Hdr", Game.EXTRA_HDR);
    }

    @Test
    public void testIntentWithHdrExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_HDR, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_HDR));
    }

    @Test
    public void testIntentWithHdrExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_HDR, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_HDR));
    }

    @Test
    public void testIntentWithNoHdrExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_HDR));
    }

    @Test
    public void testHdrOverrideLogic_True() {
        String intentHdr = "true";
        boolean enableHdr = false;

        if (intentHdr != null) {
            enableHdr = Boolean.parseBoolean(intentHdr);
        }

        assertTrue(enableHdr);
    }

    @Test
    public void testHdrOverrideLogic_False() {
        String intentHdr = "false";
        boolean enableHdr = true;  // Start with true

        if (intentHdr != null) {
            enableHdr = Boolean.parseBoolean(intentHdr);
        }

        assertFalse(enableHdr);
    }

    @Test
    public void testHdrOverrideLogic_InvalidValue() {
        String intentHdr = "invalid";
        boolean enableHdr = true;  // Start with true

        if (intentHdr != null) {
            enableHdr = Boolean.parseBoolean(intentHdr);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(enableHdr);
    }
}
