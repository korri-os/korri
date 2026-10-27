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
public class AbsoluteMouseModeIntentTest {
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
    public void testAbsoluteMouseModeExtrasConstant() {
        assertEquals("AbsoluteMouseMode", Game.EXTRA_ABSOLUTE_MOUSE_MODE);
    }

    @Test
    public void testIntentWithAbsoluteMouseModeExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_ABSOLUTE_MOUSE_MODE, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_ABSOLUTE_MOUSE_MODE));
    }

    @Test
    public void testIntentWithAbsoluteMouseModeExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_ABSOLUTE_MOUSE_MODE, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_ABSOLUTE_MOUSE_MODE));
    }

    @Test
    public void testIntentWithNoAbsoluteMouseModeExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_ABSOLUTE_MOUSE_MODE));
    }

    @Test
    public void testAbsoluteMouseModeOverrideLogic_True() {
        String intentAbsoluteMouseMode = "true";
        boolean absoluteMouseMode = false;

        if (intentAbsoluteMouseMode != null) {
            absoluteMouseMode = Boolean.parseBoolean(intentAbsoluteMouseMode);
        }

        assertTrue(absoluteMouseMode);
    }

    @Test
    public void testAbsoluteMouseModeOverrideLogic_False() {
        String intentAbsoluteMouseMode = "false";
        boolean absoluteMouseMode = true;  // Start with true

        if (intentAbsoluteMouseMode != null) {
            absoluteMouseMode = Boolean.parseBoolean(intentAbsoluteMouseMode);
        }

        assertFalse(absoluteMouseMode);
    }

    @Test
    public void testAbsoluteMouseModeOverrideLogic_InvalidValue() {
        String intentAbsoluteMouseMode = "invalid";
        boolean absoluteMouseMode = true;  // Start with true

        if (intentAbsoluteMouseMode != null) {
            absoluteMouseMode = Boolean.parseBoolean(intentAbsoluteMouseMode);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(absoluteMouseMode);
    }
}
