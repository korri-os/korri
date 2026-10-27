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
public class PipIntentTest {
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
    public void testPipExtrasConstant() {
        assertEquals("Pip", Game.EXTRA_PIP);
    }

    @Test
    public void testIntentWithPipExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_PIP, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_PIP));
    }

    @Test
    public void testIntentWithPipExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_PIP, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_PIP));
    }

    @Test
    public void testIntentWithNoPipExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_PIP));
    }

    @Test
    public void testPipOverrideLogic_True() {
        String intentPip = "true";
        boolean enablePip = false;

        if (intentPip != null) {
            enablePip = Boolean.parseBoolean(intentPip);
        }

        assertTrue(enablePip);
    }

    @Test
    public void testPipOverrideLogic_False() {
        String intentPip = "false";
        boolean enablePip = true;  // Start with true

        if (intentPip != null) {
            enablePip = Boolean.parseBoolean(intentPip);
        }

        assertFalse(enablePip);
    }

    @Test
    public void desiredValueExplicitlyClearsAutoEnterWhenPipIsDisabled() {
        assertTrue(Game.desiredPipAutoEnter(true, true, 0));
        assertFalse(Game.desiredPipAutoEnter(false, true, 0));
        assertFalse(Game.desiredPipAutoEnter(true, false, 0));
        assertFalse(Game.desiredPipAutoEnter(true, true, 1));
    }

    @Test
    public void testPipOverrideLogic_InvalidValue() {
        String intentPip = "invalid";
        boolean enablePip = true;  // Start with true

        if (intentPip != null) {
            enablePip = Boolean.parseBoolean(intentPip);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(enablePip);
    }
}
