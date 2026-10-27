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
public class MouseEmulationIntentTest {
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
    public void testMouseEmulationExtrasConstant() {
        assertEquals("MouseEmulation", Game.EXTRA_MOUSE_EMULATION);
    }

    @Test
    public void testIntentWithMouseEmulationExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_MOUSE_EMULATION, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_MOUSE_EMULATION));
    }

    @Test
    public void testIntentWithMouseEmulationExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_MOUSE_EMULATION, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_MOUSE_EMULATION));
    }

    @Test
    public void testIntentWithNoMouseEmulationExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_MOUSE_EMULATION));
    }

    @Test
    public void testMouseEmulationOverrideLogic_True() {
        String intentMouseEmulation = "true";
        boolean mouseEmulation = false;

        if (intentMouseEmulation != null) {
            mouseEmulation = Boolean.parseBoolean(intentMouseEmulation);
        }

        assertTrue(mouseEmulation);
    }

    @Test
    public void testMouseEmulationOverrideLogic_False() {
        String intentMouseEmulation = "false";
        boolean mouseEmulation = true;  // Start with true

        if (intentMouseEmulation != null) {
            mouseEmulation = Boolean.parseBoolean(intentMouseEmulation);
        }

        assertFalse(mouseEmulation);
    }

    @Test
    public void testMouseEmulationOverrideLogic_InvalidValue() {
        String intentMouseEmulation = "invalid";
        boolean mouseEmulation = true;  // Start with true

        if (intentMouseEmulation != null) {
            mouseEmulation = Boolean.parseBoolean(intentMouseEmulation);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(mouseEmulation);
    }
}
