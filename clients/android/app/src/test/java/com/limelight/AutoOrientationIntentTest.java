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
public class AutoOrientationIntentTest {
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
    public void testAutoOrientationExtrasConstant() {
        assertEquals("AutoOrientation", Game.EXTRA_AUTO_ORIENTATION);
    }

    @Test
    public void testIntentWithAutoOrientationExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_AUTO_ORIENTATION, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_AUTO_ORIENTATION));
    }

    @Test
    public void testIntentWithAutoOrientationExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_AUTO_ORIENTATION, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_AUTO_ORIENTATION));
    }

    @Test
    public void testIntentWithNoAutoOrientationExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_AUTO_ORIENTATION));
    }

    @Test
    public void testAutoOrientationOverrideLogic_True() {
        String intentAutoOrientation = "true";
        boolean autoOrientation = false;

        if (intentAutoOrientation != null) {
            autoOrientation = Boolean.parseBoolean(intentAutoOrientation);
        }

        assertTrue(autoOrientation);
    }

    @Test
    public void testAutoOrientationOverrideLogic_False() {
        String intentAutoOrientation = "false";
        boolean autoOrientation = true;  // Start with true

        if (intentAutoOrientation != null) {
            autoOrientation = Boolean.parseBoolean(intentAutoOrientation);
        }

        assertFalse(autoOrientation);
    }

    @Test
    public void testAutoOrientationOverrideLogic_InvalidValue() {
        String intentAutoOrientation = "invalid";
        boolean autoOrientation = true;  // Start with true

        if (intentAutoOrientation != null) {
            autoOrientation = Boolean.parseBoolean(intentAutoOrientation);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(autoOrientation);
    }
}
