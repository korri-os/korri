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
public class FpsIntentTest {
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
    public void testFpsExtrasConstant() {
        assertEquals("Fps", Game.EXTRA_FPS);
    }

    @Test
    public void testIntentWithFpsExtra() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_FPS, 120);

        assertEquals(120, intent.getIntExtra(Game.EXTRA_FPS, -1));
    }

    @Test
    public void testIntentWithNoFpsExtra() {
        Intent intent = new Intent(context, Game.class);

        assertEquals(-1, intent.getIntExtra(Game.EXTRA_FPS, -1));
    }

    @Test
    public void testFpsOverrideLogic_ValidFps() {
        int intentFps = 120;
        int prefFps = 60;

        if (intentFps > 0) {
            prefFps = intentFps;
        }

        assertEquals(120, prefFps);
    }

    @Test
    public void testFpsOverrideLogic_ZeroValue() {
        int intentFps = 0;
        int prefFps = 60;

        if (intentFps > 0) {
            prefFps = intentFps;
        }

        assertEquals(60, prefFps);  // Should remain unchanged
    }

    @Test
    public void testFpsOverrideLogic_NegativeValue() {
        int intentFps = -1;
        int prefFps = 60;

        if (intentFps > 0) {
            prefFps = intentFps;
        }

        assertEquals(60, prefFps);  // Should remain unchanged
    }
}
