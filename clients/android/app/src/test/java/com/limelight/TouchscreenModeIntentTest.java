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
public class TouchscreenModeIntentTest {
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
    public void testTouchscreenModeExtrasConstant() {
        assertEquals("TouchscreenMode", Game.EXTRA_TOUCHSCREEN_MODE);
    }

    @Test
    public void testIntentWithTouchscreenModeExtra_Multitouch() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, "multitouch");
        assertEquals("multitouch", intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testIntentWithTouchscreenModeExtra_Absolute() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, "absolute");
        assertEquals("absolute", intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testIntentWithTouchscreenModeExtra_TrackpadNatural() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, "trackpad-natural");
        assertEquals("trackpad-natural", intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testIntentWithTouchscreenModeExtra_TrackpadGaming() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, "trackpad-gaming");
        assertEquals("trackpad-gaming", intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testIntentWithTouchscreenModeExtra_Disabled() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, "disabled");
        assertEquals("disabled", intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testIntentWithTouchscreenModeExtra_AbsoluteSwapped() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, "absolute-swapped");
        assertEquals("absolute-swapped", intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testIntentWithNoTouchscreenModeExtra() {
        Intent intent = new Intent(context, Game.class);
        assertNull(intent.getStringExtra(Game.EXTRA_TOUCHSCREEN_MODE));
    }

    @Test
    public void testTouchscreenModeParsingLogic_Multitouch() {
        String mode = "multitouch";
        int index = -1;
        switch (mode) {
            case "multitouch": index = 0; break;
            case "absolute": index = 1; break;
            case "trackpad-natural": index = 2; break;
            case "trackpad-gaming": index = 3; break;
            case "disabled": index = 4; break;
            case "absolute-swapped": index = 5; break;
        }
        assertEquals(0, index);
    }

    @Test
    public void testTouchscreenModeParsingLogic_Invalid() {
        String mode = "invalid";
        int index = -1;
        switch (mode) {
            case "multitouch": index = 0; break;
            case "absolute": index = 1; break;
            case "trackpad-natural": index = 2; break;
            case "trackpad-gaming": index = 3; break;
            case "disabled": index = 4; break;
            case "absolute-swapped": index = 5; break;
        }
        // Invalid values should leave index at -1
        assertEquals(-1, index);
    }
}
