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
public class DisplayTopCenterIntentTest {
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
    public void testDisplayTopCenterExtrasConstant() {
        assertEquals("DisplayTopCenter", Game.EXTRA_DISPLAY_TOP_CENTER);
    }

    @Test
    public void testIntentWithDisplayTopCenterExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_DISPLAY_TOP_CENTER, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_DISPLAY_TOP_CENTER));
    }

    @Test
    public void testIntentWithDisplayTopCenterExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_DISPLAY_TOP_CENTER, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_DISPLAY_TOP_CENTER));
    }

    @Test
    public void testIntentWithNoDisplayTopCenterExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_DISPLAY_TOP_CENTER));
    }

    @Test
    public void testDisplayTopCenterOverrideLogic_True() {
        String intentDisplayTopCenter = "true";
        boolean alignDisplayTopCenter = false;

        if (intentDisplayTopCenter != null) {
            alignDisplayTopCenter = Boolean.parseBoolean(intentDisplayTopCenter);
        }

        assertTrue(alignDisplayTopCenter);
    }

    @Test
    public void testDisplayTopCenterOverrideLogic_False() {
        String intentDisplayTopCenter = "false";
        boolean alignDisplayTopCenter = true;  // Start with true

        if (intentDisplayTopCenter != null) {
            alignDisplayTopCenter = Boolean.parseBoolean(intentDisplayTopCenter);
        }

        assertFalse(alignDisplayTopCenter);
    }

    @Test
    public void testDisplayTopCenterOverrideLogic_InvalidValue() {
        String intentDisplayTopCenter = "invalid";
        boolean alignDisplayTopCenter = true;  // Start with true

        if (intentDisplayTopCenter != null) {
            alignDisplayTopCenter = Boolean.parseBoolean(intentDisplayTopCenter);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(alignDisplayTopCenter);
    }
}
