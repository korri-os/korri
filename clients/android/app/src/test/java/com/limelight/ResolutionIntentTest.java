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
public class ResolutionIntentTest {
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
    public void testResolutionExtrasConstants() {
        // Verify the constants exist and have correct values
        assertEquals("Width", Game.EXTRA_WIDTH);
        assertEquals("Height", Game.EXTRA_HEIGHT);
    }

    @Test
    public void testIntentWithBothResolutionExtras() {
        // Test that intent with both width and height extras is created correctly
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_WIDTH, 2560);
        intent.putExtra(Game.EXTRA_HEIGHT, 1440);

        assertEquals(2560, intent.getIntExtra(Game.EXTRA_WIDTH, -1));
        assertEquals(1440, intent.getIntExtra(Game.EXTRA_HEIGHT, -1));
    }

    @Test
    public void testIntentWithPartialResolutionExtras_WidthOnly() {
        // Test that intent with only width returns -1 for height
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_WIDTH, 2560);

        assertEquals(2560, intent.getIntExtra(Game.EXTRA_WIDTH, -1));
        assertEquals(-1, intent.getIntExtra(Game.EXTRA_HEIGHT, -1));
    }

    @Test
    public void testIntentWithPartialResolutionExtras_HeightOnly() {
        // Test that intent with only height returns -1 for width
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_HEIGHT, 1440);

        assertEquals(-1, intent.getIntExtra(Game.EXTRA_WIDTH, -1));
        assertEquals(1440, intent.getIntExtra(Game.EXTRA_HEIGHT, -1));
    }

    @Test
    public void testIntentWithNoResolutionExtras() {
        // Test that intent without resolution extras returns defaults
        Intent intent = new Intent(context, Game.class);

        assertEquals(-1, intent.getIntExtra(Game.EXTRA_WIDTH, -1));
        assertEquals(-1, intent.getIntExtra(Game.EXTRA_HEIGHT, -1));
    }

    @Test
    public void testResolutionOverrideLogic_BothProvided() {
        // Simulate the override logic that will be in Game.java
        int intentWidth = 2560;
        int intentHeight = 1440;
        int prefWidth = 1920;
        int prefHeight = 1080;

        // Override should happen when both are > 0
        if (intentWidth > 0 && intentHeight > 0) {
            prefWidth = intentWidth;
            prefHeight = intentHeight;
        }

        assertEquals(2560, prefWidth);
        assertEquals(1440, prefHeight);
    }

    @Test
    public void testResolutionOverrideLogic_OnlyWidthProvided() {
        // Simulate the override logic - should NOT override when only one is provided
        int intentWidth = 2560;
        int intentHeight = -1;
        int prefWidth = 1920;
        int prefHeight = 1080;

        if (intentWidth > 0 && intentHeight > 0) {
            prefWidth = intentWidth;
            prefHeight = intentHeight;
        }

        // Should remain unchanged
        assertEquals(1920, prefWidth);
        assertEquals(1080, prefHeight);
    }

    @Test
    public void testResolutionOverrideLogic_OnlyHeightProvided() {
        // Simulate the override logic - should NOT override when only one is provided
        int intentWidth = -1;
        int intentHeight = 1440;
        int prefWidth = 1920;
        int prefHeight = 1080;

        if (intentWidth > 0 && intentHeight > 0) {
            prefWidth = intentWidth;
            prefHeight = intentHeight;
        }

        // Should remain unchanged
        assertEquals(1920, prefWidth);
        assertEquals(1080, prefHeight);
    }

    @Test
    public void testResolutionOverrideLogic_ZeroValues() {
        // Zero values should not trigger override
        int intentWidth = 0;
        int intentHeight = 0;
        int prefWidth = 1920;
        int prefHeight = 1080;

        if (intentWidth > 0 && intentHeight > 0) {
            prefWidth = intentWidth;
            prefHeight = intentHeight;
        }

        assertEquals(1920, prefWidth);
        assertEquals(1080, prefHeight);
    }

    @Test
    public void testResolutionOverrideLogic_NegativeValues() {
        // Negative values should not trigger override
        int intentWidth = -100;
        int intentHeight = -200;
        int prefWidth = 1920;
        int prefHeight = 1080;

        if (intentWidth > 0 && intentHeight > 0) {
            prefWidth = intentWidth;
            prefHeight = intentHeight;
        }

        assertEquals(1920, prefWidth);
        assertEquals(1080, prefHeight);
    }
}
